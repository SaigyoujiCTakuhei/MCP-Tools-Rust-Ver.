use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let Some(text) = args["text"].as_str() else {
        return ToolOutput::err("缺少 text 参数");
    };
    let reversed: String = text.chars().rev().collect();
    ToolOutput::ok(json!({ "original": text, "reversed": reversed }))
}

kzm_tool!(
    ToolDecl {
        name: "reverse_text".into(),
        title: Some("文本反转".into()),
        description: "反转输入的文本字符串".into(),
        annotations: Some(ToolAnnotations::read_only()),
        category: Some("文本处理".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "要反转的文本"}
            },
            "required": ["text"]
        }),
    },
    run
);
