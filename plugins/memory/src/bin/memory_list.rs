//! kzm-memory-list — 浏览长期记忆（按时间倒序）
//!
//! 参数：source（可选过滤）、limit（缺省 20）、offset（缺省 0）
//! 返回条目含正文预览（前 200 字符），完整内容用 id 配合其他工具取用。

use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

// 共享库按二进制分别编译；本工具只用 db 部分
#[allow(dead_code)]
#[path = "../lib.rs"]
mod memory;

fn run(args: Value) -> ToolOutput {
    let limit = args["limit"].as_u64().unwrap_or(20).clamp(1, 100) as i64;
    let offset = args["offset"].as_u64().unwrap_or(0) as i64;
    let source = args["source"].as_str();

    let mut client = match memory::connect() {
        Ok(c) => c,
        Err(e) => return ToolOutput::err(format!("{e:#}")),
    };

    let (sql, params): (&str, Vec<&(dyn postgres::types::ToSql + Sync)>) = if source.is_some() {
        (
            "SELECT id, source, source_date, left(content, 200) AS preview, \
                    length(content) AS content_len, created_at \
             FROM memory_chunks WHERE source = $1 \
             ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3",
            vec![&source, &limit, &offset],
        )
    } else {
        (
            "SELECT id, source, source_date, left(content, 200) AS preview, \
                    length(content) AS content_len, created_at \
             FROM memory_chunks \
             ORDER BY created_at DESC, id DESC LIMIT $1 OFFSET $2",
            vec![&limit, &offset],
        )
    };

    match client.query(sql, &params) {
        Ok(rows) => {
            let items: Vec<Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "id": r.get::<_, i64>("id"),
                        "source": r.get::<_, Option<String>>("source"),
                        "source_date": r.get::<_, Option<String>>("source_date"),
                        "preview": r.get::<_, String>("preview"),
                        "content_len": r.get::<_, i32>("content_len"),
                        "created_at": r.get::<_, Option<chrono::DateTime<chrono::Utc>>>("created_at")
                            .map(|t| t.to_rfc3339()),
                    })
                })
                .collect();
            ToolOutput::ok(json!({ "count": items.len(), "offset": offset, "items": items }))
        }
        Err(e) => ToolOutput::err(format!("查询失败: {e}")),
    }
}

kzm_tool!(
    ToolDecl {
        name: "memory_list".into(),
        title: Some("浏览长期记忆".into()),
        description: "按时间倒序浏览长期记忆库条目（预览前 200 字符）。".into(),
        annotations: Some(ToolAnnotations::read_only()),
        category: Some("思考与记忆".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "source": {"type": "string", "description": "按来源过滤（可选）"},
                "limit": {"type": "integer", "description": "返回条数上限，默认 20", "default": 20},
                "offset": {"type": "integer", "description": "翻页偏移，默认 0", "default": 0}
            }
        }),
    },
    run
);
