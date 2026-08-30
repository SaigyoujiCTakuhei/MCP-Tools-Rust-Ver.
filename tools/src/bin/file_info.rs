use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let Some(path) = args["path"].as_str() else {
        return ToolOutput::err("缺少 path 参数");
    };
    match std::fs::metadata(path) {
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            ToolOutput::ok(json!({
                "size": meta.len(),
                "is_file": meta.is_file(),
                "is_dir": meta.is_dir(),
                "modified_unix": modified,
            }))
        }
        Err(e) => ToolOutput::err(format!("获取文件信息失败: {e}")),
    }
}

kzm_tool!(
    ToolDecl {
        name: "file_info".into(),
        title: Some("文件元数据".into()),
        description: "获取文件的元数据信息（大小、类型、修改时间）".into(),
        annotations: Some(ToolAnnotations::read_only()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "文件路径"}
            },
            "required": ["path"]
        }),
    },
    run
);
