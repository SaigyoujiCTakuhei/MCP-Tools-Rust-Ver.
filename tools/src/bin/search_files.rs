use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(args: Value) -> ToolOutput {
    let (Some(pattern), path) = (args["pattern"].as_str(), args["path"].as_str().unwrap_or("."))
    else {
        return ToolOutput::err("缺少 pattern 参数");
    };
    // 路径统一 '/' 分隔：Windows 反斜杠路径在 glob 模式里有转义歧义
    let base = path.replace('\\', "/");
    let base = base.trim_end_matches('/');
    let glob_pattern = if base.is_empty() || base == "." {
        pattern.to_string()
    } else {
        format!("{base}/{pattern}")
    };
    let entries = match glob::glob(&glob_pattern) {
        Ok(e) => e,
        Err(e) => return ToolOutput::err(format!("glob 模式无效: {e}")),
    };
    let files: Vec<Value> = entries
        .filter_map(|r| r.ok())
        .map(|p| json!({ "path": p.display().to_string() }))
        .collect();
    ToolOutput::ok(json!({
        "pattern": pattern,
        "count": files.len(),
        "files": files,
    }))
}

kzm_tool!(
    ToolDecl {
        name: "search_files".into(),
        title: Some("文件名搜索".into()),
        description: "在目录中搜索匹配通配符模式的文件名".into(),
        annotations: Some(ToolAnnotations::read_only()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "glob 模式（例: *.py, *config*）"},
                "path": {"type": "string", "description": "搜索根目录，默认当前目录", "default": "."}
            },
            "required": ["pattern"]
        }),
    },
    run
);
