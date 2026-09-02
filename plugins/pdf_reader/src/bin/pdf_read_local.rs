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
        description: "从本地 PDF 文件提取文本内容（支持任意本地路径）。".into(),
        annotations: Some(ToolAnnotations::read_only()),
        category: Some("PDF 文档".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "本地 PDF 文件路径（绝对或相对）"}
            },
            "required": ["path"]
        }),
    },
    run
);
