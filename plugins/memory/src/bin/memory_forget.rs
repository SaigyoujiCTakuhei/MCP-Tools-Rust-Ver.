//! kzm-memory-forget — 永久删除记忆条目（对应 v11 的 memory_forget）
//!
//! 参数：id（必填）。删除前回显被删条目摘要，便于确认删对了。

use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

// 共享库按二进制分别编译；本工具只用 db 部分
#[allow(dead_code)]
#[path = "../lib.rs"]
mod memory;

fn run(args: Value) -> ToolOutput {
    let Some(id) = args["id"].as_i64() else {
        return ToolOutput::err("缺少 id 参数（要删除的记忆条目 id）");
    };

    let mut client = match memory::connect() {
        Ok(c) => c,
        Err(e) => return ToolOutput::err(format!("{e:#}")),
    };

    // 先取摘要用于回显
    let preview = client
        .query_opt(
            "SELECT source, left(content, 120) AS preview FROM memory_chunks WHERE id = $1",
            &[&id],
        )
        .ok()
        .flatten();

    match client.execute("DELETE FROM memory_chunks WHERE id = $1", &[&id]) {
        Ok(n) if n > 0 => {
            let info = preview
                .map(|r| {
                    format!(
                        "source={:?} preview={:?}",
                        r.get::<_, Option<String>>("source"),
                        r.get::<_, String>("preview")
                    )
                })
                .unwrap_or_default();
            ToolOutput::ok(json!({ "deleted": id, "info": info }))
        }
        Ok(_) => ToolOutput::err(format!("id {id} 不存在")),
        Err(e) => ToolOutput::err(format!("删除失败: {e}")),
    }
}

kzm_tool!(
    ToolDecl {
        name: "memory_forget".into(),
        title: Some("遗忘记忆".into()),
        description: "永久删除一条长期记忆条目（按 id，不可恢复）。".into(),
        annotations: Some(ToolAnnotations::destructive()),
        category: Some("思考与记忆".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "id": {"type": "integer", "description": "要删除的记忆条目 id"}
            },
            "required": ["id"]
        }),
    },
    run
);
