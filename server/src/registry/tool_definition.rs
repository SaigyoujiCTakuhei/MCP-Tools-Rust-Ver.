/// MCP Tool 的完整定义
use serde::{Deserialize, Serialize};

/// MCP Tool 完整定义（对应 Python 版 ToolDefinition dataclass）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// 工具唯一标识（对应文件名/工具名）
    pub name: String,
    /// 功能描述
    pub description: String,
    /// JSON Schema 入参定义
    pub input_schema: serde_json::Value,
    /// 人类可读标题
    pub title: Option<String>,
    /// MCP 协议注解
    pub annotations: Option<ToolAnnotations>,
    /// 是否启用（热插拔开关）
    pub enabled: bool,
    /// 来源模块标识
    pub source: ToolSource,
}

/// MCP 协议注解（对标 MCP ToolAnnotations；协议字段名为 camelCase）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

/// 工具来源分类
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ToolSource {
    Native,
    Browser,
    Subprocess,
    Plugin,
}

impl ToolDefinition {
    /// 生成 MCP 协议所需的工具描述 JSON（title / annotations 存在时一并输出）
    pub fn to_mcp_tool_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        });
        if let Some(title) = &self.title {
            v["title"] = serde_json::json!(title);
        }
        if let Some(annotations) = &self.annotations {
            if let Ok(a) = serde_json::to_value(annotations) {
                v["annotations"] = a;
            }
        }
        v
    }
}