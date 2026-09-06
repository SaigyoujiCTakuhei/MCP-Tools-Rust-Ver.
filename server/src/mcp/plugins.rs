/// 子进程插件 — 发现、探测、执行与热重载
///
/// 契约（见 tool_kit）：`kzm-*` 可执行文件
/// - `<bin> decl` → stdout 一行 ToolDecl JSON
/// - `<bin> call` → stdin 读 JSON 参数，stdout 输出 ToolOutput JSON
///
/// 设计要点：
/// - 每次调用独立子进程：工具崩溃只影响该次调用；Windows 下无 DLL 文件锁，
///   「改动 → 重新编译 → 重载」语义最干净（二进制落盘即生效）
/// - kill_on_drop：服务器端超时会连带杀掉子进程
/// - 加载/重载失败一律写 ERROR 日志（需求二：工具无法唤起要可见）
use anyhow::{anyhow, bail, Context};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::info;

use crate::executor::{ToolExecutor, ToolResult};
use crate::mcp::handler::AppState;
use crate::registry::tool_definition::{ToolAnnotations, ToolDefinition};
use tool_kit::{ToolDecl, ToolOutput};

/// 插件二进制文件名前缀（发现规则）
pub const PLUGIN_PREFIX: &str = "kzm-";

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// 子进程插件执行器
pub struct PluginExecutor {
    pub binary: PathBuf,
    /// 任务登记表（进展流广播）
    pub tasks: Arc<crate::tasks::TaskRegistry>,
    /// 显示名（decl.name，任务页/日志用）
    pub label: String,
}

impl PluginExecutor {
    /// 探测工具定义（运行 decl 子命令，5 秒超时）
    pub async fn probe_decl(&self) -> anyhow::Result<ToolDecl> {
        let output = tokio::time::timeout(PROBE_TIMEOUT, run_mode(&self.binary, "decl", None))
            .await
            .map_err(|_| anyhow!("探测超时（{} 秒）", PROBE_TIMEOUT.as_secs()))??;
        if !output.status.success() {
            bail!(
                "decl 退出码 {}，stderr: {}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        serde_json::from_slice(&output.stdout).context("decl 输出不是合法的 ToolDecl JSON")
    }
}

#[async_trait::async_trait]
impl ToolExecutor for PluginExecutor {
    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        use tokio::io::AsyncReadExt;

        let args_preview = {
            let a = serde_json::to_string(&args).unwrap_or_default();
            if a.chars().count() > 200 { format!("{}…", a.chars().take(200).collect::<String>()) } else { a }
        };
        let task_id = self.tasks.start(&self.label, &args_preview);
        self.tasks.spawn_watchdog(&task_id);

        // 建子进程（stdout=结果协议，缓冲；stderr=进展流，实时转发）
        let mut child = match Command::new(&self.binary)
            .arg("call")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                self.tasks.mark_killed(&task_id, "进程启动失败");
                return ToolResult::err(format!("工具进程启动失败: {e}"));
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            let payload = serde_json::to_string(&args).unwrap_or_default();
            let _ = stdin.write_all(payload.as_bytes()).await;
            let _ = stdin.write_all(b"\n").await;
        }

        // stderr 进展流：逐段（\n / \r 分隔）实时广播；同时收集全文备用
        let (se_tx, mut se_rx) = tokio::sync::mpsc::channel::<String>(16);
        if let Some(stderr) = child.stderr.take() {
            let tasks = self.tasks.clone();
            let tid = task_id.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 8192];
                let mut acc: Vec<u8> = Vec::new();
                let mut reader = stderr;
                loop {
                    match reader.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            acc.extend_from_slice(&buf[..n]);
                            while let Some(pos) = acc.iter().position(|&b| b == b'\n' || b == b'\r') {
                                let seg: Vec<u8> = acc.drain(..=pos).collect();
                                let text = String::from_utf8_lossy(&seg[..seg.len() - 1])
                                    .trim_end_matches('\r')
                                    .to_string();
                                if !text.trim().is_empty() {
                                    tasks.line(&tid, "stderr", &text);
                                }
                            }
                        }
                    }
                }
                if !acc.is_empty() {
                    let text = String::from_utf8_lossy(&acc).trim().to_string();
                    if !text.is_empty() {
                        tasks.line(&tid, "stderr", &text);
                    }
                }
                let _ = se_tx.send(String::from_utf8_lossy(&acc).to_string());
            });
        } // 无 stderr 时 drop 发送端（接收端即收到关闭）

        // stdout：结果协议（ToolOutput JSON），缓冲至进程结束
        let stdout_task = match child.stdout.take() {
            Some(stdout) => {
                let mut reader = stdout;
                Some(tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let _ = reader.read_to_end(&mut buf).await;
                    buf
                }))
            }
            None => None,
        };

        // 等子进程退出（stderr 读任务独立收集，不受影响）
        let started = std::time::Instant::now();
        let status = match child.wait().await {
            Ok(s) => s,
            Err(e) => {
                self.tasks.mark_killed(&task_id, &format!("wait 失败: {e}"));
                return ToolResult::err(format!("等待工具进程失败: {e}"));
            }
        };
        let stdout_buf = match stdout_task {
            Some(h) => h.await.unwrap_or_default(),
            None => Vec::new(),
        };
        let stderr_text = se_rx.recv().await.unwrap_or_default();

        self.tasks.finish(
            &task_id,
            status.success(),
            status.code().map(|c| c as i64),
            started.elapsed().as_secs_f64(),
            String::from_utf8_lossy(&stdout_buf).to_string(),
            stderr_text.clone(),
        );

        if !status.success() {
            return ToolResult::err(format!(
                "工具进程退出码 {}，stderr: {}",
                status.code().unwrap_or(-1),
                stderr_text.trim()
            ));
        }
        match serde_json::from_slice::<ToolOutput>(&stdout_buf) {
            Ok(o) => ToolResult { success: o.success, data: o.data, error: o.error },
            Err(e) => ToolResult::err(format!("工具输出不是合法 ToolOutput JSON: {e}")),
        }
    }
}

/// 运行插件子命令：mode = "decl" | "call"；call 时把参数 JSON 写入 stdin
async fn run_mode(
    binary: &Path,
    mode: &str,
    stdin_json: Option<&serde_json::Value>,
) -> anyhow::Result<std::process::Output> {
    let mut child = Command::new(binary)
        .arg(mode)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("启动工具二进制失败: {}", binary.display()))?;
    if let Some(json) = stdin_json {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(serde_json::to_string(json)?.as_bytes())
                .await
                .with_context(|| "写入工具参数失败")?;
        }
        // drop(stdin) 关闭管道，子进程 read_to_string 正常返回
    }
    let output = child
        .wait_with_output()
        .await
        .with_context(|| "等待工具进程结束失败")?;
    Ok(output)
}

/// 从目录列表发现插件：文件名以 kzm- 开头的可执行文件（去重，先到先得）
fn find_plugin_binaries(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut binaries = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with(PLUGIN_PREFIX) || !path.is_file() {
                continue;
            }
            #[cfg(windows)]
            let executable = name.to_ascii_lowercase().ends_with(".exe");
            #[cfg(not(windows))]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                path.metadata()
                    .map(|m| m.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            };
            if !executable {
                continue;
            }
            let key = name.trim_end_matches(".exe").to_string();
            if seen.insert(key) {
                binaries.push(path);
            }
        }
    }
    binaries.sort();
    binaries
}

/// 发现并探测所有插件。返回 (二进制路径, 工具定义) 列表；
/// 探测失败的条目一律写 ERROR 日志（需求二：工具无法唤起必须可见）。
pub async fn discover(
    dirs: &[PathBuf],
    logs: &crate::mcp::handler::LogSystem,
    tasks: &std::sync::Arc<crate::tasks::TaskRegistry>,
) -> Vec<(PathBuf, ToolDecl)> {
    let binaries = find_plugin_binaries(dirs);
    let mut found = Vec::new();
    for path in binaries {
        let exec = PluginExecutor { binary: path.clone(), tasks: tasks.clone(), label: path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string() };
        match exec.probe_decl().await {
            Ok(decl) => {
                info!(tool = %decl.name, binary = %path.display(), "插件工具已发现");
                found.push((path, decl));
            }
            Err(e) => {
                logs.log(
                    "ERROR",
                    format!("工具加载失败 {}: {e}", path.display()),
                )
                .await;
            }
        }
    }
    found
}

/// 扫描发现目录，登记「新增」的插件（已登记的路径跳过，不影响现有工具）。
/// 返回新登记的工具名列表；探测失败的新文件写 ERROR 日志。
/// 注册中心会发出 Registered 事件 → 订阅客户端收到 notifications/tools/list_changed。
pub async fn rescan_new_tools(state: &AppState) -> anyhow::Result<Vec<String>> {
    let binaries = find_plugin_binaries(&state.discovery_dirs);
    let mut added = Vec::new();
    for path in binaries {
        if state.registry.has_plugin_path(&path) {
            continue; // 已登记（无论启用与否）——重载走 per-tool reload 接口
        }
        let exec = PluginExecutor { binary: path.clone(), tasks: state.tasks.clone(), label: path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string() };
        match exec.probe_decl().await {
            Ok(decl) => {
                let tool_name = decl.name.clone();
                if state.registry.get(&tool_name).is_some() {
                    state
                        .logs
                        .log(
                            "WARN",
                            format!(
                                "跳过插件 {}：工具名 {} 与现有工具冲突",
                                path.display(),
                                tool_name
                            ),
                        )
                        .await;
                    continue;
                }
                register_plugin(state, path, decl);
                added.push(tool_name);
            }
            Err(e) => {
                state
                    .logs
                    .log(
                        "ERROR",
                        format!("发现新插件但无法唤起 {}: {e}", path.display()),
                    )
                    .await;
            }
        }
    }
    Ok(added)
}

/// 把探测到的 ToolDecl 登记进注册中心（覆盖同名旧条目并重新启用）
pub fn register_plugin(state: &AppState, binary: PathBuf, decl: ToolDecl) {
    let def = ToolDefinition {
        name: decl.name.clone(),
        title: decl.title,
        description: decl.description,
        input_schema: decl.input_schema,
        category: decl.category,
        annotations: decl.annotations.map(|a| ToolAnnotations {
            read_only_hint: a.read_only_hint,
            destructive_hint: a.destructive_hint,
            idempotent_hint: a.idempotent_hint,
            open_world_hint: a.open_world_hint,
        }),
        enabled: true,
        source: crate::registry::tool_definition::ToolSource::Plugin,
    };
    state
        .registry
        .register(
            def,
            Box::new(PluginExecutor {
                binary: binary.clone(),
                tasks: state.tasks.clone(),
                label: binary.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string(),
            }),
            Some(binary),
        );
}

/// 热装载二进制：重新探测产物 → 覆盖登记。
/// restore_state=true 时保留该工具既有的启用/禁用状态（源码监听的后台装载用，
/// 不悄悄重新启用用户手动卸载的工具）；false 时恢复为启用（⟳ 重载按钮语义）。
pub async fn reload_binary(
    state: &AppState,
    binary: &Path,
    restore_state: bool,
) -> anyhow::Result<String> {
    let exec = PluginExecutor { binary: binary.to_path_buf(), tasks: state.tasks.clone(), label: "?".into() };
    let decl = exec
        .probe_decl()
        .await
        .with_context(|| format!("探测工具二进制失败: {}", binary.display()))?;
    let name = decl.name.clone();
    let prev_enabled = state
        .registry
        .get(&name)
        .and_then(|e| e.definition.read().ok().map(|d| d.enabled));
    register_plugin(state, binary.to_path_buf(), decl);
    if restore_state && prev_enabled == Some(false) {
        state.registry.toggle(&name);
        state
            .logs
            .log("INFO", format!("工具 {name} 已热装载（保持禁用状态）"))
            .await;
    }
    Ok(name)
}

/// 热重载单个插件工具：重新探测二进制 → 覆盖登记并启用。
/// 成功 → INFO 日志；失败（无法唤起）→ ERROR 日志（需求二）。
pub async fn reload_tool(state: &AppState, name: &str) -> anyhow::Result<String> {
    let Some(path) = state.registry.plugin_path(name) else {
        bail!("工具 {name} 不是插件工具（无二进制路径）");
    };
    match reload_binary(state, &path, false).await {
        Ok(new_name) => {
            state
                .logs
                .log("INFO", format!("工具 {name} 已从磁盘重载（当前名: {new_name}）"))
                .await;
            Ok(new_name)
        }
        Err(e) => {
            state
                .logs
                .log("ERROR", format!("工具 {name} 重载失败（无法唤起）: {e}"))
                .await;
            bail!("工具 {name} 重载失败: {e}");
        }
    }
}
