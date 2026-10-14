//! 运行中任务注册表 — 工具子进程的实时进展可见性
//!
//! 每次插件工具调用（有子进程的）在此登记：
//!   start → 若干 line（stderr 进展流，实时广播）→ finish（退出码/耗时/完整输出）
//!
//! 事件经 broadcast 通道推送，Dashboard「⚡ 任务」页签经 /api/tasks/stream 订阅；
//! /api/tasks 返回快照。另带看门狗：超过工具超时 + 5 秒仍标 running 的记录
//! 标记为「已终止」（对应 tools_call 超时强杀子进程的路径）。

use chrono::Local;
use dashmap::DashMap;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize)]
pub struct TaskRecord {
    pub id: String,
    pub tool: String,
    pub args: String,
    #[serde(rename = "startedAt")]
    pub started_at: String,
    /// running | ok | failed | killed
    pub status: String,
    #[serde(rename = "exitCode", skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    #[serde(rename = "elapsedSecs", skip_serializing_if = "Option::is_none")]
    pub elapsed: Option<f64>,
    /// 完整 stdout（仅结束时保留，尾部 512KB）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    /// 完整 stderr（仅结束时保留，尾部 512KB）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

pub struct TaskRegistry {
    records: DashMap<String, TaskRecord>,
    events: broadcast::Sender<Value>,
    tool_timeout: u64,
    seq: AtomicU64,
    /// 插入顺序（供裁剪最旧的已完成记录）
    order: Mutex<Vec<String>>,
}

impl TaskRegistry {
    pub fn new(tool_timeout: u64) -> Self {
        let (events, _) = broadcast::channel(2048);
        Self {
            records: DashMap::new(),
            events,
            tool_timeout,
            seq: AtomicU64::new(0),
            order: Mutex::new(Vec::new()),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }

    fn emit(&self, v: Value) {
        let _ = self.events.send(v);
    }

    fn next_id(&self) -> String {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("t{millis:x}-{seq}")
    }

    /// 登记一个开始运行的任务，返回任务 id。附带看门狗：超时 + 5 秒仍未结束 → 标记「已终止」。
    pub fn start(&self, tool: &str, args: &str) -> String {
        let id = self.next_id();
        let record = TaskRecord {
            id: id.clone(),
            tool: tool.to_string(),
            args: args.to_string(),
            started_at: Local::now().format("%H:%M:%S").to_string(),
            status: "running".into(),
            exit_code: None,
            elapsed: None,
            stdout: None,
            stderr: None,
        };
        self.records.insert(id.clone(), record);
        self.order.lock().unwrap().push(id.clone());
        self.prune();
        self.emit(json!({
            "type": "start", "id": id, "tool": tool, "args": args,
        }));
        id
    }

    /// 看门狗（由调用方以 Arc<TaskRegistry> 克隆后 spawn）：
    /// 超时 + 5 秒仍 running → 标记「已终止」并广播 exit
    pub fn spawn_watchdog(self: &Arc<Self>, id: &str) {
        let timeout = self.tool_timeout + 5;
        let this = Arc::clone(self);
        let id = id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(timeout)).await;
            if let Some(mut r) = this.records.get_mut(&id) {
                if r.status == "running" {
                    r.status = "killed".into();
                    r.exit_code = None;
                    r.elapsed = Some(timeout as f64);
                    drop(r);
                    this.emit(json!({
                        "type": "exit", "id": id, "ok": false,
                        "code": Value::Null, "elapsed": timeout as f64,
                        "note": "已终止",
                    }));
                }
            }
        });
    }

    /// 推送一行进展（stderr 流，实时）
    pub fn line(&self, id: &str, stream: &str, text: &str) {
        self.emit(json!({ "type": "line", "id": id, "stream": stream, "text": text }));
    }

    /// 标记任务为「已终止」（进程启动失败等无法归类的路径）
    pub fn mark_killed(&self, id: &str, note: &str) {
        if let Some(mut r) = self.records.get_mut(id) {
            if r.status != "running" {
                return;
            }
            r.status = "killed".into();
            r.elapsed = Some(0.0);
            drop(r);
            self.emit(json!({
                "type": "exit", "id": id, "ok": false, "code": Value::Null,
                "elapsed": 0.0, "note": note,
            }));
        }
    }

    /// 结束任务：记录退出码/耗时/完整输出（尾部 512KB），广播 exit 事件
    pub fn finish(
        &self,
        id: &str,
        ok: bool,
        exit_code: Option<i64>,
        elapsed: f64,
        stdout: String,
        stderr: String,
    ) {
        if let Some(mut r) = self.records.get_mut(id) {
            if r.status != "running" {
                return; // 看门狗已标记
            }
            r.status = if ok { "ok".into() } else { "failed".into() };
            r.exit_code = exit_code;
            r.elapsed = Some(elapsed);
            r.stdout = Some(cap_tail(&stdout, 512 * 1024));
            r.stderr = Some(cap_tail(&stderr, 512 * 1024));
            drop(r);
            self.emit(json!({
                "type": "exit", "id": id, "ok": ok,
                "code": exit_code, "elapsed": elapsed,
            }));
        }
    }

    /// 快照：running 在前，其余按登记顺序
    pub fn snapshot(&self) -> Vec<TaskRecord> {
        let order = self.order.lock().unwrap();
        let mut all: Vec<TaskRecord> = self.records.iter().map(|r| r.clone()).collect();
        all.sort_by_key(|r| {
            let pos = order.iter().position(|i| i == &r.id).unwrap_or(usize::MAX);
            let running = if r.status == "running" { 0 } else { 1 };
            (running, std::cmp::Reverse(pos))
        });
        all
    }

    /// 裁剪：超过 100 条时移除最旧的已完成记录
    fn prune(&self) {
        if self.records.len() <= 100 {
            return;
        }
        let mut order = self.order.lock().unwrap();
        let mut removed_ids: Vec<String> = Vec::new();
        let excess = self.records.len() - 100;
        for id in order.iter() {
            if removed_ids.len() >= excess {
                break;
            }
            // ⚠️ 禁止在 get() 的读守卫存活期间调用 remove()/get_mut()——dashmap
            // 同分片读写锁不可重入，守卫在手再取写锁 = 等自己释放 = 永久自死锁。
            // 10-13 僵死事故根因：破百后首次 prune 在 remove 上冻死一个 worker，
            // 该 worker 连带 tokio I/O 驱动器，accept/SSE/定时器全停（gdb 栈实证
            // tasks.rs:198 ← start() ← tools_call）。先取状态、丢守卫、再删。
            let finished = self
                .records
                .get(id)
                .map(|r| r.status != "running")
                .unwrap_or(false);
            if finished {
                removed_ids.push(id.clone());
            }
        }
        for id in &removed_ids {
            self.records.remove(id);
        }
        // order 同步收缩（旧实现只删 records，order 里的陈旧 id 只增不减）
        if !removed_ids.is_empty() {
            let gone: std::collections::HashSet<&str> =
                removed_ids.iter().map(|s| s.as_str()).collect();
            order.retain(|i| !gone.contains(i.as_str()));
        }
    }
}

fn cap_tail(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let cut: String = s.chars().skip(n - max).collect();
    format!("…（前段已省略）\n{cut}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10-13 僵死事故回归测试：prune() 曾在 get() 读守卫存活期间调 remove()，
    /// dashmap 同分片自死锁——记录数破百且最旧记录已完成时必现（修复前本测试原地挂死）。
    #[test]
    fn prune_after_100_must_not_deadlock() {
        let reg = TaskRegistry::new(300);
        let mut done_ids = Vec::new();
        // 60 条已完成 + 45 条 running = 105 条；第 101 次 start 内部触发 prune
        for i in 0..60 {
            let id = reg.start(&format!("t{i}"), "{}");
            reg.finish(&id, true, Some(0), 0.1, String::new(), String::new());
            done_ids.push(id);
        }
        for i in 0..45 {
            reg.start(&format!("r{i}"), "{}");
        }
        // prune 挂在 start() 内：长度一破百立刻被裁回 100（生产上同理，records 恒 ≤100）
        assert_eq!(reg.records.len(), 100);
        reg.start("trigger", "{}");
        // start 先插入（101）再 prune：裁掉 1 条最旧已完成后仍回到 100
        assert_eq!(reg.records.len(), 100, "应裁掉 1 条最旧已完成记录");
        assert!(reg.records.get(&done_ids[0]).is_none(), "最旧已完成记录应被裁掉");
        assert!(reg.records.get(&done_ids[59]).is_some());
        // order 表同步收缩，不残留陈旧 id
        assert_eq!(reg.order.lock().unwrap().len(), 100);
    }
}
