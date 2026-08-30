/// Dashboard REST API — 工具管理 + 日志查询 + 实时日志 SSE
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json, Response,
    },
    routing::{get, post},
    Router,
};
use futures::stream::Stream;
use std::convert::Infallible;

use crate::mcp::handler::{AppState, LogEntry};

// ==================== 工具管理 API ====================

/// GET /api/tools — 全部工具列表（含启用状态）
pub async fn api_tools(State(state): State<AppState>) -> Json<Vec<serde_json::Value>> {
    let tools: Vec<serde_json::Value> = state
        .registry
        .list_all()
        .into_iter()
        .map(|def| {
            serde_json::json!({
                "name": def.name,
                "description": def.description,
                "enabled": def.enabled,
                "source": format!("{:?}", def.source),
                "inputSchema": def.input_schema,
            })
        })
        .collect();
    Json(tools)
}

/// POST /api/tools/{name}/unload — 禁用（卸载）工具
pub async fn api_unload(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    match state.registry.toggle(&name) {
        Some(false) => {
            state.logs.log_tool("WARN", &name, "已卸载（禁用）").await;
            (StatusCode::OK, format!("Tool '{name}' unloaded")).into_response()
        }
        Some(true) => {
            // toggle 把它启用了，不是我们想要的 — 重新 toggle 回去
            state.registry.toggle(&name);
            (StatusCode::OK, format!("Tool '{name}' was already disabled")).into_response()
        }
        None => (StatusCode::NOT_FOUND, format!("Tool '{name}' not found")).into_response(),
    }
}

/// POST /api/tools/{name}/load — 启用（加载）工具
pub async fn api_load(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    match state.registry.toggle(&name) {
        Some(true) => {
            state.logs.log("INFO", format!("Tool '{}' reloaded (enabled)", name)).await;
            (StatusCode::OK, format!("Tool '{}' loaded", name)).into_response()
        }
        Some(false) => {
            // toggle 把它禁用了，不是我们想要的 — 重新 toggle 回去
            state.registry.toggle(&name);
            (StatusCode::OK, format!("Tool '{}' was already enabled", name)).into_response()
        }
        None => (StatusCode::NOT_FOUND, format!("Tool '{}' not found", name)).into_response(),
    }
}

/// POST /api/tools/{name}/reload — 从磁盘热重载插件工具（改动代码后无需重启服务器）
pub async fn api_reload_tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    match crate::mcp::plugins::reload_tool(&state, &name).await {
        Ok(new_name) => (StatusCode::OK, format!("Tool '{name}' reloaded as '{new_name}'")).into_response(),
        Err(e) => (
            StatusCode::CONFLICT,
            format!("Reload failed: {e:#}"),
        )
            .into_response(),
    }
}

/// POST /api/tools/rescan — 扫描发现目录，登记新增插件（不动已有工具）
pub async fn api_rescan_tools(State(state): State<AppState>) -> Response {
    match crate::mcp::plugins::rescan_new_tools(&state).await {
        Ok(added) => (
            StatusCode::OK,
            axum::Json(serde_json::json!({ "added": added, "count": added.len() })),
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")).into_response(),
    }
}

// ==================== 提示词 / 资源（需求四） ====================

/// GET /api/prompts — 提示词列表
pub async fn api_prompts(State(state): State<AppState>) -> Json<Vec<serde_json::Value>> {
    Json(state.prompts.list().await)
}

/// POST /api/prompts/reload — 从磁盘重载提示词（推送 prompts/list_changed）
pub async fn api_prompts_reload(State(state): State<AppState>) -> Response {
    match state.prompts.reload_from_dir(&state.prompts_dir, Some(&state.logs)).await {
        Ok(n) => {
            let _ = state.lists_changed.send("prompts".to_string());
            (StatusCode::OK, format!("Prompts reloaded: {n}")).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")).into_response(),
    }
}

/// GET /api/resources — 资源列表
pub async fn api_resources(State(state): State<AppState>) -> Json<Vec<serde_json::Value>> {
    Json(state.resources.list().await)
}

/// POST /api/resources/reload — 从磁盘重载资源（推送 resources/list_changed）
pub async fn api_resources_reload(State(state): State<AppState>) -> Response {
    match state.resources.reload_from_dir(Some(&state.logs)).await {
        Ok(n) => {
            let _ = state.lists_changed.send("resources".to_string());
            (StatusCode::OK, format!("Resources reloaded: {n}")).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")).into_response(),
    }
}

// ==================== 日志 API ====================

/// GET /api/logs — 全部历史日志
pub async fn api_logs(State(state): State<AppState>) -> Json<Vec<LogEntry>> {
    let logs = state.logs.buffer.read().await;
    Json(logs.clone())
}

/// GET /api/logs/stream — 实时日志 SSE 流
pub async fn api_logs_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.logs.tx.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(entry) => {
                    let data = serde_json::to_string(&entry).unwrap_or_default();
                    yield Ok(Event::default().event("log").data(data));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::new())
}

// ==================== 路由构建 ====================

/// 构建 Dashboard 路由
pub fn build_dashboard_router(state: AppState) -> Router {
    Router::new()
        .route("/api/tools", get(api_tools))
        .route("/api/tools/rescan", post(api_rescan_tools))
        .route("/api/tools/:name/unload", post(api_unload))
        .route("/api/tools/:name/load", post(api_load))
        .route("/api/tools/:name/reload", post(api_reload_tool))
        .route("/api/prompts", get(api_prompts))
        .route("/api/prompts/reload", post(api_prompts_reload))
        .route("/api/resources", get(api_resources))
        .route("/api/resources/reload", post(api_resources_reload))
        .route("/api/logs", get(api_logs))
        .route("/api/logs/stream", get(api_logs_stream))
        .with_state(state)
}
