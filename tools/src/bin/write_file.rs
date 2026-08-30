use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};
use std::path::Path;

fn run(args: Value) -> ToolOutput {
    let (Some(path), Some(content)) = (args["path"].as_str(), args["content"].as_str()) else {
        return ToolOutput::err("缺少 path 或 content 参数");
    };
    // 自动创建父目录
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return ToolOutput::err(format!("创建父目录失败: {e}"));
            }
        }
    }
    match std::fs::write(path, content) {
        Ok(_) => ToolOutput::ok(json!({ "status": "written" })),
        Err(e) => ToolOutput::err(format!("写入文件失败: {e}")),
    }
}

kzm_tool!(
    ToolDecl {
        name: "write_file".into(),
        title: Some("写入文件".into()),
        description: "将内容写入指定文件（覆盖模式）".into(),
        annotations: Some(ToolAnnotations::destructive()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "文件路径"},
                "content": {"type": "string", "description": "要写入的内容"}
            },
            "required": ["path", "content"]
        }),
    },
    run
);
