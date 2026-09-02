use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let Some(path) = args["path"].as_str() else {
        return ToolOutput::err("缺少 path 参数");
    };
    match std::fs::read(path) {
        Ok(bytes) => {
            // lossy 解码：GBK 等 非 UTF-8 文件不再直接报错
            let content = String::from_utf8_lossy(&bytes).to_string();
            ToolOutput::ok(json!({ "content": content }))
        }
        Err(e) => ToolOutput::err(format!("读取文件失败: {e}")),
    }
}

kzm_tool!(
    ToolDecl {
        name: "read_file".into(),
        title: Some("读取文件".into()),
        description: "读取指定文件的内容".into(),
        annotations: Some(ToolAnnotations::read_only()),
        category: Some("文件与目录".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "文件路径（绝对或相对）"}
            },
            "required": ["path"]
        }),
    },
    run
);
