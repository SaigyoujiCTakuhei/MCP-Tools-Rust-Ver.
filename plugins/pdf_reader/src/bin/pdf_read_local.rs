use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

#[path = "../pdf_utils.rs"]
mod pdf_utils;

fn run(args: Value) -> ToolOutput {
    let Some(path) = args["path"].as_str() else {
        return ToolOutput::err("缺少 path 参数");
    };
    if !std::path::Path::new(path).exists() {
        return ToolOutput::err(format!("PDF file not found: {path}"));
    }
    match pdf_utils::extract_text_from_file(path) {
        Ok(text) => ToolOutput::ok(json!({ "text": text })),
        Err(e) => ToolOutput::err(e),
    }
}

kzm_tool!(
    ToolDecl {
        name: "pdf_read_local".into(),
        title: Some("读取本地 PDF".into()),
        description: "Read text content from a local PDF file. Supports path traversal within local filesystem.".into(),
        annotations: Some(ToolAnnotations::read_only()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "The absolute or relative path to the local PDF file."}
            },
            "required": ["path"]
        }),
    },
    run
);
