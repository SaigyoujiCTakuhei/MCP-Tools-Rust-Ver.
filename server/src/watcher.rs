//! 源码监听器 — 开发态热编译 + 自动热装载（tools.watch: true 时随服务器挂载）
//!
//! 职责：轮询 workspace 各成员的 src 树，检测到源码变更后：
//!   1. 按变更所属成员执行 `cargo build -p <成员包>`（增量，通常数秒）
//!   2. 编译成功 → 将该成员的全部 kzm-* 产物重新探测并热装载进注册表
//!   3. 编译失败 → ERROR 日志输出 stderr 尾部，不装载（保留旧版本继续服务）
//!
//! 生命周期：随服务器启动而挂载、随进程退出而终止（子进程 kill_on_drop，
//! 不会残留孤儿编译）；启停均写入 WebUI 日志。
//!
//! 注意：这是开发态设施——发布部署（无源码树）时保持 tools.watch: false。

use anyhow::{anyhow, Context};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::mcp::handler::{AppState, LogSystem};

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const DEBOUNCE: Duration = Duration::from_millis(800);

struct Member {
    /// 成员目录（workspace 根的相对目录的绝对形式）
    dir: PathBuf,
    /// [package] name（cargo build -p 用）
    package: String,
    /// 成员内声明的 kzm-* 工具二进制：name → 源码路径（[[bin]] path，构建目标映射用）
    tools: BTreeMap<String, PathBuf>,
    /// 产物目录（cargo build 的输出位置，如 target/debug）
    target_dir: PathBuf,
}

impl Member {
    /// 成员内全部 kzm-* 产物的绝对路径（热装载用）
    fn bin_paths(&self) -> Vec<PathBuf> {
        self.tools.keys().map(|n| self.target_dir.join(n)).collect()
    }
}

/// 从 workspace 根解析出所有"含 kzm-* 工具"的成员
fn parse_workspace(root: &Path) -> anyhow::Result<Vec<Member>> {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml"))
        .with_context(|| format!("读取 workspace 清单失败: {}", root.join("Cargo.toml").display()))?;
    let members: Vec<String> = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("members"))
        .and_then(|l| {
            let inner = l.trim_start()["members".len()..].trim();
            let inner = inner.strip_prefix('=')?.trim();
            let inner = inner.strip_prefix('[')?.strip_suffix(']')?;
            Some(
                inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            )
        })
        .ok_or_else(|| anyhow!("Cargo.toml 中未找到 members 列表"))?;

    let mut out = Vec::new();
    for m in members {
        let dir = root.join(&m);
        let mt = match std::fs::read_to_string(dir.join("Cargo.toml")) {
            Ok(t) => t,
            Err(_) => continue,
        };
        // [package] name 是清单中第一个 name = 行
        let package = mt
            .lines()
            .find_map(|l| {
                let l = l.trim_start();
                l.starts_with("name")
                    .then(|| l.split('"').nth(1).map(str::to_string))
                    .flatten()
            })
            .ok_or_else(|| anyhow!("成员 {m} 缺少 package name"))?;

        // [[bin]] 块：按块整体解析（name + path），仅收集 kzm-* 前缀的工具二进制
        let mut tools = BTreeMap::new();
        for block in mt.split("[[bin]]").skip(1) {
            let get = |key: &str| -> Option<String> {
                block.lines().find_map(|l| {
                    let l = l.trim_start();
                    l.starts_with(key)
                        .then(|| l.split('"').nth(1).map(str::to_string))
                        .flatten()
                })
            };
            if let (Some(name), Some(rel)) = (get("name"), get("path")) {
                if name.starts_with("kzm-") {
                    tools.insert(name, dir.join(rel));
                }
            }
        }
        if !tools.is_empty() {
            out.push(Member {
                dir,
                package,
                tools,
                target_dir: root.join("target").join("debug"),
            });
        }
    }
    Ok(out)
}

/// 递归收集成员 src 树下全部 .rs 文件的 mtime 快照
fn scan_mtimes(member: &Member) -> BTreeMap<PathBuf, u64> {
    let mut out = BTreeMap::new();
    let src = member.dir.join("src");
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, u64>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
                let mtime = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                out.insert(p, mtime);
            }
        }
    }
    walk(&src, &mut out);
    out
}

/// 快照对比 → 变更文件集合（新增/修改/删除）
fn diff_files(
    before: &BTreeMap<PathBuf, u64>,
    after: &BTreeMap<PathBuf, u64>,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for (p, t) in after {
        if before.get(p) != Some(t) {
            files.push(p.clone());
        }
    }
    for p in before.keys() {
        if !after.contains_key(p) {
            files.push(p.clone());
        }
    }
    files
}

/// 挂载源码监听：立即校验 workspace 可解析，监听循环在后台任务中运行
pub fn spawn(
    state: AppState,
    root: PathBuf,
    logs: Arc<LogSystem>,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    parse_workspace(&root)?; // 清单问题在挂载时即刻暴露
    Ok(tokio::spawn(async move { run(state, root, logs).await }))
}

pub async fn run(state: AppState, root: PathBuf, logs: Arc<LogSystem>) {
    let members = match parse_workspace(&root) {
        Ok(m) if !m.is_empty() => m,
        Ok(_) => {
            warn!("源码监听：workspace 中未发现含 kzm-* 工具的成员，监听器退出");
            return;
        }
        Err(e) => {
            warn!("源码监听启动失败: {e:#}");
            return;
        }
    };
    let files: usize = members.iter().map(|m| scan_mtimes(m).len()).sum();
    logs
        .log(
            "INFO",
            format!(
                "👀 源码监听已挂载: {} 个成员 / {} 个源文件（保存后自动编译并热装载；发布部署请设 tools.watch: false）",
                members.len(),
                files
            ),
        )
        .await;
    info!(
        members = members.len(),
        files = files,
        "源码监听已挂载（tools.watch）"
    );

    // 每个成员独立的 mtime 快照（按包名隔离，避免跨成员误报）
    let mut snapshots: HashMap<String, BTreeMap<PathBuf, u64>> = members
        .iter()
        .map(|m| (m.package.clone(), scan_mtimes(m)))
        .collect();
    let mut pending: BTreeSet<String> = BTreeSet::new();
    let mut last_change = Instant::now();

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        for m in &members {
            let mt = scan_mtimes(m);
            let before = snapshots.get(&m.package).cloned().unwrap_or_default();
            if !diff_files(&before, &mt).is_empty() {
                snapshots.insert(m.package.clone(), mt);
                pending.insert(m.package.clone());
                any_change_marker(&mut last_change);
            }
        }
        if pending.is_empty() || last_change.elapsed() < DEBOUNCE {
            continue;
        }

        // 防抖窗口已过：逐成员编译并热装载
        for pkg in pending.clone() {
            let Some(member) = members.iter().find(|m| m.package == pkg) else { continue };
            logs
                .log("INFO", format!("🔨 检测到源码变更: {pkg}（正在编译…）"))
                .await;
            let output = tokio::process::Command::new("cargo")
                .args(["build", "-p", &pkg])
                .current_dir(&root)
                .kill_on_drop(true)
                .output()
                .await;
            match output {
                Ok(o) if o.status.success() => {
                    let mut loaded = Vec::new();
                    for bin_path in member.bin_paths() {
                        match crate::mcp::plugins::reload_binary(&state, &bin_path).await {
                            Ok(tool_name) => loaded.push(tool_name),
                            Err(e) => {
                                logs
                                    .log(
                                        "ERROR",
                                        format!(
                                            "❌ {} 编译成功但热装载失败: {e:#}",
                                            bin_path.display()
                                        ),
                                    )
                                    .await;
                            }
                        }
                    }
                    logs
                        .log(
                            "INFO",
                            format!("✅ {pkg} 重编译完成，已热装载: {}", loaded.join(", ")),
                        )
                        .await;
                }
                Ok(o) => {
                    let tail: String = String::from_utf8_lossy(&o.stderr)
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(|l| format!("  {l}\n"))
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .take(3)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    logs
                        .log(
                            "ERROR",
                            format!("❌ {pkg} 编译失败（未热装载，旧版本继续服务）:\n{tail}"),
                        )
                        .await;
                }
                Err(e) => {
                    logs.log("ERROR", format!("❌ {pkg} 编译进程启动失败: {e}")).await;
                }
            }
        }
        pending.clear();
    }
}

/// 防抖计时的小助手（仅为可读性）
fn any_change_marker(last_change: &mut Instant) {
    *last_change = Instant::now();
}
