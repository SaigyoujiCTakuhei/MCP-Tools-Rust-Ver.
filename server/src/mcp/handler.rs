/// MCP 协议核心层 — JSON-RPC 方法处理（规范版本 2026-07-28，即 "MCP 2.0"）
///
/// 设计对照 https://modelcontextprotocol.io/specification/2026-07-28 ：
/// - 无握手：不实现 initialize / notifications/initialized；版本与能力随每个请求的 `_meta` 携带
/// - `_meta` 键名必须使用保留前缀 `io.modelcontextprotocol/`
///   （REQUIRED：`io.modelcontextprotocol/protocolVersion`、`io.modelcontextprotocol/clientCapabilities`）
/// - 每个成功 result 必须携带 `resultType`（本项目恒为 "complete"），
///   并 SHOULD 在 `result._meta["io.modelcontextprotocol/serverInfo"]` 中自报身份
/// - 错误模型：未知工具/参数错误 → -32602；未知方法 → -32601（HTTP 层映射 404）；
///   版本不支持 → -32022；头体不一致 → -32020（legacy 区 -32000 起不再使用）
/// - 工具执行错误不作为 JSON-RPC error，而是 result.isError = true，便于模型自纠正
/// - 订阅唯一入口为 subscriptions/listen（SSE 长流，实现在 transport.rs）
/// - 另提供 legacy（2024-11-05 HTTP+SSE）语义分发，供 transport.rs 的 /sse、/message 使用
use chrono::Local;
use dashmap::DashMap;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, RwLock, watch};
use tracing::{debug, error, info};

use crate::executor::ToolResult;
use crate::registry::tool_registry::ToolRegistry;

// ==================== 协议常量 ====================

/// 本服务器实现的协议版本（现代版：每请求 _meta，无握手、无会话）
pub const PROTOCOL_VERSION: &str = "2026-07-28";

/// 受支持的协议版本。本项目为 modern-only：不声明 legacy 版本
/// （2025-11-25 及更早需要 initialize 握手语义，本服务器未实现，声明即为虚报）。
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[PROTOCOL_VERSION];

/// Legacy HTTP+SSE 通道（/sse、/message，为 llama.cpp UI 等旧客户端保留）使用的协议版本
pub const LEGACY_PROTOCOL_VERSION: &str = "2024-11-05";

// ---- _meta 保留键（io.modelcontextprotocol/ 前缀） ----
pub const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
pub const META_CLIENT_INFO: &str = "io.modelcontextprotocol/clientInfo";
pub const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
pub const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";
pub const META_SUBSCRIPTION_ID: &str = "io.modelcontextprotocol/subscriptionId";

// ---- JSON-RPC / MCP 错误码 ----
pub const ERR_PARSE_ERROR: i64 = -32700;
pub const ERR_INVALID_REQUEST: i64 = -32600;
pub const ERR_METHOD_NOT_FOUND: i64 = -32601;
pub const ERR_INVALID_PARAMS: i64 = -32602;
/// MCP 规范保留区：-32020 HeaderMismatch / -32022 UnsupportedProtocolVersion
pub const ERR_HEADER_MISMATCH: i64 = -32020;
pub const ERR_UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

// ==================== JSON-RPC 错误 ====================

/// JSON-RPC error 对象的 Rust 侧表示（响应中 result 与 error 互斥，绝不同时出现）
#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

// ==================== 每请求协议元数据 ====================

/// 从 `params._meta` 解析出的每请求元数据（现代版协议没有会话，一切随请求携带）
#[derive(Debug, Clone)]
pub struct RequestMeta {
    pub protocol_version: String,
    pub client_capabilities: Value,
    pub client_info: Option<Value>,
}

impl RequestMeta {
    /// 解析 params._meta；缺失必需字段 → -32602（HTTP 层映射为 400 Bad Request）
    pub fn parse(params: &Value) -> Result<Self, RpcError> {
        let meta = params.get("_meta");
        let missing =
            |key: &str| RpcError::new(ERR_INVALID_PARAMS, format!("请求缺少必需的 _meta 字段: {key}"));

        let protocol_version = meta
            .and_then(|m| m.get(META_PROTOCOL_VERSION))
            .and_then(|v| v.as_str())
            .ok_or_else(|| missing(META_PROTOCOL_VERSION))?
            .to_string();
        let client_capabilities = meta
            .and_then(|m| m.get(META_CLIENT_CAPABILITIES))
            .cloned()
            .ok_or_else(|| missing(META_CLIENT_CAPABILITIES))?;

        Ok(Self {
            protocol_version,
            client_capabilities,
            client_info: meta.and_then(|m| m.get(META_CLIENT_INFO)).cloned(),
        })
    }

    pub fn version_supported(&self) -> bool {
        SUPPORTED_PROTOCOL_VERSIONS.contains(&self.protocol_version.as_str())
    }
}

// ==================== 日志系统 ====================

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    /// 结构化工具名（日志筛选用；非工具相关日志为 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

/// 双轨日志：内存环形缓冲（Dashboard 查询）+ broadcast（实时 SSE 推送）
pub struct LogSystem {
    pub buffer: RwLock<Vec<LogEntry>>,
    pub tx: broadcast::Sender<LogEntry>,
}

impl LogSystem {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            buffer: RwLock::new(Vec::new()),
            tx,
        }
    }

    /// 注意：async fn —— 调用方必须 `.await`，否则 future 被直接丢弃、日志静默丢失
    pub async fn log(&self, level: &str, message: impl Into<String>) {
        self.emit(LogEntry {
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            level: level.to_string(),
            message: message.into(),
            tool: None,
        })
        .await;
    }

    /// 带结构化工具名的日志（WebUI 按工具筛选的数据来源）
    pub async fn log_tool(&self, level: &str, tool: &str, message: impl Into<String>) {
        self.emit(LogEntry {
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            level: level.to_string(),
            message: message.into(),
            tool: Some(tool.to_string()),
        })
        .await;
    }

    async fn emit(&self, entry: LogEntry) {
        {
            let mut buf = self.buffer.write().await;
            buf.push(entry.clone());
            if buf.len() > 500 {
                let drain_len = buf.len() - 500;
                buf.drain(0..drain_len);
            }
        }
        let _ = self.tx.send(entry);
    }
}

// ==================== Legacy 会话表（2024-11-05 响应回程） ====================

/// 2024-11-05 HTTP+SSE 传输：POST /message 只负责提交（回 202），
/// JSON-RPC 响应必须经由该会话的 SSE 流送回——官方 SDK 忽略 POST 响应体。
#[derive(Default)]
pub struct LegacySessions {
    sessions: DashMap<String, mpsc::Sender<Value>>,
}

impl LegacySessions {
    pub fn new() -> Self {
        Self { sessions: DashMap::new() }
    }

    pub fn insert(&self, id: String, tx: mpsc::Sender<Value>) {
        self.sessions.insert(id, tx);
    }

    pub fn remove(&self, id: &str) {
        self.sessions.remove(id);
    }

    /// 把 JSON-RPC 响应推给会话的 SSE 流；会话不存在/已断开/队列满 → false（POST 侧回 404）
    pub fn try_send_response(&self, session: &str, body: Value) -> bool {
        self.sessions
            .get(session)
            .map(|tx| tx.try_send(body).is_ok())
            .unwrap_or(false)
    }
}

// ==================== 应用状态 ====================

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<ToolRegistry>,
    pub logs: Arc<LogSystem>,
    /// 工具执行超时（来自 config 的 tools.default_timeout，秒）
    pub tool_timeout: Duration,
    /// 可选 Bearer Token：Some 时 MCP 端点要求 Authorization: Bearer <token>
    pub auth_token: Option<String>,
    /// 除回环地址外额外放行的 Origin 白名单（如 llama.cpp UI 的来源）
    pub allowed_origins: Vec<String>,
    /// 提示词注册表（文件驱动，可热重载）
    pub prompts: Arc<crate::registry::prompts::PromptRegistry>,
    /// 资源注册表（文件驱动，可热重载）
    pub resources: Arc<crate::registry::prompts::ResourceRegistry>,
    /// 列表变更事件（"prompts" / "resources"），驱动 notifications/*_list_changed
    pub lists_changed: tokio::sync::broadcast::Sender<String>,
    /// 提示词目录（重载 API 用）
    pub prompts_dir: std::path::PathBuf,
    /// 插件扫描目录（rescan API 用）
    pub discovery_dirs: Vec<std::path::PathBuf>,
    /// 优雅关闭信号：WebUI「关闭」按钮置 true，等价于终端 Ctrl+C
    pub shutdown: Arc<watch::Sender<bool>>,
    /// Legacy（2024-11-05）会话表：sessionId → 响应回传通道
    pub legacy_sessions: Arc<LegacySessions>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<ToolRegistry>,
        logs: Arc<LogSystem>,
        tool_timeout: Duration,
        auth_token: Option<String>,
        allowed_origins: Vec<String>,
        prompts: Arc<crate::registry::prompts::PromptRegistry>,
        resources: Arc<crate::registry::prompts::ResourceRegistry>,
        lists_changed: tokio::sync::broadcast::Sender<String>,
        prompts_dir: std::path::PathBuf,
        discovery_dirs: Vec<std::path::PathBuf>,
        shutdown: Arc<watch::Sender<bool>>,
        legacy_sessions: Arc<LegacySessions>,
    ) -> Self {
        Self {
            registry,
            logs,
            tool_timeout,
            auth_token,
            allowed_origins,
            prompts,
            resources,
            lists_changed,
            prompts_dir,
            discovery_dirs,
            shutdown,
            legacy_sessions,
        }
    }
}

// ==================== 核心分发 ====================

/// 分发 JSON-RPC 请求（现代 2026-07-28 语义）。Ok(result) / Err(error)；HTTP 状态码由传输层（transport.rs）映射。
pub async fn handle_rpc(
    state: &AppState,
    method: &str,
    params: Value,
    meta: &RequestMeta,
) -> Result<Value, RpcError> {
    info!(
        method = %method,
        version = %meta.protocol_version,
        client = ?meta.client_info,
        "RPC 请求"
    );
    debug!(capabilities = ?meta.client_capabilities, "客户端能力");

    match method {
        "server/discover" => Ok(server_discover()),
        "tools/list" => tools_list(state, &params).await,
        "tools/call" => tools_call(state, &params).await,
        "prompts/list" => prompts_list(state).await,
        "prompts/get" => prompts_get(state, &params).await,
        "resources/list" => resources_list(state).await,
        "resources/read" => resources_read(state, &params).await,
        // 现代-only 服务器：initialize 属于未知方法，但按规范应在错误信息中
        // 列出受支持的版本（legacy 客户端唯一的诊断来源）。
        "initialize" | "notifications/initialized" => Err(RpcError::new(
            ERR_METHOD_NOT_FOUND,
            format!(
                "本服务器仅支持现代版 MCP（每请求 _meta，无 initialize 握手）；受支持的协议版本: {}",
                SUPPORTED_PROTOCOL_VERSIONS.join(", ")
            ),
        )),
        other => Err(RpcError::new(
            ERR_METHOD_NOT_FOUND,
            format!("不支持的方法: {other}"),
        )),
    }
}

// ==================== Legacy（2024-11-05 HTTP+SSE）分发 ====================

/// Legacy 通道语义：initialize 握手 + 无 _meta 要求 + 结果不注入 resultType/_meta。
/// tools/list 与 tools/call 与现代通道共用同一实现（业务层无差异）。
pub async fn handle_legacy_rpc(
    state: &AppState,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": LEGACY_PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": true } },
            "serverInfo": {
                "name": "KazeMiMiRin MCP Server",
                "version": env!("CARGO_PKG_VERSION"),
            },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => tools_list(state, &params).await,
        "tools/call" => tools_call(state, &params).await,
        "prompts/list" => prompts_list(state).await,
        "prompts/get" => prompts_get(state, &params).await,
        "resources/list" => resources_list(state).await,
        "resources/read" => resources_read(state, &params).await,
        other => Err(RpcError::new(
            ERR_METHOD_NOT_FOUND,
            format!("不支持的方法: {other}"),
        )),
    }
}

// ==================== server/discover ====================

/// 规范要求服务器 MUST 实现 server/discover。
/// DiscoverResult: { resultType, supportedVersions, capabilities, instructions? }
/// （serverInfo 由 finalize_result 统一注入 result._meta）
fn server_discover() -> Value {
    json!({
        "supportedVersions": SUPPORTED_PROTOCOL_VERSIONS,
        "capabilities": {
            "tools": { "listChanged": true },
            "prompts": { "listChanged": true },
            "resources": {},
        },
        "instructions": "通用工具集（子进程插件，热重载）：文件/目录操作、Shell 执行、网络抓取与搜索、Git、文本处理与统计；另提供文件驱动的提示词与资源。用 tools/list、prompts/list、resources/list 获取完整清单。",
    })
}

// ==================== tools/list ====================

async fn tools_list(state: &AppState, params: &Value) -> Result<Value, RpcError> {
    // 分页：cursor 为不透明字符串（本实现为起始下标）。单页 TOOLS_PAGE_SIZE 条，
    // 还有剩余时返回 nextCursor，客户端原样带回即可。
    const TOOLS_PAGE_SIZE: usize = 50;
    let start = match params.get("cursor") {
        None | Some(Value::Null) => 0usize,
        Some(Value::String(s)) => s
            .parse::<usize>()
            .map_err(|_| RpcError::new(ERR_INVALID_PARAMS, "cursor 无效"))?,
        Some(_) => return Err(RpcError::new(ERR_INVALID_PARAMS, "cursor 必须是字符串")),
    };
    // registry.list() 按名称排序，保证确定性输出（利于客户端缓存与 prompt cache 命中）
    let tools: Vec<Value> = state
        .registry
        .list()
        .iter()
        .map(|d| d.to_mcp_tool_json())
        .collect();
    let end = (start + TOOLS_PAGE_SIZE).min(tools.len());
    let mut result = json!({ "tools": tools.get(start..end).unwrap_or(&[]) });
    if end < tools.len() {
        result["nextCursor"] = json!(end.to_string());
    }
    Ok(result)
}

// ==================== tools/call ====================

async fn tools_call(state: &AppState, params: &Value) -> Result<Value, RpcError> {
    let tool_name = params
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| RpcError::new(ERR_INVALID_PARAMS, "tools/call 缺少 name 参数"))?;

    // 未知工具 / 已禁用工具对客户端而言都不可用 → 统一 -32602（规范示例错误码）
    let entry = state.registry.get(tool_name).ok_or_else(|| {
        RpcError::new(ERR_INVALID_PARAMS, format!("Unknown tool: {tool_name}"))
    })?;
    let enabled = entry
        .definition
        .read()
        .ok()
        .map(|d| d.enabled)
        .unwrap_or(true);
    if !enabled {
        state
            .logs
            .log_tool("WARN", tool_name, format!("工具 {tool_name} 已被禁用，拒绝调用"))
            .await;
        return Err(RpcError::new(
            ERR_INVALID_PARAMS,
            format!("Unknown tool: {tool_name}（工具已被禁用）"),
        ));
    }

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    // RAW 输入日志：排障时能看到工具到底收到了什么（多行 pretty JSON，截断防刷屏）
    let args_text = serde_json::to_string_pretty(&arguments).unwrap_or_default();
    state
        .logs
        .log_tool("INFO", tool_name, format!("▶ 输入:\n{}", preview_str(&args_text, 400)))
        .await;
    let started = std::time::Instant::now();

    // 输入校验（规范 MUST：Servers MUST validate all tool inputs）：
    // 按 inputSchema 校验，失败属「输入校验错误」→ result.isError = true，便于模型自纠正
    if let Some(validator) = &entry.validator {
        let failures: Vec<String> = validator
            .iter_errors(&arguments)
            .take(3)
            .map(|e| e.to_string())
            .collect();
        if !failures.is_empty() {
            let msg = format!("参数不符合 inputSchema: {}", failures.join("; "));
            state
                .logs
                .log_tool("WARN", tool_name, format!("输入校验失败: {}", failures.join("; ")))
                .await;
            return Ok(json!({
                "content": [{ "type": "text", "text": msg }],
                "isError": true,
            }));
        }
    }

    info!(tool = %tool_name, "tools/call 调用");

    // 工具执行带超时（config tools.default_timeout），避免单个调用挂死 worker
    let exec_future = entry.executor.execute(arguments);
    let result = match tokio::time::timeout(state.tool_timeout, exec_future).await {
        Ok(r) => r,
        Err(_) => {
            let msg = format!(
                "工具 {} 执行超时（超过 {} 秒）",
                tool_name,
                state.tool_timeout.as_secs()
            );
            error!(tool = %tool_name, "{}", msg);
            ToolResult::err(msg)
        }
    };

    if result.success {
        let data = result.data.unwrap_or_else(|| json!(null));
        let text = serde_json::to_string_pretty(&data).unwrap_or_else(|_| data.to_string());
        state
            .logs
            .log_tool(
                "INFO",
                tool_name,
                format!(
                    "✓ 成功（{} 字符，{:.1} 秒）⬅ 输出:\n{}",
                    text.len(),
                    started.elapsed().as_secs_f32(),
                    preview_str(&text, 800)
                ),
            )
            .await;
        Ok(json!({
            "content": [{ "type": "text", "text": text }],
            "structuredContent": data,
            "isError": false,
        }))
    } else {
        let msg = result.error.unwrap_or_else(|| "未知错误".to_string());
        error!(tool = %tool_name, error = %msg, "工具执行失败");
        state
            .logs
            .log_tool(
                "ERROR",
                tool_name,
                format!("✗ 失败（{:.1} 秒）: {msg}", started.elapsed().as_secs_f32()),
            )
            .await;
        // 工具执行错误：按规范放在 result.isError 中返回（而非 JSON-RPC error），
        // 让模型能读到错误文本并自纠正。
        Ok(json!({
            "content": [{ "type": "text", "text": msg }],
            "isError": true,
        }))
    }
}

// ==================== 日志截断助手 ====================

/// 日志预览：超长截断并注明原始长度（按字符计，中文友好）
fn preview_str(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…（截断，共 {n} 字符）")
    }
}

// ==================== prompts / resources（需求四） ====================

async fn prompts_list(state: &AppState) -> Result<Value, RpcError> {
    // category 是 Dashboard 界面专用概念，不进 MCP 协议面（与 tools/list 口径一致）
    let prompts: Vec<Value> = state
        .prompts
        .list()
        .await
        .into_iter()
        .map(|mut p| {
            if let Some(obj) = p.as_object_mut() {
                obj.remove("category");
            }
            p
        })
        .collect();
    Ok(json!({ "prompts": prompts }))
}

async fn prompts_get(state: &AppState, params: &Value) -> Result<Value, RpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::new(ERR_INVALID_PARAMS, "prompts/get 缺少 name 参数"))?;
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    state
        .prompts
        .get(name, &arguments)
        .await
        .map_err(|e| RpcError::new(ERR_INVALID_PARAMS, format!("{e:#}")))
}

async fn resources_list(state: &AppState) -> Result<Value, RpcError> {
    let resources = state.resources.list().await;
    Ok(json!({ "resources": resources }))
}

async fn resources_read(state: &AppState, params: &Value) -> Result<Value, RpcError> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::new(ERR_INVALID_PARAMS, "resources/read 缺少 uri 参数"))?;
    let contents = state
        .resources
        .read(uri)
        .await
        .map_err(|e| RpcError::new(ERR_INVALID_PARAMS, format!("{e:#}")))?;
    Ok(json!({ "contents": [contents] }))
}

// ==================== 响应信封 ====================

/// 为业务 result 补充规范必需字段：resultType 与 result._meta.serverInfo
pub fn finalize_result(mut result: Value) -> Value {
    if !result.is_object() {
        result = json!({});
    }
    let obj = result.as_object_mut().expect("result 已确认为 object");
    obj.insert("resultType".into(), json!("complete"));
    obj.insert("_meta".into(), server_info_meta());
    result
}

fn server_info_meta() -> Value {
    json!({
        (META_SERVER_INFO): {
            "name": "KazeMiMiRin MCP Server",
            "version": env!("CARGO_PKG_VERSION"),
        }
    })
}

/// 成功响应：{jsonrpc, id, result} —— 不允许出现 error 字段（JSON-RPC 互斥规则），
/// id 必须回显请求 id。
pub fn result_envelope(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// 错误响应：{jsonrpc, id?, error} —— 不允许出现 result 字段；id 仅在可读时回显。
pub fn error_envelope(id: Option<&Value>, err: &RpcError) -> Value {
    let mut error = json!({ "code": err.code, "message": err.message });
    if let Some(data) = &err.data {
        error["data"] = data.clone();
    }
    let mut body = json!({ "jsonrpc": "2.0", "error": error });
    if let Some(id) = id {
        body["id"] = id.clone();
    }
    body
}
