use serde_json::Value;
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let name = args["name"].as_str().unwrap_or("World");
    ToolOutput::ok(serde_json::json!({ "message": format!("Hello, {name}! 【监听器自动编译版】") }))
}

kzm_tool!(
    ToolDecl {
        name: "hello_world".into(),
        title: Some("问候语".into()),
        description: "返回一条简单的问候消息（测试用基础工具）".into(),
        annotations: Some(ToolAnnotations::read_only()),
        category: Some("文本处理".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "名字（可选）"}
            }
        }),
    },
    run
);
