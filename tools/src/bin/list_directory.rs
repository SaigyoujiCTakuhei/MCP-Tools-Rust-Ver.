use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let path = args["path"].as_str().unwrap_or(".");
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(e) => return ToolOutput::err(format!("读取目录失败: {e}")),
    };
    let mut items = Vec::new();
    for entry in entries.flatten() {
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        items.push(json!({
            "name": entry.file_name().to_string_lossy(),
            "is_dir": is_dir,
        }));
    }
    ToolOutput::ok(json!({ "path": path, "count": items.len(), "entries": items }))
}

kzm_tool!(
    ToolDecl {
        name: "list_directory".into(),
        title: Some("目录列表".into()),
        description: "列出指定目录下的所有文件和子目录".into(),
        annotations: Some(ToolAnnotations::read_only()),
        category: Some("文件与目录".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "目录路径，默认为当前目录", "default": "."}
            }
        }),
    },
    run
);
