/// 数据库调用指南工具 — 泛化的库级内省（不限于记忆表）
///
/// 返回：库标识、全部 public 表（用途/行数估算/列结构）与访问约定。
/// 目的：让 LLM 在新会话里调用本工具一次即获得完整数据库认知，
/// 替代反复的 psql 探测。表用途在 PURPOSES 中登记；未登记的表显示「用途未登记」，
/// 新表（如未来的 RAG 表）建好后自动出现在本指南中。
use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

#[path = "../lib.rs"]
mod memory;

/// 表用途登记（建新表后在此补一行；未登记显示「用途未登记」）
fn table_purpose(name: &str) -> &'static str {
    match name {
        "memory_chunks" => {
            "长期记忆（向量 512 维，HNSW 余弦索引）。dsh 记忆插件与 kzm-memory-* 工具共用：\
             写入用 memory_remember，语义检索用 memory_recall，浏览/删除用 memory_list / memory_forget"
        }
        _ => "用途未登记",
    }
}

fn run(_args: Value) -> ToolOutput {
    let mut client = match memory::connect() {
        Ok(c) => c,
        Err(e) => return ToolOutput::err(format!("{e:#}")),
    };

    // 全部 public 表
    let tables = match client.query(
        "SELECT tablename FROM pg_tables \
         WHERE schemaname = 'public' ORDER BY tablename",
        &[],
    ) {
        Ok(rows) => rows,
        Err(e) => return ToolOutput::err(format!("查询表清单失败: {e}")),
    };

    let mut tables_out: Vec<Value> = Vec::new();
    for row in &tables {
        let name: String = row.get(0);
        // 行数（pg_class 估算值，避免对大表全量 COUNT）
        let rows_est: i64 = client
            .query_one(
                "SELECT GREATEST(reltuples, 0)::bigint FROM pg_class \
                 WHERE relname = $1 AND relnamespace = 'public'::regnamespace",
                &[&name],
            )
            .map(|r| r.get(0))
            .unwrap_or(0);
        let columns: Vec<Value> = client
            .query(
                "SELECT column_name, data_type FROM information_schema.columns \
                 WHERE table_schema = 'public' AND table_name = $1 \
                 ORDER BY ordinal_position",
                &[&name],
            )
            .map(|rows| {
                rows.iter()
                    .map(|r| {
                        json!({
                            "name": r.get::<_, String>(0),
                            "type": r.get::<_, String>(1),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        tables_out.push(json!({
            "name": name,
            "purpose": table_purpose(&name),
            "rows_estimate": rows_est,
            "columns": columns,
        }));
    }

    ToolOutput::ok(json!({
        "database": "Agent_Memories",
        "host": "本机 PostgreSQL 17（Unix socket；psql -U p -d Agent_Memories 免密）",
        "tables": tables_out,
        "conventions": [
            "记忆写入：memory_remember；语义检索：memory_recall；浏览与删除：memory_list / memory_forget",
            "本机直连 SQL：psql -U p -d Agent_Memories（无需密码）",
            "共享表纪律：memory_chunks 由 dsh 记忆插件与本服务器共用——严禁按 source 批量 DELETE",
            "SQL 里访问向量列需 pgvector 类型；相似度用 embedding <=> $1（余弦距离）",
            "RAG 等新表就绪后建表并登记用途，即自动出现在本指南"
        ],
    }))
}

kzm_tool!(
    ToolDecl {
        name: "db_guide".into(),
        title: Some("数据库调用指南".into()),
        category: Some("数据库".into()),
        description: "返回本机 PostgreSQL（Agent_Memories 库）的全部表结构、用途、行数与访问约定。涉及数据库、记忆或 RAG 的操作前请先调用本工具，无需再用 psql 探测表结构。".into(),
        annotations: Some(ToolAnnotations::read_only()),
        input_schema: json!({ "type": "object", "properties": {} }),
    },
    run
);
