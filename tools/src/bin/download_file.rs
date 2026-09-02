use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};
use std::path::Path;

fn run(args: Value) -> ToolOutput {
    let (Some(url), Some(save_path)) = (
        args["url"].as_str().map(str::to_string),
        args["save_path"].as_str().map(str::to_string),
    ) else {
        return ToolOutput::err("缺少 url 或 save_path 参数");
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
                Err(e) => return ToolOutput::err(format!("读取下载数据失败: {e}")),
            },
            Err(e) => return ToolOutput::err(format!("下载请求失败: {e}")),
        };
        if let Some(parent) = Path::new(&save_path).parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    return ToolOutput::err(format!("创建父目录失败: {e}"));
                }
            }
        }
        match tokio::fs::write(&save_path, &bytes).await {
            Ok(_) => ToolOutput::ok(json!({
                "status": "downloaded",
                "path": save_path,
                "size": bytes.len(),
            })),
            Err(e) => ToolOutput::err(format!("写入文件失败: {e}")),
        }
    })
}

kzm_tool!(
    ToolDecl {
        name: "download_file".into(),
        title: Some("下载文件".into()),
        description: "从指定 URL 下载文件到本地路径".into(),
        annotations: Some(ToolAnnotations::writes()),
        category: Some("网络".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "文件 URL"},
                "save_path": {"type": "string", "description": "本地保存路径"}
            },
            "required": ["url", "save_path"]
        }),
    },
    run
);
