/// 配置解析模块 — 读取 config.yaml
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub auto_open_browser: bool,
    /// 可选 Bearer Token：非空时 MCP 端点（/mcp、/sse、/message）要求
    /// Authorization: Bearer <token>；环境变量 MCP_AUTH_TOKEN 优先于本项
    #[serde(default)]
    pub auth_token: String,
    /// 除回环地址外额外放行的 Origin 白名单（如 llama.cpp UI 的来源 http://127.0.0.1:8080）
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsConfig {
    /// 插件工具扫描目录（含 kzm-* 可执行文件）；空 = 使用 exe 同目录
    #[serde(default)]
    pub discovery_path: String,
    #[serde(default = "default_timeout")]
    pub default_timeout: u64,
    /// 开发态源码监听：保存源码后自动编译并热装载对应工具（发布部署请保持 false）
    #[serde(default)]
    pub watch: bool,
}

/// MCP 数据文件（提示词 / 资源）目录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default = "default_prompts_path")]
    pub prompts_path: String,
    #[serde(default = "default_resources_path")]
    pub resources_path: String,
}

/// 模型上下文参数（上下文看板用）
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelCtx {
    /// 上下文窗口（token）
    #[serde(default = "default_ctx_window")]
    pub context_window: i64,
    /// 最大输出（token，触发公式里 min(out, 21k) 用）
    #[serde(default = "default_ctx_max_output")]
    pub max_output: i64,
}

impl Default for ModelCtx {
    fn default() -> Self {
        Self {
            context_window: default_ctx_window(),
            max_output: default_ctx_max_output(),
        }
    }
}

fn default_ctx_window() -> i64 {
    1_000_000
}
fn default_ctx_max_output() -> i64 {
    128_000
}

/// 上下文看板配置 — 全部字段有内置默认，config.yaml 不写 ctxboard 节即用默认值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtxBoardConfig {
    /// ZCode 引擎库 db.sqlite 路径（只读打开）
    #[serde(default = "default_ctx_db_path")]
    pub db_path: String,
    /// ZCode 会话流水目录（rollout，压缩事件结构化解析用）
    #[serde(default = "default_ctx_rollout_dir")]
    pub rollout_dir: String,
    /// 桌面壳任务索引（tasks-index.sqlite，会话存档/删除软标记的真源）
    #[serde(default = "default_ctx_tasks_index")]
    pub tasks_index_path: String,
    /// 模型上下文参数表：model_id → 参数；未登记的模型用 fallback_model
    #[serde(default = "default_ctx_models")]
    pub models: std::collections::HashMap<String, ModelCtx>,
    /// 未登记模型的兜底参数
    #[serde(default)]
    pub fallback_model: ModelCtx,
    /// 计费系数（估算积分 / 每百万 token）：model_id → [输入, 缓存读, 输出]；
    /// 未登记模型按 fallback_pricing
    #[serde(default = "default_ctx_pricing")]
    pub pricing: std::collections::HashMap<String, [f64; 3]>,
    #[serde(default = "default_ctx_fallback_pricing")]
    pub fallback_pricing: [f64; 3],
}

fn default_ctx_db_path() -> String {
    "~/.zcode/cli/db/db.sqlite".to_string()
}
fn default_ctx_rollout_dir() -> String {
    "~/.zcode/cli/rollout".to_string()
}
fn default_ctx_tasks_index() -> String {
    "~/.zcode/v2/tasks-index.sqlite".to_string()
}
fn default_ctx_models() -> std::collections::HashMap<String, ModelCtx> {
    std::collections::HashMap::from([
        ("GLM-5.3-Flash".to_string(), ModelCtx::default()),
        ("GLM-5.3".to_string(), ModelCtx::default()),
    ])
}
fn default_ctx_pricing() -> std::collections::HashMap<String, [f64; 3]> {
    // 2026-09 官方系数（积分/百万 token：输入 / 缓存读 / 输出），调价时改 config.yaml 即可
    std::collections::HashMap::from([
        ("GLM-5.3".to_string(), [6.9, 1.7, 24.0]),
        ("GLM-5.3-Flash".to_string(), [2.3, 0.56, 8.0]),
    ])
}
fn default_ctx_fallback_pricing() -> [f64; 3] {
    [2.3, 0.56, 8.0]
}

impl Default for CtxBoardConfig {
    fn default() -> Self {
        Self {
            db_path: default_ctx_db_path(),
            rollout_dir: default_ctx_rollout_dir(),
            tasks_index_path: default_ctx_tasks_index(),
            models: default_ctx_models(),
            fallback_model: ModelCtx::default(),
            pricing: default_ctx_pricing(),
            fallback_pricing: default_ctx_fallback_pricing(),
        }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            prompts_path: default_prompts_path(),
            resources_path: default_resources_path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default)]
    pub ctxboard: CtxBoardConfig,
}

fn default_port() -> u16 {
    58081
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_log_format() -> String {
    "json".to_string()
}
fn default_timeout() -> u64 {
    30
}
fn default_prompts_path() -> String {
    "mcp_data/prompts".to_string()
}
fn default_resources_path() -> String {
    "mcp_data/resources".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 58081,
                auto_open_browser: false,
                auth_token: String::new(),
                allowed_origins: Vec::new(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
            },
            tools: ToolsConfig {
                discovery_path: String::new(),
                default_timeout: 30,
                watch: false,
            },
            mcp: McpConfig {
                prompts_path: default_prompts_path(),
                resources_path: default_resources_path(),
            },
            ctxboard: CtxBoardConfig::default(),
        }
    }
}

/// 加载配置文件，若文件不存在则返回默认配置
pub async fn load_config(config_path: &Path) -> anyhow::Result<AppConfig> {
    if !config_path.exists() {
        tracing::warn!(
            path = %config_path.display(),
            "config.yaml 不存在，使用默认配置"
        );
        return Ok(AppConfig::default());
    }

    let content = fs_err::tokio::read_to_string(config_path).await
        .context("读取 config.yaml 失败")?;

    let config: AppConfig = serde_yaml::from_str(&content)
        .context("解析 config.yaml 失败")?;

    Ok(config)
}