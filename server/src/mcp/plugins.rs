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
        let output = match run_mode(&self.binary, "call", Some(&args)).await {
            Ok(o) => o,
            Err(e) => return ToolResult::err(format!("工具进程启动失败: {e}")),
        };
        if !output.status.success() {
            return ToolResult::err(format!(
                "工具进程退出码 {}，stderr: {}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        match serde_json::from_slice::<ToolOutput>(&output.stdout) {
            Ok(o) => ToolResult {
                success: o.success,
                data: o.data,
                error: o.error,
            },
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
) -> Vec<(PathBuf, ToolDecl)> {
    let binaries = find_plugin_binaries(dirs);
    let mut found = Vec::new();
    for path in binaries {
        let exec = PluginExecutor { binary: path.clone() };
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
        let exec = PluginExecutor { binary: path.clone() };
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
        .register(def, Box::new(PluginExecutor { binary: binary.clone() }), Some(binary));
}

/// 热重载单个插件工具：重新探测二进制 → 覆盖登记并启用。
/// 成功 → INFO 日志；失败（无法唤起）→ ERROR 日志（需求二）。
pub async fn reload_tool(state: &AppState, name: &str) -> anyhow::Result<String> {
    let Some(path) = state.registry.plugin_path(name) else {
        bail!("工具 {name} 不是插件工具（无二进制路径）");
    };
    let exec = PluginExecutor { binary: path.clone() };
    match exec.probe_decl().await {
        Ok(decl) => {
            let new_name = decl.name.clone();
            register_plugin(state, path, decl);
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
