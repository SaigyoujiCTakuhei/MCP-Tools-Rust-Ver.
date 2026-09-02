//! kzm-sequentialthinking — 工具注册薄壳（对应 v11 的 scripts/tool_register.py）
//!
//! 职责：Pydantic 式输入校验 → 状态加载（显式 sessionId 句柄）→ 委托 thinking_core
//! → 状态持久化 → stderr 框图日志（DISABLE_THOUGHT_LOGGING 可关）。
//!
//! 与 v11 的差异：状态从 thread_local 改为 mcp_data/sequential_thinking/state-<sessionId>.json，
//! 可选参数 sessionId 用于多会话续链（MCP Stateful Tools 的显式 handle 模式）。

use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

#[path = "../thinking_core.rs"]
mod thinking_core;
#[path = "../config.rs"]
mod config;

use thinking_core::ThoughtState;

fn run(args: Value) -> ToolOutput {
    // ===== 1. 输入校验（对应 v11 的 SequentialThoughtInput Pydantic 模型） =====
    if let Err(msg) = validate(&args) {
        return validation_error(&msg);
    }

    // thought 规范化：去空白、超长截断（v11 THOUGHT_MAX_LEN=4000），
    // 截断结果写回参数（核心处理读的是 args 里的 thought）
    let mut thought = args["thought"].as_str().unwrap_or("").trim().to_string();
    if thought.chars().count() > config::THOUGHT_MAX_LEN {
        let cut: String = thought.chars().take(config::THOUGHT_MAX_LEN).collect();
        thought = format!("{cut}\n[...truncated by MCP server]");
    }
    let mut args = args;
    args["thought"] = json!(thought);

    // 会话句柄：同 sessionId 的调用共享思维链状态
    let session = args["sessionId"].as_str().unwrap_or("default");

    // ===== 2. 加载会话状态（显式句柄 + 磁盘持久化） =====
    let mut state = load_state(session);

    // ===== 3. 委托核心处理 =====
    let snapshot = match thinking_core::process_thought(&mut state, &args) {
        Ok(s) => s,
        Err(msg) => return runtime_error(&msg),
    };

    // 思考步骤框图 → stderr（v11 行为；DISABLE_THOUGHT_LOGGING=true 可关）
    if !config::disable_thought_logging() {
        if let Some(data) = state.thought_history.last() {
            eprintln!("{}", thinking_core::format_thought_box(data));
        }
    }

    // ===== 4. 持久化会话状态 =====
    if let Err(e) = save_state(session, &state) {
        return runtime_error(&format!("状态持久化失败: {e}"));
    }

    ToolOutput::ok(json!({
        "sessionId": session,
        "snapshot": snapshot,
    }))
}

/// Pydantic 式校验：必填项 + 数值下界（thought 的截断在校验后进行）
fn validate(args: &Value) -> Result<(), String> {
    let missing = |name: &str| format!("'{name}' parameter is required");
    let thought = args["thought"].as_str().map(str::trim).unwrap_or("");
    if thought.is_empty() {
        return Err("'thought' parameter is empty or whitespace-only".into());
    }
    for name in ["nextThoughtNeeded", "thoughtNumber", "totalThoughts"] {
        if args.get(name).map(Value::is_null).unwrap_or(true) {
            return Err(missing(name));
        }
    }
    for name in ["thoughtNumber", "totalThoughts"] {
        match thinking_core_coerce_i64(args.get(name)) {
            Some(n) if n >= 1 => {}
            _ => return Err(format!("'{name}' must be an integer >= 1")),
        }
    }
    Ok(())
}

// 供 validate 使用的数字转换（与 thinking_core 的 coerce 同规则）
fn thinking_core_coerce_i64(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

fn validation_error(msg: &str) -> ToolOutput {
    // 对应 v11 的 ErrorResponse { error: "Validation error", details: { message } }
    ToolOutput::err(json!({
        "error": "Validation error",
        "status": "failed",
        "details": { "message": msg },
    }).to_string())
}

fn runtime_error(msg: &str) -> ToolOutput {
    ToolOutput::err(json!({
        "error": "Runtime error",
        "status": "failed",
        "details": { "message": msg },
    }).to_string())
}

/// 会话状态文件路径：mcp_data/sequential_thinking/state-<sessionId>.json
/// （sessionId 做文件名安全化，防路径注入）
fn state_path(session: &str) -> std::path::PathBuf {
    let safe: String = session
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    std::path::Path::new(config::STATE_DIR).join(format!("state-{safe}.json"))
}

fn load_state(session: &str) -> ThoughtState {
    std::fs::read_to_string(state_path(session))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(ThoughtState::new)
}

fn save_state(session: &str, state: &ThoughtState) -> std::io::Result<()> {
    let path = state_path(session);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // 临时文件 + 原子改名，避免并发调用读到半截状态
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(state).unwrap_or_default())?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

kzm_tool!(
    ToolDecl {
        name: "sequentialthinking".into(),
        title: Some("序列化思考".into()),
        description: "通过迭代式思考步骤进行动态问题求解，支持线性推进、分支探索与自我修正。".into(),
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        }),
        category: Some("思考与记忆".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "thought": {"type": "string", "description": "当前的思考步骤内容"},
                "nextThoughtNeeded": {"type": "boolean", "description": "是否还需要下一步思考"},
                "thoughtNumber": {"type": "integer", "minimum": 1, "description": "当前思考步骤编号（如 1、2、3）"},
                "totalThoughts": {"type": "integer", "minimum": 1, "description": "预估所需思考总步数（如 5、10）"},
                "isRevision": {"type": "boolean", "description": "是否在修正之前的思考"},
                "revisesThought": {"type": "integer", "minimum": 1, "description": "正在重新考虑的思考编号"},
                "branchFromThought": {"type": "integer", "minimum": 1, "description": "分支起点思考编号"},
                "branchId": {"type": "string", "description": "分支标识符"},
                "needsMoreThoughts": {"type": "boolean", "description": "是否需要更多思考"},
                "sessionId": {"type": "string", "description": "会话句柄：同 sessionId 的调用共享思维链状态（缺省 default）"}
            },
            "required": ["thought", "nextThoughtNeeded", "thoughtNumber", "totalThoughts"]
        }),
    },
    run
);
