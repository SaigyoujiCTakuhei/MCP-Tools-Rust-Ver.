/// KazeMiMiRin MCP Server — Rust 纯二进制版入口（Cargo workspace: server / tool_kit / tools）
///
/// 工具全部为子进程插件（kzm-* 可执行文件），启动时自动发现；
/// 改动工具源码后 `cargo build`，再经 WebUI/API「重载」即生效，无需重启服务器。
mod config;
mod dashboard;
mod executor;
mod mcp;
mod registry;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::response::Html;
use axum::routing::get;
use http::header::HeaderName;
use http::{header, Method};
use tokio::sync::broadcast;
use tracing::info;
use tracing_subscriber::{layer::Layer, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};

use config::load_config;
use mcp::handler::{AppState, LogSystem};
use registry::prompts::{PromptRegistry, ResourceRegistry};
use registry::tool_registry::ToolRegistry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ========== 1. 加载配置 ==========
    // 优先 exe 同目录，其次工作目录：避免 Windows 服务/计划任务等场景下
    // 工作目录漂移导致配置被静默忽略（双端行为一致）。
    let config_path = resolve_config_path();
    let used_default_config = !config_path.exists();
    let app_config = load_config(&config_path).await?;

    // ========== 2. 日志初始化（读取 logging.level / logging.format） ==========
    let filter = EnvFilter::try_new(&app_config.logging.level).unwrap_or_else(|_| "info".into());
    let fmt_layer = if app_config.logging.format.eq_ignore_ascii_case("json") {
        tracing_subscriber::fmt::layer().json().boxed()
    } else {
        tracing_subscriber::fmt::layer().pretty().boxed()
    };
    tracing_subscriber::registry().with(filter).with(fmt_layer).init();

    info!("🚀 KazeMiMiRin MCP Server 启动中...");
    if used_default_config {
        tracing::warn!("未找到 config.yaml，使用默认配置");
    }
    info!(
        path = %config_path.display(),
        host = %app_config.server.host,
        port = app_config.server.port,
        "配置加载完成"
    );

    // ========== 3. 基础设施：注册中心 / 日志 / 变更通道 / 提示词与资源 ==========
    let registry = Arc::new(ToolRegistry::new());
    let logs = Arc::new(LogSystem::new());
    let (lists_changed, _) = broadcast::channel::<String>(64);

    let prompts_dir = config_path
        .parent()
        .map(|d| d.join(&app_config.mcp.prompts_path))
        .unwrap_or_else(|| PathBuf::from(&app_config.mcp.prompts_path));
    let resources_dir = config_path
        .parent()
        .map(|d| d.join(&app_config.mcp.resources_path))
        .unwrap_or_else(|| PathBuf::from(&app_config.mcp.resources_path));
    let prompts = Arc::new(PromptRegistry::new());
    let resources = Arc::new(ResourceRegistry::new(resources_dir));
    let prompts_count = prompts.reload_from_dir(&prompts_dir, Some(&logs)).await.unwrap_or_else(|e| {
        tracing::warn!("提示词目录不可用: {e:#}");
        0
    });
    let resources_count = resources.reload_from_dir(Some(&logs)).await.unwrap_or_else(|e| {
        tracing::warn!("资源目录不可用: {e:#}");
        0
    });
    info!(prompts = prompts_count, resources = resources_count, "📚 提示词与资源已加载");

    // ========== 4. 构建共享状态 ==========
    // 鉴权：环境变量 MCP_AUTH_TOKEN 优先，其次 config.server.auth_token（空 = 关闭鉴权）
    let auth_token = std::env::var("MCP_AUTH_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| {
            let t = &app_config.server.auth_token;
            (!t.is_empty()).then(|| t.clone())
        });
    if auth_token.is_some() {
        info!("🔒 MCP 端点鉴权已启用（Bearer Token）");
    } else {
        tracing::warn!("未配置 auth_token，MCP 端点不做鉴权（仅靠回环绑定 + Origin 校验）");
    }
    let tool_timeout = Duration::from_secs(app_config.tools.default_timeout.max(1));
    // 优雅关闭通道：Ctrl+C / SIGTERM / WebUI「关闭」按钮三路汇聚
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let shutdown_tx = Arc::new(shutdown_tx);
    // 插件扫描目录：config.tools.discovery_path（可选）+ exe 同目录
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let mut discovery_dirs: Vec<PathBuf> = Vec::new();
    if !app_config.tools.discovery_path.is_empty() {
        discovery_dirs.push(PathBuf::from(&app_config.tools.discovery_path));
    }
    if let Some(d) = exe_dir {
        discovery_dirs.push(d);
    }
    let state = AppState::new(
        registry.clone(),
        logs.clone(),
        tool_timeout,
        auth_token,
        app_config.server.allowed_origins.clone(),
        prompts.clone(),
        resources,
        lists_changed,
        prompts_dir,
        discovery_dirs.clone(),
        shutdown_tx,
    );

    // ========== 5. 发现并加载插件工具（失败 → ERROR 日志，不阻断启动） ==========
    let plugins = mcp::plugins::discover(&discovery_dirs, &logs).await;
    for (binary, decl) in plugins {
        mcp::plugins::register_plugin(&state, binary, decl);
    }
    info!(count = registry.count(), "🔧 插件工具加载完成");

    // ========== 6. 构建路由 ==========
    // 注意：/mcp、/sse、/message 只能在 transport::build_router 中注册一次，
    // 重复注册会使 axum 在启动时 panic。
    let mcp_router = mcp::transport::build_router(state.clone());
    let dashboard_api_router = dashboard::api::build_dashboard_router(state.clone());

    // CORS 白名单 = 本机回环同端口来源 + config.server.allowed_origins
    // （llama.cpp UI 等浏览器端 MCP 客户端跨 Origin 直连时，把它的来源加进配置）
    let port = app_config.server.port;
    let mut origins = vec![
        header::HeaderValue::from_str(&format!("http://127.0.0.1:{port}")).expect("合法 Origin"),
        header::HeaderValue::from_str(&format!("http://localhost:{port}")).expect("合法 Origin"),
    ];
    for o in &app_config.server.allowed_origins {
        match header::HeaderValue::from_str(o) {
            Ok(v) => origins.push(v),
            Err(_) => tracing::warn!(origin = %o, "allowed_origins 中的 Origin 非法，已忽略"),
        }
    }
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(AllowHeaders::list(vec![
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
            HeaderName::from_static("mcp-protocol-version"),
            HeaderName::from_static("mcp-method"),
            HeaderName::from_static("mcp-name"),
            HeaderName::from_static("last-event-id"),
        ]));

    let app = axum::Router::new()
        .merge(mcp_router)
        .merge(dashboard_api_router)
        // WebUI 首页
        .route("/", get(|| async { Html(dashboard::html::dashboard_html()) }))
        .layer(cors);

    // ========== 7. 启动服务器 ==========
    let addr = format!("{}:{}", app_config.server.host, app_config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("============================================");
    info!("KazeMiMiRin's MCP Toolset Started (Rust)");
    info!("协议: 现代 2026-07-28 (\"MCP 2.0\") + Legacy 2024-11-05 HTTP+SSE（llama.cpp UI）");
    info!("Address: http://{}", addr);
    info!("MCP 现代: POST http://{}/mcp   (Streamable HTTP, 单端点)", addr);
    info!("MCP 旧版: GET  http://{}/sse  +  POST http://{}/message", addr, addr);
    info!("WebUI:    GET  http://{}/", addr);
    info!("Tools: {} | Prompts: {} | Resources: {}",
        registry.count(), prompts_count, resources_count);
    info!("============================================");

    // 启动后自动打开浏览器（不阻塞服务器启动）
    if app_config.server.auto_open_browser {
        let url = format!("http://{}", addr);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            match open_in_browser(&url).await {
                Ok(_) => info!("已自动打开浏览器: {}", url),
                Err(e) => tracing::warn!("打开浏览器失败: {}", e),
            }
        });
    }

    // 优雅关闭：Ctrl+C（双端）+ SIGTERM（Linux systemd / docker stop）。
    // axum 0.7 的 graceful shutdown 在信号触发后仍会等待全部连接排空（包括
    // 浏览器 keep-alive 长连接），可能无限拖延进程退出；因此并行一个
    // 「信号 + 10 秒」截止 future，到点后 main 返回、运行时丢弃剩余任务，
    // 由进程退出强制断开残留连接。
    let shutdown_rx_server = shutdown_rx.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal(shutdown_rx_server))
            .await
    });
    tokio::select! {
        res = server => {
            res.context("MCP 服务器任务异常终止")?
                .context("MCP 服务器运行错误")?;
            info!("👋 服务器已优雅退出");
        }
        _ = shutdown_then_deadline(shutdown_rx, logs.clone()) => {
            info!("⚠️ 排水超时（10 秒），服务器已强制退出");
        }
    }
    Ok(())
}

/// 跨平台打开浏览器。用 tokio::process（其孤儿进程回收器自动 waitpid 收割），
/// 替代 webbrowser crate——后者用 std::process 派生且不等待，会留下僵尸子进程。
async fn open_in_browser(url: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        tokio::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()?
            .wait()
            .await?;
    }
    #[cfg(target_os = "macos")]
    {
        tokio::process::Command::new("open").arg(url).spawn()?.wait().await?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        tokio::process::Command::new("xdg-open").arg(url).spawn()?.wait().await?;
    }
    Ok(())
}

/// 配置文件定位：exe 同目录 → 工作目录
fn resolve_config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("config.yaml")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("config.yaml"))
}

/// 关闭信号 + 排水截止：信号到来后最多再等 10 秒即触发强制退出
async fn shutdown_then_deadline(rx: tokio::sync::watch::Receiver<bool>, logs: Arc<LogSystem>) {
    shutdown_signal(rx).await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    // 此时浏览器等客户端的 SSE 连接多半还挂着（正是排水超时的原因），
    // 把强制退出原因经日志通道推给它们，浏览器就能看到退出原因而非无声断连
    logs.log("WARN", "⚠️ 优雅排水超时（10 秒），即将强制退出")
        .await;
}

/// 关闭信号：Ctrl+C（双端）+ SIGTERM（仅 Unix）+ WebUI 关闭按钮（watch 通道）
async fn shutdown_signal(mut rx: tokio::sync::watch::Receiver<bool>) {
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("注册 SIGTERM 处理器失败");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = sigterm.recv() => {},
            _ = rx.changed() => {},
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = rx.changed() => {},
        }
    }
    info!("🛑 收到关闭信号，正在优雅退出...");
}
