use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let Some(path) = args["file_path"].as_str() else {
        return ToolOutput::err("缺少 file_path 参数");
    };
    let content = match std::fs::read(path) {
        Ok(b) => String::from_utf8_lossy(&b).to_string(),
        Err(e) => return ToolOutput::err(format!("读取文件失败: {e}")),
    };
    let (mut total, mut blank, mut comment, mut code) = (0u64, 0u64, 0u64, 0u64);
    for line in content.lines() {
        total += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank += 1;
        } else if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("--") {
            comment += 1;
        } else {
            code += 1;
        }
    }
    ToolOutput::ok(json!({
        "file": path,
        "total": total,
        "blank": blank,
        "comment": comment,
        "code": code,
    }))
}

kzm_tool!(
    ToolDecl {
        name: "count_lines".into(),
        title: Some("行数统计".into()),
        description: "统计指定文件的代码行数（空行、注释行、代码行）".into(),
        annotations: Some(ToolAnnotations::read_only()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "file_path": {"type": "string", "description": "文件路径"}
            },
            "required": ["file_path"]
        }),
    },
    run
);
