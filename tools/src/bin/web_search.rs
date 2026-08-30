use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let Some(query) = args["query"].as_str().map(str::to_string) else {
        return ToolOutput::err("缺少 query 参数");
    };
    let limit = args["limit"].as_u64().unwrap_or(5);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("构建 tokio runtime 失败");
    rt.block_on(async move {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolOutput::err(format!("创建 HTTP 客户端失败: {e}")),
        };
        // DuckDuckGo Instant Answer API
        let resp = match client
            .get("https://api.duckduckgo.com/")
            .query(&[
                ("q", query.as_str()),
                ("format", "json"),
                ("no_html", "1"),
                ("skip_disambig", "1"),
            ])
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolOutput::err(format!("搜索请求失败: {e}")),
        };
        let doc: Value = match resp.json().await {
            Ok(d) => d,
            Err(e) => return ToolOutput::err(format!("解析搜索结果失败: {e}")),
        };
        let empty = Vec::new();
        let related: Vec<String> = doc["RelatedTopics"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .take(limit as usize)
            .filter_map(|r| r["Text"].as_str().map(str::to_string))
            .collect();
        let abstract_text = doc["AbstractText"].as_str().unwrap_or("");
        ToolOutput::ok(json!({
            "query": query,
            "abstract": abstract_text,
            "results": related,
            "count": related.len(),
        }))
    })
}

kzm_tool!(
    ToolDecl {
        name: "web_search".into(),
        title: Some("网络搜索".into()),
        description: "通过网络搜索查询关键词（使用 DuckDuckGo 零点击 API）".into(),
        annotations: Some(ToolAnnotations::open_world_read_only()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "搜索关键词"},
                "limit": {"type": "integer", "description": "返回结果数上限，默认 5", "default": 5}
            },
            "required": ["query"]
        }),
    },
    run
);
