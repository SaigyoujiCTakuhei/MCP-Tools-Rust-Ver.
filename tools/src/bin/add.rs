use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let (Some(a), Some(b)) = (args["a"].as_i64(), args["b"].as_i64()) else {
        return ToolOutput::err("参数 a 与 b 必须是 64 位整数（约 ±9.22×10^18；更大的数请用 run_command 调 python3）");
    };
    match a.checked_add(b) {
        Some(sum) => ToolOutput::ok(json!({ "result": sum })),
        None => ToolOutput::err(format!(
            "{a} + {b} 的结果溢出 64 位整数范围；请用 run_command 调 python3 做大数精确运算"
        )),
    }
}

kzm_tool!(
    ToolDecl {
        name: "add".into(),
        title: Some("加法计算".into()),
        description: "执行加法运算，返回 a + b 的结果（输入与结果限 64 位整数，约 ±9.22×10^18；超出请用 run_command 调 python3）".into(),
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
