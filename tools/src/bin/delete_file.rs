use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let Some(path) = args["path"].as_str() else {
        return ToolOutput::err("缺少 path 参数");
    };
    match std::fs::remove_file(path) {
        Ok(_) => ToolOutput::ok(json!({ "status": "deleted" })),
        Err(e) => ToolOutput::err(format!("删除文件失败: {e}")),
    }
}

kzm_tool!(
    ToolDecl {
        name: "delete_file".into(),
        title: Some("删除文件".into()),
        description: "删除指定文件".into(),
        annotations: Some(ToolAnnotations::destructive()),
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
