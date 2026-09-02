use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let Some(arr) = args["numbers"].as_array() else {
        return ToolOutput::err("numbers 必须是数组");
    };
    let numbers: Vec<f64> = arr.iter().filter_map(|v| v.as_f64()).collect();
    if numbers.is_empty() {
        return ToolOutput::err("numbers 数组不能为空");
    }
    let sum: f64 = numbers.iter().sum();
    let avg = sum / numbers.len() as f64;
    let max = numbers.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min = numbers.iter().cloned().fold(f64::INFINITY, f64::min);
    ToolOutput::ok(json!({
        "count": numbers.len(),
        "sum": sum,
        "average": avg,
        "max": max,
        "min": min,
    }))
}

kzm_tool!(
    ToolDecl {
        name: "calculate".into(),
        title: Some("数组统计".into()),
        description: "对给定的数字数组执行统计计算（求和、平均、最大、最小）。注意：内部为 64 位浮点，约 15-16 位有效数字，大整数会静默丢失精度（需精确大数请用 run_command 调 python3）".into(),
        annotations: Some(ToolAnnotations::read_only()),
        category: Some("计算与统计".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "numbers": {"type": "array", "items": {"type": "number"}, "description": "数字数组"}
            },
            "required": ["numbers"]
        }),
    },
    run
);
