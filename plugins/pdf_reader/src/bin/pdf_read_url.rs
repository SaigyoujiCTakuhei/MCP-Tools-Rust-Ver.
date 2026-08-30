use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

// 共享工具模块按二进制分别编译；本二进制只用 from_bytes 版本
#[allow(dead_code)]
#[path = "../pdf_utils.rs"]
mod pdf_utils;

fn run(args: Value) -> ToolOutput {
    let Some(url) = args["url"].as_str().map(str::to_string) else {
        return ToolOutput::err("缺少 url 参数");
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("构建 tokio runtime 失败");
    rt.block_on(async move {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolOutput::err(format!("创建 HTTP 客户端失败: {e}")),
        };
        let bytes = match client.get(&url).send().await {
            Ok(r) => match r.bytes().await {
                Ok(b) => b,
                Err(e) => return ToolOutput::err(format!("下载 PDF 失败: {e}")),
            },
            Err(e) => return ToolOutput::err(format!("下载 PDF 请求失败: {e}")),
        };
        match pdf_utils::extract_text_from_bytes(&bytes) {
            Ok(text) => ToolOutput::ok(json!({ "text": text, "size": bytes.len() })),
            Err(e) => ToolOutput::err(e),
        }
    })
}

kzm_tool!(
    ToolDecl {
        name: "pdf_read_url".into(),
        title: Some("读取网络 PDF".into()),
        description: "Extract text content from a PDF file hosted at a URL (downloads to memory).".into(),
        annotations: Some(ToolAnnotations::open_world_read_only()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "The URL of the PDF file."}
            },
            "required": ["url"]
        }),
    },
    run
);
