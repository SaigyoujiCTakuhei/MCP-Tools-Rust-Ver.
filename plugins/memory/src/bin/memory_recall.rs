//! kzm-memory-recall — 语义检索长期记忆（对应 v11 的 memory_recall）
//!
//! 参数：query（必填）、limit（缺省 5）、source（可选过滤）
//! 行为：嵌入查询 → pgvector HNSW 余弦最近邻 → 返回 [{id, score, source, content, created_at}]。

use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

#[path = "../lib.rs"]
mod memory;

fn run(args: Value) -> ToolOutput {
    let Some(query) = args["query"].as_str().map(str::trim).filter(|s| !s.is_empty()) else {
        return ToolOutput::err("缺少 query 参数（检索 query 不能为空）");
    };
    let limit = args["limit"].as_u64().unwrap_or(5).clamp(1, 50) as i64;
    let source = args["source"].as_str();

    let mut embedder = match memory::Embedder::new() {
        Ok(e) => e,
        Err(e) => return ToolOutput::err(format!("{e:#}")),
    };
    let vecs = match embedder.embed(vec![query.to_string()]) {
        Ok(v) => v,
        Err(e) => return ToolOutput::err(format!("{e:#}")),
    };
    let literal = memory::to_pg_vector(&vecs[0]);

    let mut client = match memory::connect() {
        Ok(c) => c,
        Err(e) => return ToolOutput::err(format!("{e:#}")),
    };

    // 余弦相似度 = 1 - 余弦距离（pgvector <=> 为余弦距离，值域 [0,2]）
    let (sql, params): (&str, Vec<&(dyn postgres::types::ToSql + Sync)>) = if source.is_some() {
        (
            "SELECT id, source, source_date, content, created_at, \
                    1 - (embedding <=> $1) AS score \
             FROM memory_chunks WHERE source = $2 \
             ORDER BY embedding <=> $1 LIMIT $3",
            vec![&literal, &source, &limit],
        )
    } else {
        (
            "SELECT id, source, source_date, content, created_at, \
                    1 - (embedding <=> $1) AS score \
             FROM memory_chunks \
             ORDER BY embedding <=> $1 LIMIT $2",
            vec![&literal, &limit],
        )
    };

    match client.query(sql, &params) {
        Ok(rows) => {
            let results: Vec<Value> = rows
                .iter()
                .map(|r| {
                    let score: f64 = r.get("score");
                    json!({
                        "id": r.get::<_, i64>("id"),
                        "score": (score * 10000.0).round() / 10000.0,
                        "source": r.get::<_, Option<String>>("source"),
                        "source_date": r.get::<_, Option<String>>("source_date"),
                        "content": r.get::<_, String>("content"),
                        "created_at": r.get::<_, Option<chrono::DateTime<chrono::Utc>>>("created_at")
                            .map(|t| t.to_rfc3339()),
                    })
                })
                .collect();
            ToolOutput::ok(json!({ "query": query, "count": results.len(), "results": results }))
        }
        Err(e) => ToolOutput::err(format!("检索失败: {e}")),
    }
}

kzm_tool!(
    ToolDecl {
        name: "memory_recall".into(),
        title: Some("检索长期记忆".into()),
        description: "按语义相似度检索长期记忆库（本地 PostgreSQL + pgvector，HNSW 索引）。".into(),
        annotations: Some(ToolAnnotations::read_only()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "自然语言检索语句"},
                "limit": {"type": "integer", "description": "返回条数上限，默认 5", "default": 5},
                "source": {"type": "string", "description": "按来源过滤（可选，如 user_memory / daily_log:2026-08-30）"}
            },
            "required": ["query"]
        }),
    },
    run
);
