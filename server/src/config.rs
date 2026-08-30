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
}

/// MCP 数据文件（提示词 / 资源）目录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default = "default_prompts_path")]
    pub prompts_path: String,
    #[serde(default = "default_resources_path")]
    pub resources_path: String,
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
            },
            mcp: McpConfig {
                prompts_path: default_prompts_path(),
                resources_path: default_resources_path(),
            },
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