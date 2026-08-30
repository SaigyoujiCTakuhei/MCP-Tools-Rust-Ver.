use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(_args: Value) -> ToolOutput {
    let now = chrono::Local::now();
    ToolOutput::ok(json!({
        "date": now.format("%Y-%m-%d").to_string(),
        "weekday": now.format("%A").to_string(),
    }))
}

kzm_tool!(
    ToolDecl {
        name: "get_current_date".into(),
        title: Some("当前日期".into()),
        description: "获取当前日期（YYYY-MM-DD 格式）".into(),
        annotations: Some(ToolAnnotations::read_only()),
        input_schema: json!({ "type": "object", "properties": {} }),
    },
    run
);
