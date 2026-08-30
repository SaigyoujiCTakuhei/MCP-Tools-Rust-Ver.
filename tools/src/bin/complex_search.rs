use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let Some(text) = args["text"].as_str().map(str::to_string) else {
        return ToolOutput::err("缺少 text 参数");
    };
    let directory = args["directory"].as_str().unwrap_or(".").replace('\\', "/");
    let file_glob = args["file_glob"].as_str().map(str::to_string);
    let max_results = args["max_results"].as_u64().unwrap_or(30) as usize;

    let glob_pattern = format!("{}/**/*", directory.trim_end_matches('/'));
    let entries = match glob::glob(&glob_pattern) {
        Ok(e) => e,
        Err(e) => return ToolOutput::err(format!("glob 失败: {e}")),
    };
    let mut results = Vec::new();
    for entry in entries {
        let Ok(path) = entry else { continue };
        if !path.is_file() {
            continue;
        }
        if let Some(fg) = &file_glob {
            let ok = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|fname| {
                    glob::Pattern::new(fg)
                        .map(|p| p.matches(fname))
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if !ok {
                continue;
            }
        }
        // 读字节后 lossy 解码：GBK 等 非 UTF-8 文件不再被当作二进制跳过
        let content = match std::fs::read(&path) {
            Ok(b) => String::from_utf8_lossy(&b).to_string(),
            Err(_) => continue,
        };
        let lines: Vec<Value> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(&text))
            .take(5) // 每个文件最多 5 行
            .map(|(idx, line)| json!({ "line": idx + 1, "content": line }))
            .collect();
        if !lines.is_empty() {
            results.push(json!({
                "file": path.display().to_string(),
                "match_count": lines.len(),
                "lines": lines,
            }));
            if results.len() >= max_results {
                break;
            }
        }
    }
    ToolOutput::ok(json!({
        "text": text,
        "total_files": results.len(),
        "results": results,
    }))
}

kzm_tool!(
    ToolDecl {
        name: "complex_search".into(),
        title: Some("递归文本搜索".into()),
        description: "在目录下递归搜索包含指定文本内容的文件".into(),
        annotations: Some(ToolAnnotations::read_only()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "搜索的文本内容"},
                "directory": {"type": "string", "description": "搜索目录，默认当前目录", "default": "."},
                "file_glob": {"type": "string", "description": "可选文件名过滤 glob（例: *.py）"},
                "max_results": {"type": "integer", "description": "最大结果数，默认 30", "default": 30}
            },
            "required": ["text"]
        }),
    },
    run
);
