use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let Some(path) = args["path"].as_str() else {
        return ToolOutput::err("缺少 path 参数");
    };
    match std::fs::create_dir_all(path) {
        Ok(_) => ToolOutput::ok(json!({ "status": "created" })),
        Err(e) => ToolOutput::err(format!("创建目录失败: {e}")),
    }
}

kzm_tool!(
    ToolDecl {
        name: "create_directory".into(),
        title: Some("创建目录".into()),
        description: "创建目录（递归创建父目录）".into(),
        annotations: Some(ToolAnnotations::writes()),
        category: Some("文件与目录".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "要创建的目录路径"}
            },
            "required": ["path"]
        }),
    },
    run
);
