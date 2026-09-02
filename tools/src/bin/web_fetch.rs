use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let Some(url) = args["url"].as_str().map(str::to_string) else {
        return ToolOutput::err("缺少 url 参数");
    };
    let method = args["method"].as_str().unwrap_or("GET").to_string();
    let body = args["body"].as_str().map(str::to_string);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("构建 tokio runtime 失败");
    rt.block_on(async move {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolOutput::err(format!("创建 HTTP 客户端失败: {e}")),
        };
        let response = match method.to_uppercase().as_str() {
            "POST" => {
                let mut req = client.post(&url);
                if let Some(b) = body {
                    req = req.body(b);
                }
                match req.send().await {
                    Ok(r) => r,
                    Err(e) => return ToolOutput::err(format!("请求失败: {e}")),
                }
            }
            _ => match client.get(&url).send().await {
                Ok(r) => r,
                Err(e) => return ToolOutput::err(format!("请求失败: {e}")),
            },
        };
        let status = response.status().as_u16();
        let headers: serde_json::Map<String, Value> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_str().unwrap_or("").to_string())))
            .collect();
        match response.text().await {
            Ok(text) => ToolOutput::ok(json!({
                "status": status,
                "headers": headers,
                "body": text,
            })),
            Err(e) => ToolOutput::err(format!("读取响应失败: {e}")),
        }
    })
}

kzm_tool!(
    ToolDecl {
        name: "web_fetch".into(),
        title: Some("网页抓取".into()),
        description: "抓取指定 URL 的网页内容".into(),
        annotations: Some(ToolAnnotations::open_world()),
        category: Some("网络".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "目标 URL"},
                "method": {"type": "string", "description": "HTTP 方法 (GET/POST)，默认 GET", "default": "GET"},
                "body": {"type": "string", "description": "POST 请求体（JSON 字符串），仅 POST 时使用"}
            },
            "required": ["url"]
        }),
    },
    run
);
