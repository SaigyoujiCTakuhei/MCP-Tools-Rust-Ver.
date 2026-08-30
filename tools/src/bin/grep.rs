use regex::Regex;
use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let (Some(pattern), Some(file_path)) = (args["pattern"].as_str(), args["file_path"].as_str())
    else {
        return ToolOutput::err("缺少 pattern 或 file_path 参数");
    };
    let max_matches = args["max_matches"].as_u64().unwrap_or(50) as usize;

    let re = match Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return ToolOutput::err(format!("正则表达式无效: {e}")),
    };
    // 读字节后 lossy 解码：GBK 等 非 UTF-8 文件不再直接报错
    let content = match std::fs::read(file_path) {
        Ok(b) => String::from_utf8_lossy(&b).to_string(),
        Err(e) => return ToolOutput::err(format!("读取文件失败: {e}")),
    };
    let matches: Vec<Value> = content
        .lines()
        .enumerate()
        .filter(|(_, line)| re.is_match(line))
        .take(max_matches)
        .map(|(idx, line)| json!({ "line": idx + 1, "content": line }))
        .collect();
    ToolOutput::ok(json!({
        "file": file_path,
        "pattern": pattern,
        "count": matches.len(),
        "matches": matches,
    }))
}

kzm_tool!(
    ToolDecl {
        name: "grep".into(),
        title: Some("文件内容搜索".into()),
        description: "在指定文件中使用正则表达式搜索匹配的行".into(),
        annotations: Some(ToolAnnotations::read_only()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "正则表达式模式"},
                "file_path": {"type": "string", "description": "目标文件路径"},
                "max_matches": {"type": "integer", "description": "最大返回数，默认 50", "default": 50}
            },
            "required": ["pattern", "file_path"]
        }),
    },
    run
);
