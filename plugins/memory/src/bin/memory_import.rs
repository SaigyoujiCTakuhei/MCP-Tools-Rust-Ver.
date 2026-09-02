//! kzm-memory-import — 批量导入记忆 Markdown 文件（移植 HY3 的 import_memories_to_pg.py）
//!
//! 与原脚本一致的部分：
//! - 来源发现：user_memory(~/.dsh/memory/MEMORY.md)、project_note(WS/MEMORY.md)、
//!   daily_log:日期(WS/20XX-XX-XX.md)、reflection:名称(WS/reflections/)、summary:名称(WS/summaries/)
//! - 分块：按 `## ` 标题切分（段长 >10 字符）；无标题时按空行分段
//! - 嵌入：bge-small-zh-v1.5，CLS 池化 + L2（fastembed Pooling::Cls，已与存量向量对齐 score=1.0）
//!
//! 与原脚本的有意差异：
//! - 原脚本 **DROP 重建**（会清掉 dbsync 钩子等其它来源写入的行）；本工具默认**增量跳过**
//!   （同 source+content 视为已存在），`rebuild_sources=true` 时按「本次涉及的来源」删除后重插，
//!   等价于原语义但不波及无关来源。
//! - 嵌入走 embed_texts 路由：批量 >5 条自动孵化常驻服务，模型只加载一次。

use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

// 共享库按二进制分别编译；未用到的部分是预期内的（豁免 dead_code）
#[allow(dead_code)]
#[path = "../lib.rs"]
mod memory;

const DEFAULT_WS_MEM: &str = "/home/p/.dsh/memory/workspaces/---home-p-AI Related--";
const DEFAULT_USER_MEM: &str = "/home/p/.dsh/memory/MEMORY.md";

/// 展开路径前缀 ~
fn expand(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var("HOME").ok() {
            return format!("{home}/{rest}");
        }
    }
    p.to_string()
}

/// 来源发现（对应原 discover()），返回 (路径, source, source_date)
fn discover(dir: &str, user_mem: &str) -> Vec<(std::path::PathBuf, String, Option<String>)> {
    let mut files = Vec::new();
    files.push((std::path::PathBuf::from(expand(user_mem)), "user_memory".into(), None));
    files.push((std::path::PathBuf::from(dir).join("MEMORY.md"), "project_note".into(), None));
    let entries = |sub: &str| -> Vec<std::path::PathBuf> {
        let mut v: Vec<std::path::PathBuf> = std::fs::read_dir(std::path::Path::new(dir).join(sub))
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    };
    // 带日期的日志：20XX-XX-XX.md
    for fp in entries(".") {
        let name = fp.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_dated = name.len() == 13
            && name.starts_with("20")
            && name.as_bytes()[4] == b'-'
            && name.as_bytes()[7] == b'-'
            && name.ends_with(".md");
        if is_dated {
            let d = name.trim_end_matches(".md").to_string();
            files.push((fp, format!("daily_log:{d}"), Some(d)));
        }
    }
    for fp in entries("reflections") {
        let stem = fp.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        files.push((fp, format!("reflection:{stem}"), Some(stem)));
    }
    for fp in entries("summaries") {
        let stem = fp.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        files.push((fp, format!("summary:{stem}"), None));
    }
    files
}

/// 分块（对应原 chunk_text）：## 标题分节，段长 >10；无标题则按空行分段
fn chunk_text(text: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut cur: Option<String> = None;
    for ln in text.lines() {
        if ln.starts_with("## ") {
            if let Some(c) = &cur {
                if c.trim().chars().count() > 10 {
                    chunks.push(c.trim().to_string());
                }
            }
            cur = Some(format!("{ln}\n"));
        } else if cur.is_none() {
            cur = Some(format!("{ln}\n"));
        } else {
            let c = cur.as_mut().unwrap();
            c.push_str(ln);
            c.push('\n');
        }
    }
    if let Some(c) = &cur {
        if c.trim().chars().count() > 10 {
            chunks.push(c.trim().to_string());
        }
    }
    if chunks.is_empty() {
        for p in text.split("\n\n") {
            let p = p.trim();
            if p.chars().count() > 10 {
                chunks.push(p.to_string());
            }
        }
    }
    chunks
}

fn run(args: Value) -> ToolOutput {
    let dir = args["dir"].as_str().unwrap_or(DEFAULT_WS_MEM).to_string();
    let user_mem = args["user_memory_path"].as_str().unwrap_or(DEFAULT_USER_MEM).to_string();
    let rebuild = args["rebuild_sources"].as_bool().unwrap_or(false);
    let batch_size = args["batch_size"].as_u64().unwrap_or(32).clamp(1, 256) as usize;

    // 1. 发现并分块
    let files = discover(&dir, &user_mem);
    let mut rows: Vec<(String, Option<String>, String)> = Vec::new();
    let mut file_count = 0usize;
    for (path, source, date) in &files {
        if !path.exists() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        file_count += 1;
        for ch in chunk_text(&text) {
            rows.push((source.clone(), date.clone(), ch));
        }
    }
    if rows.is_empty() {
        return ToolOutput::ok(json!({
            "files": 0, "chunks": 0, "skipped_existing": 0, "imported": 0,
            "note": "未发现可导入的记忆文件"
        }));
    }
    let sources: Vec<String> = {
        let mut s: Vec<String> = rows.iter().map(|(src, _, _)| src.clone()).collect();
        s.sort();
        s.dedup();
        s
    };

    // 2. 连库 + 幂等处理
    let mut client = match memory::connect() {
        Ok(c) => c,
        Err(e) => return ToolOutput::err(format!("{e:#}")),
    };
    if rebuild {
        if let Err(e) = client.execute(
            "DELETE FROM memory_chunks WHERE source = ANY($1)",
            &[&sources],
        ) {
            return ToolOutput::err(format!("重建清理失败: {e}"));
        }
    }
    // 已存在的 (source, content) 集合（增量跳过）
    let existing: std::collections::HashSet<(String, String)> = match client.query(
        "SELECT source, content FROM memory_chunks WHERE source = ANY($1)",
        &[&sources],
    ) {
        Ok(rs) => rs
            .iter()
            .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
            .collect(),
        Err(e) => return ToolOutput::err(format!("读取既有条目失败: {e}")),
    };

    let pending: Vec<&(String, Option<String>, String)> = rows
        .iter()
        .filter(|(src, _, content)| !existing.contains(&(src.clone(), content.clone())))
        .collect();
    let skipped = rows.len() - pending.len();
    if pending.is_empty() {
        return ToolOutput::ok(json!({
            "files": file_count, "chunks": rows.len(), "skipped_existing": skipped,
            "imported": 0, "sources": sources,
            "note": "全部条目已存在（增量跳过）"
        }));
    }

    // 3. 分批嵌入（embed_texts 路由：>5 条自动孵化/复用常驻服务）
    let mut embedder_state: Option<memory::Embedder> = None; // 冷启动回退用（保留会话复用）
    let mut vectors: Vec<Vec<f32>> = Vec::with_capacity(pending.len());
    for batch in pending.chunks(batch_size) {
        let texts: Vec<String> = batch.iter().map(|(_, _, c)| c.clone()).collect();
        let vecs = match memory::embed_texts(texts) {
            Ok(v) => v,
            Err(e) => return ToolOutput::err(format!("批量嵌入失败: {e:#}")),
        };
        let _ = &mut embedder_state;
        vectors.extend(vecs);
    }

    // 4. 写库
    let mut imported = 0usize;
    for ((src, date, content), vec) in pending.iter().zip(&vectors) {
        let literal = memory::to_pg_vector(vec);
        match client.execute(
            "INSERT INTO memory_chunks (source, source_date, content, embedding) \
             VALUES ($1, $2, $3, $4)",
            &[src, date, content, &literal],
        ) {
            Ok(_) => imported += 1,
            Err(e) => return ToolOutput::err(format!("写入失败（source={src}）: {e}")),
        }
    }

    ToolOutput::ok(json!({
        "files": file_count,
        "chunks": rows.len(),
        "skipped_existing": skipped,
        "imported": imported,
        "sources": sources,
        "rebuild": rebuild,
    }))
}

kzm_tool!(
    ToolDecl {
        name: "memory_import".into(),
        title: Some("批量导入记忆文件".into()),
        description: "把 dsh 记忆 Markdown（MEMORY.md / 日志 / 反思 / 总结）分块、向量化并导入长期记忆库（增量幂等）。".into(),
        annotations: Some(ToolAnnotations::writes()),
        category: Some("思考与记忆".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "dir": {"type": "string", "description": "记忆工作区目录（含 MEMORY.md、20XX-XX-XX.md、reflections/、summaries/）", "default": "/home/p/.dsh/memory/workspaces/---home-p-AI Related--"},
                "user_memory_path": {"type": "string", "description": "用户级记忆文件路径", "default": "/home/p/.dsh/memory/MEMORY.md"},
                "rebuild_sources": {"type": "boolean", "description": "true = 先删除本次涉及来源的旧行再重插（复刻原脚本 DROP 语义）；默认 false 增量跳过", "default": false},
                "batch_size": {"type": "integer", "description": "每批嵌入条数，默认 32", "default": 32}
            }
        }),
    },
    run
);
