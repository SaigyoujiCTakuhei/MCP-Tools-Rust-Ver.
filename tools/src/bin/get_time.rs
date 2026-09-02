use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};
use std::time::{SystemTime, UNIX_EPOCH};

fn run(_args: Value) -> ToolOutput {
    let now = chrono::Local::now();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    ToolOutput::ok(json!({
        "time": now.format("%H:%M:%S").to_string(),
        "timestamp": timestamp,
    }))
}

kzm_tool!(
    ToolDecl {
        name: "get_time".into(),
        title: Some("当前时间".into()),
        description: "获取当前时间（HH:MM:SS 格式）和 Unix 时间戳".into(),
        annotations: Some(ToolAnnotations::read_only()),
        category: Some("系统与命令".into()),
        input_schema: json!({ "type": "object", "properties": {} }),
    },
    run
);
