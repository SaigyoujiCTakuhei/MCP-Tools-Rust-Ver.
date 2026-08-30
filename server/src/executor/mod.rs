/// 工具执行器 trait 与结果类型
///
/// 现在所有工具均为子进程插件（见 mcp/plugins.rs 的 PluginExecutor），
/// trait 保留作为统一抽象入口，便于未来增加原生/其他形态的执行器。
use async_trait::async_trait;
use serde::Serialize;

/// 工具执行结果（与 tool_kit::ToolOutput 线协议字段对齐）
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct ToolResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    /// 失败结果
    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

/// 工具执行器 trait —— 所有工具的抽象执行入口
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// 执行工具，返回 ToolResult
    async fn execute(&self, args: serde_json::Value) -> ToolResult;
}
