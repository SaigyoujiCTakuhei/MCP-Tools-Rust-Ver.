use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let (Some(a), Some(b)) = (args["a"].as_i64(), args["b"].as_i64()) else {
        return ToolOutput::err("参数 a 与 b 必须是整数");
    };
    ToolOutput::ok(json!({ "result": a.wrapping_add(b) }))
}

kzm_tool!(
    ToolDecl {
        name: "add".into(),
        title: Some("加法计算".into()),
        description: "执行加法运算，返回 a + b 的结果".into(),
        annotations: Some(ToolAnnotations::read_only()),
        category: Some("计算与统计".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "a": {"type": "integer", "description": "第一个加数"},
                "b": {"type": "integer", "description": "第二个加数"}
            },
            "required": ["a", "b"]
        }),
    },
    run
);
