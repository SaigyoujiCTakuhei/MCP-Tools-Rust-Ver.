//! kzm-memory-remember — 存入长期记忆（对应 v11 的 memory_remember）
//!
//! 参数：content（必填）、source（可选，缺省 "manual"）、source_date（可选）
//! 行为：本地嵌入 content → 写入 memory_chunks → 返回新条目 id。

use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

// 共享库按二进制分别编译；未用到的部分是预期内的（豁免 dead_code）
#[allow(dead_code)]
#[path = "../lib.rs"]
mod memory;

fn run(args: Value) -> ToolOutput {
    let Some(content) = args["content"].as_str().map(str::trim).filter(|s| !s.is_empty()) else {
        return ToolOutput::err("缺少 content 参数（记忆原文不能为空）");
    };
    let source = args["source"].as_str().unwrap_or("manual");
    let source_date = args["source_date"].as_str();

    // 1. 嵌入（路由：≤5 条冷启动/热复用；常驻不可用回退冷启动）
    let vecs = match memory::embed_texts(vec![content.to_string()]) {
        Ok(v) => v,
        Err(e) => return ToolOutput::err(format!("{e:#}")),
    };
    let literal = memory::to_pg_vector(&vecs[0]);

    // 2. 写库
    let mut client = match memory::connect() {
        Ok(c) => c,
        Err(e) => return ToolOutput::err(format!("{e:#}")),
    };
    let row = client.query_opt(
        "INSERT INTO memory_chunks (source, source_date, content, embedding) \
         VALUES ($1, $2, $3, $4::vector) RETURNING id",
        &[&source, &source_date, &content, &literal],
    );
    match row {
        Ok(Some(row)) => {
            let id: i64 = row.get(0);
            ToolOutput::ok(json!({ "id": id, "source": source, "dims": memory::EMBED_DIMS }))
        }
        Ok(None) => ToolOutput::err("插入成功但未返回 id"),
        Err(e) => ToolOutput::err(format!("写入数据库失败: {e}")),
    }
}

kzm_tool!(
    ToolDecl {
        name: "memory_remember".into(),
        title: Some("存入长期记忆".into()),
        description: "把一条记忆（事实/偏好/事件/日志）存入长期记忆库（本地 PostgreSQL + pgvector），语义可检索。".into(),
        annotations: Some(ToolAnnotations::writes()),
        category: Some("思考与记忆".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "content": {"type": "string", "description": "记忆原文（建议一条一件事）"},
                "source": {"type": "string", "description": "来源标识，如 user_memory / daily_log:2026-08-30 / project_note", "default": "manual"},
                "source_date": {"type": "string", "description": "记忆发生日期（可选，YYYY-MM-DD）"}
            },
            "required": ["content"]
        }),
    },
    run
);
