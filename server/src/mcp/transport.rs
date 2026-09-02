// ============================================================================
// MCP HTTP 传输层（文件名沿用历史 sse.rs → 已更名为 transport.rs）
//
// 双时代服务器，两条协议面并行：
//
// 【现代 · 2026-07-28（"MCP 2.0"）】 单一 MCP 端点 `/mcp`：
//   - POST   /mcp：每个 JSON-RPC 消息独立 POST；响应为 application/json
//     （服务器未启用请求级 SSE 响应，规范允许二选一）
//   - GET    /mcp：2025-11-25 时代的流式端点已在 2026-07-28 移除 → 一律 405
//   - 必需头：MCP-Protocol-Version / Mcp-Method（tools/call 还需 Mcp-Name），
//     头体不一致 → 400 -32020；版本不支持 → 400 -32022；缺 _meta 必需字段 → 400 -32602
//   - 通知（无 id）→ 202 无 body
//
// 【Legacy · 2024-11-05 HTTP+SSE】（为 llama.cpp UI 等旧客户端保留）：
//   - GET  /sse：打开 SSE 长流，首条事件 `endpoint` 携带回传地址
//     （规范形状：data 为纯 URI 字符串 /message?sessionId=xxx）
//   - POST /message?sessionId=xxx：旧语义 JSON-RPC（initialize 握手、无 _meta、
//     结果无 resultType/_meta）；工具列表变更经此通道推送
//     notifications/tools/list_changed
//
// 两代共用的强制校验（Security / Server Validation 章节）：
//   - Origin：非回环且不在 allowed_origins 白名单 → 403（DNS rebinding 防护）
//   - 可选 Bearer Token：config.server.auth_token / 环境变量 MCP_AUTH_TOKEN
//     配置后，MCP 端点（/mcp、/sse、/message）要求 Authorization: Bearer <token>
//
// 注意：/mcp、/sse、/message 只允许在此注册一次，重复注册会使 axum panic。
// ============================================================================

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::warn;

use crate::mcp::handler::{
    error_envelope, finalize_result, handle_legacy_rpc, handle_rpc, result_envelope, AppState,
    LegacySessions, RequestMeta, RpcError, ERR_HEADER_MISMATCH, ERR_INVALID_REQUEST,
    ERR_METHOD_NOT_FOUND, ERR_PARSE_ERROR, ERR_UNSUPPORTED_PROTOCOL_VERSION, META_SUBSCRIPTION_ID,
    SUPPORTED_PROTOCOL_VERSIONS,
};
use crate::registry::tool_registry::ToolChangeEvent;

/// 构建 MCP 路由（现代 + Legacy 双时代）
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // ---- 现代 2026-07-28 ----
        .route("/mcp", post(handle_mcp_post))
        .route("/mcp", get(handle_mcp_get))
        // ---- Legacy 2024-11-05 HTTP+SSE ----
        .route("/sse", get(handle_legacy_sse))
        .route("/message", post(handle_legacy_message))
        .with_state(state)
}

// ==================== 现代：POST /mcp — 唯一消息入口 ====================

async fn handle_mcp_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(resp) = check_access(&state, &headers) {
        return resp;
    }

    // 1. Origin 校验：浏览器类客户端会携带 Origin，host 必须是回环或白名单成员（防 DNS rebinding）
    if let Some(origin) = header_str(&headers, header::ORIGIN) {
        if !is_origin_allowed(&state, &origin) {
            warn!(origin = %origin, "拒绝非白名单 Origin 请求（防 DNS rebinding）");
            return json_response(
                StatusCode::FORBIDDEN,
                error_envelope(
                    None,
                    &RpcError::new(ERR_INVALID_REQUEST, format!("Origin '{origin}' 不被允许")),
                ),
            );
        }
    }

    // 2. 解析 body：单个 JSON-RPC 消息（不支持批量）
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                error_envelope(None, &RpcError::new(ERR_PARSE_ERROR, "请求体不是合法 JSON")),
            )
        }
    };
    let Some(obj) = payload.as_object() else {
        return json_response(
            StatusCode::BAD_REQUEST,
            error_envelope(
                None,
                &RpcError::new(ERR_INVALID_REQUEST, "请求体必须是单个 JSON-RPC 消息对象（不支持批量）"),
            ),
        );
    };
    if obj.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
        return json_response(
            StatusCode::BAD_REQUEST,
            error_envelope(None, &RpcError::new(ERR_INVALID_REQUEST, "jsonrpc 字段必须是 \"2.0\"")),
        );
    }
    let Some(method) = obj.get("method").and_then(|v| v.as_str()).map(str::to_string) else {
        return json_response(
            StatusCode::BAD_REQUEST,
            error_envelope(None, &RpcError::new(ERR_INVALID_REQUEST, "缺少 method 字段")),
        );
    };

    // 3. 请求 id：MCP 规定为 string|number 且不得为 null；无 id = 通知
    let id = match obj.get("id") {
        Some(Value::Null) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                error_envelope(None, &RpcError::new(ERR_INVALID_REQUEST, "请求 id 不能为 null")),
            )
        }
        None => None,
        Some(v) if v.is_number() || v.is_string() => Some(v.clone()),
        Some(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                error_envelope(
                    None,
                    &RpcError::new(ERR_INVALID_REQUEST, "请求 id 必须是 string 或 number"),
                ),
            )
        }
    };
    let Some(id) = id else {
        // 本修订版核心协议未定义 client → server 通知；按传输规则接受即可（202，无 body）
        return StatusCode::ACCEPTED.into_response();
    };
    let params = obj.get("params").cloned().unwrap_or_else(|| json!({}));

    // 4. 必需镜像头校验（-32020 HeaderMismatch）
    let header_version = match validate_headers(&headers, &id, &method, &params) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    // 5. _meta 解析（-32602 → 400）与版本协商（-32022 → 400）
    let meta = match RequestMeta::parse(&params) {
        Ok(m) => m,
        Err(e) => return json_response(StatusCode::BAD_REQUEST, error_envelope(Some(&id), &e)),
    };
    if header_version != meta.protocol_version {
        return header_mismatch(
            &id,
            format!(
                "MCP-Protocol-Version 头 '{}' 与 _meta 的 protocolVersion '{}' 不一致",
                header_version, meta.protocol_version
            ),
        );
    }
    if !meta.version_supported() {
        return json_response(
            StatusCode::BAD_REQUEST,
            error_envelope(
                Some(&id),
                &RpcError::new(ERR_UNSUPPORTED_PROTOCOL_VERSION, "Unsupported protocol version")
                    .with_data(json!({
                        "supported": SUPPORTED_PROTOCOL_VERSIONS,
                        "requested": meta.protocol_version,
                    })),
            ),
        );
    }

    // 6. 分发：subscriptions/listen 需要返回 SSE 长流，在此拦截；其余走 handle_rpc
    if method == "subscriptions/listen" {
        return handle_subscriptions_listen(&state, &id, params).await;
    }

    match handle_rpc(&state, &method, params, &meta).await {
        Ok(result) => json_response(StatusCode::OK, result_envelope(&id, finalize_result(result))),
        Err(e) => {
            let status = if e.code == ERR_METHOD_NOT_FOUND {
                // 规范：MCP 端点上未知方法 → 404 + JSON-RPC -32601
                StatusCode::NOT_FOUND
            } else {
                StatusCode::OK
            };
            json_response(status, error_envelope(Some(&id), &e))
        }
    }
}

/// 2026-07-28 移除了 GET 流端点（订阅走 POST subscriptions/listen）
async fn handle_mcp_get() -> Response {
    (
        [(header::ALLOW, "POST")],
        StatusCode::METHOD_NOT_ALLOWED,
    )
        .into_response()
}

// ==================== 现代：subscriptions/listen — 订阅长流 ====================

/// 唯一的订阅入口：响应本身是 SSE 长流（保持 POST 请求-响应形态）。
/// - 第一条消息必须是 notifications/subscriptions/acknowledged
/// - subscriptionId = 本请求的 JSON-RPC id，经 _meta["io.modelcontextprotocol/subscriptionId"] 关联
/// - 取消 = 客户端关闭流（无需 unsubscribe 方法）
async fn handle_subscriptions_listen(state: &AppState, id: &Value, params: Value) -> Response {
    let filter = params
        .get("notifications")
        .cloned()
        .unwrap_or_else(|| json!({}));
    // 支持三类 list_changed；resourceSubscriptions（按 URI 的资源更新）暂不支持，
    // 不支持的类型在 ack 中省略（客户端应优雅处理）
    let tools_list_changed = filter.get("toolsListChanged").and_then(Value::as_bool).unwrap_or(false);
    let prompts_list_changed = filter.get("promptsListChanged").and_then(Value::as_bool).unwrap_or(false);
    let resources_list_changed = filter.get("resourcesListChanged").and_then(Value::as_bool).unwrap_or(false);
    let mut acked = json!({});
    if tools_list_changed {
        acked["toolsListChanged"] = json!(true);
    }
    if prompts_list_changed {
        acked["promptsListChanged"] = json!(true);
    }
    if resources_list_changed {
        acked["resourcesListChanged"] = json!(true);
    }

    let ack = json!({
        "jsonrpc": "2.0",
        "method": "notifications/subscriptions/acknowledged",
        "params": {
            "_meta": { (META_SUBSCRIPTION_ID): id },
            "notifications": acked,
        }
    });

    let mut rx = state.registry.notify_rx();
    let mut lists_rx = state.lists_changed.subscribe();
    let sub_id = id.clone();

    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(Event::default().event("message").data(ack.to_string()));
        loop {
            let notification = tokio::select! {
                change = rx.recv() => match change {
                    // 注册（含 rescan 新增）与启停都改变工具列表 → 推送 list_changed
                    Ok(ToolChangeEvent::Registered { .. }) | Ok(ToolChangeEvent::Toggled { .. })
                        if tools_list_changed =>
                    {
                        Some(json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/tools/list_changed",
                            "params": { "_meta": { (META_SUBSCRIPTION_ID): sub_id } }
                        }))
                    }
                    Ok(_) => None,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => None,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                list = lists_rx.recv() => match list {
                    Ok(kind) if kind == "prompts" && prompts_list_changed => {
                        Some(json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/prompts/list_changed",
                            "params": { "_meta": { (META_SUBSCRIPTION_ID): sub_id } }
                        }))
                    }
                    Ok(kind) if kind == "resources" && resources_list_changed => {
                        Some(json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/resources/list_changed",
                            "params": { "_meta": { (META_SUBSCRIPTION_ID): sub_id } }
                        }))
                    }
                    Ok(_) => None,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => None,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
            };
            if let Some(note) = notification {
                yield Ok(Event::default().event("message").data(note.to_string()));
            }
        }
    };

    sse_response(stream)
}

// ==================== Legacy：GET /sse — 旧传输通道 ====================

/// 2024-11-05 HTTP+SSE：打开 SSE 长流，首条 `endpoint` 事件的 data 为
/// 纯 URI 字符串。该会话后续所有 JSON-RPC 响应都经此流以 `message` 事件送回
/// （官方 SDK 忽略 POST 响应体）；工具/提示词/资源列表变更也在此推送。
async fn handle_legacy_sse(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = check_access(&state, &headers) {
        return resp;
    }

    let session_id = next_session_id();
    let endpoint = format!("/message?sessionId={session_id}");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Value>(64);
    state.legacy_sessions.insert(session_id.clone(), tx);
    let mut rx_tools = state.registry.notify_rx();
    let mut rx_lists = state.lists_changed.subscribe();

    let sessions = state.legacy_sessions.clone();
    let stream = async_stream::stream! {
        // 守卫：流被丢弃（客户端断开）时注销会话
        struct SessionDropGuard {
            id: String,
            sessions: Arc<LegacySessions>,
        }
        impl Drop for SessionDropGuard {
            fn drop(&mut self) {
                self.sessions.remove(&self.id);
            }
        }
        let _guard = SessionDropGuard { id: session_id.clone(), sessions };

        yield Ok::<Event, Infallible>(Event::default().event("endpoint").data(endpoint));
        loop {
            let notification = tokio::select! {
                resp = rx.recv() => match resp {
                    // 该会话的 JSON-RPC 响应（POST /message 提交后路由到此）
                    Some(body) => Some(body),
                    None => break,
                },
                change = rx_tools.recv() => match change {
                    Ok(ToolChangeEvent::Registered { .. }) | Ok(ToolChangeEvent::Toggled { .. }) => {
                        Some(json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/tools/list_changed",
                        }))
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => None,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                list = rx_lists.recv() => match list {
                    Ok(kind) if kind == "prompts" => {
                        Some(json!({ "jsonrpc": "2.0", "method": "notifications/prompts/list_changed" }))
                    }
                    Ok(kind) if kind == "resources" => {
                        Some(json!({ "jsonrpc": "2.0", "method": "notifications/resources/list_changed" }))
                    }
                    Ok(_) => None,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => None,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
            };
            if let Some(note) = notification {
                yield Ok(Event::default().event("message").data(note.to_string()));
            }
        }
    };

    sse_response(stream)
}

// ==================== Legacy：POST /message — 旧消息入口 ====================

#[derive(Deserialize)]
struct LegacyQuery {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

/// 2024-11-05 语义：POST 只负责提交——处理 JSON-RPC 后把响应体经该会话的
/// /sse 流送回，本端点按规范回 `202 Accepted`（官方 SDK 忽略 POST 响应体）。
/// 未知/已断开的 sessionId → 404（客户端应重新 GET /sse 建立会话）。
async fn handle_legacy_message(
    State(state): State<AppState>,
    Query(q): Query<LegacyQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(resp) = check_access(&state, &headers) {
        return resp;
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                error_envelope(None, &RpcError::new(ERR_PARSE_ERROR, "请求体不是合法 JSON")),
            )
        }
    };
    let Some(obj) = payload.as_object() else {
        return json_response(
            StatusCode::BAD_REQUEST,
            error_envelope(None, &RpcError::new(ERR_INVALID_REQUEST, "请求体必须是 JSON-RPC 消息对象")),
        );
    };
    let Some(method) = obj.get("method").and_then(|v| v.as_str()).map(str::to_string) else {
        return json_response(
            StatusCode::BAD_REQUEST,
            error_envelope(None, &RpcError::new(ERR_INVALID_REQUEST, "缺少 method 字段")),
        );
    };

    // 通知（无 id）：legacy 通道接受即忽略（如 notifications/initialized）
    let id = match obj.get("id") {
        None | Some(Value::Null) => None,
        Some(v) if v.is_number() || v.is_string() => Some(v.clone()),
        Some(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                error_envelope(None, &RpcError::new(ERR_INVALID_REQUEST, "请求 id 必须是 string 或 number")),
            )
        }
    };
    let Some(id) = id else {
        return StatusCode::ACCEPTED.into_response();
    };

    let params = obj.get("params").cloned().unwrap_or_else(|| json!({}));
    let response = match handle_legacy_rpc(&state, &method, params).await {
        Ok(result) => result_envelope(&id, result),
        Err(e) => error_envelope(Some(&id), &e),
    };

    // 响应经会话的 SSE 流送回；本端点按规范回 202 无 body
    let session = q.session_id.clone().unwrap_or_default();
    if state.legacy_sessions.try_send_response(&session, response) {
        StatusCode::ACCEPTED.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            format!("unknown or expired session '{session}'"),
        )
            .into_response()
    }
}

// ==================== 访问控制与头部校验 ====================

/// 鉴权 + Origin 之外的公共入口检查：可选 Bearer Token
fn check_access(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let Some(expected) = &state.auth_token else {
        return Ok(());
    };
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if provided == Some(expected.as_str()) {
        return Ok(());
    }
    warn!("请求缺少或携带错误的 Bearer Token，返回 401");
    Err((
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        axum::Json(json!({
            "jsonrpc": "2.0",
            "error": { "code": ERR_INVALID_REQUEST, "message": "缺少或错误的 Bearer Token" }
        })),
    )
        .into_response())
}

/// 校验必需的标准镜像头；返回 body 版本应匹配的 MCP-Protocol-Version 值
fn validate_headers(
    headers: &HeaderMap,
    id: &Value,
    method: &str,
    params: &Value,
) -> Result<String, Response> {
    let mismatch = |msg: String| -> Response {
        json_response(
            StatusCode::BAD_REQUEST,
            error_envelope(Some(id), &RpcError::new(ERR_HEADER_MISMATCH, msg)),
        )
    };

    let version = header_str(headers, "mcp-protocol-version")
        .ok_or_else(|| mismatch("缺少必需头 MCP-Protocol-Version".to_string()))?;

    let header_method = header_str(headers, "mcp-method")
        .ok_or_else(|| mismatch("缺少必需头 Mcp-Method".to_string()))?;
    if header_method != method {
        return Err(mismatch(format!(
            "Mcp-Method 头 '{header_method}' 与请求体 method '{method}' 不一致"
        )));
    }

    if method == "tools/call" {
        if let Some(body_name) = params.get("name").and_then(Value::as_str) {
            let Some(header_name) = header_str(headers, "mcp-name") else {
                return Err(mismatch("tools/call 缺少必需头 Mcp-Name".to_string()));
            };
            if header_name != body_name {
                return Err(mismatch(format!(
                    "Mcp-Name 头 '{header_name}' 与请求体 name '{body_name}' 不一致"
                )));
            }
        }
        // body 缺 name 的情况交给 tools_call 以 -32602 报告
    }

    Ok(version)
}

fn header_mismatch(id: &Value, msg: String) -> Response {
    json_response(
        StatusCode::BAD_REQUEST,
        error_envelope(Some(id), &RpcError::new(ERR_HEADER_MISMATCH, msg)),
    )
}

fn json_response(status: StatusCode, body: Value) -> Response {
    (status, axum::Json(body)).into_response()
}

fn sse_response(stream: impl futures::Stream<Item = Result<Event, Infallible>> + Send + 'static) -> Response {
    let mut resp = Sse::new(stream).keep_alive(KeepAlive::new()).into_response();
    // 反向代理（nginx 等）禁用缓冲，保证 SSE 即时送达（规范 SHOULD）
    resp.headers_mut().insert(
        header::HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    resp
}

/// 读取并解码头（支持规范定义的 `=?base64?...?=` 哨兵编码；非 UTF-8 视为无效）
fn header_str(headers: &HeaderMap, name: impl axum::http::header::AsHeaderName) -> Option<String> {
    let raw = headers.get(name)?.to_str().ok()?;
    Some(decode_mcp_value(raw))
}

/// 按 Value Encoding 规则解码：=?base64?<payload>?= → UTF-8 字符串；其余原样返回
fn decode_mcp_value(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix("=?base64?") {
        if let Some(payload) = rest.strip_suffix("?=") {
            if let Ok(bytes) = base64_decode(payload) {
                if let Ok(s) = String::from_utf8(bytes) {
                    return s;
                }
            }
            // 无法解码的哨兵值按原样返回（后续与 body 比对必然不一致而拒绝）
        }
    }
    raw.to_string()
}

fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = input.trim_end_matches('=');
    let mut out = Vec::with_capacity(input.len() * 3 / 4 + 3);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for ch in input.bytes() {
        let v = TABLE.iter().position(|&c| c == ch).ok_or(())? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

/// Origin 合法性：host 是本机回环，或整体命中 allowed_origins 白名单
fn is_origin_allowed(state: &AppState, origin: &str) -> bool {
    is_loopback_origin(origin)
        || state
            .allowed_origins
            .iter()
            .any(|o| o.eq_ignore_ascii_case(origin))
}

fn is_loopback_origin(origin: &str) -> bool {
    let after_scheme = origin.split("://").last().unwrap_or(origin);
    let authority = after_scheme.split(['/', '?']).next().unwrap_or("");
    let host = if let Some(inner) = authority.strip_prefix('[') {
        inner.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    matches!(
        host.to_ascii_lowercase().as_str(),
        "127.0.0.1" | "localhost" | "::1"
    )
}

/// legacy 通道的传输层会话标识（无状态寻址用，非协议会话）
fn next_session_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:016x}{seq:04x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn origin_check() {
        assert!(is_loopback_origin("http://127.0.0.1:58081"));
        assert!(is_loopback_origin("http://localhost:3000/dashboard"));
        assert!(is_loopback_origin("http://[::1]:58081"));
        assert!(!is_loopback_origin("http://evil.example.com"));
        assert!(!is_loopback_origin("null"));
    }

    #[test]
    fn value_decoding() {
        assert_eq!(decode_mcp_value("get_weather"), "get_weather");
        // 规范中的编码示例
        assert_eq!(decode_mcp_value("=?base64?SGVsbG8sIOS4lueVjA==?="), "Hello, 世界");
        assert_eq!(decode_mcp_value("=?base64?bGluZTEKbGluZTI=?="), "line1\nline2");
        assert_eq!(decode_mcp_value("=?base64?__无效__?="), "=?base64?__无效__?=");
    }

    #[test]
    fn meta_parse_requires_namespaced_keys() {
        // 裸键名（旧实现）必须被拒绝
        let bad = json!({ "_meta": { "protocolVersion": "2026-07-28" } });
        assert!(RequestMeta::parse(&bad).is_err());
        let good = json!({ "_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {},
        }});
        let meta = RequestMeta::parse(&good).unwrap();
        assert!(meta.version_supported());
    }
}
