//! config — 对应 v11 的 scripts/config.py

/// 预估总思考数的缺省值（客户端未传 totalThoughts 时；正常流程 schema 必填）
pub const DEFAULT_TOTAL_THOUGHTS: i64 = 5;

/// 历史长度上限，防止状态文件无限膨胀
pub const MAX_HISTORY_LENGTH: usize = 1000;

/// 分支 id 缺省前缀（v11 遗留常量，当前实现未使用）
#[allow(dead_code)]
pub const BRANCH_PREFIX: &str = "branch_";

/// thought 字段超长截断阈值（超长时截断并附加标记，与 v11 一致）
pub const THOUGHT_MAX_LEN: usize = 4000;

/// 思考步骤框图的 stderr 输出开关（环境变量 DISABLE_THOUGHT_LOGGING=true 关闭）
pub fn disable_thought_logging() -> bool {
    std::env::var("DISABLE_THOUGHT_LOGGING")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 状态文件目录（相对服务器工作目录）
pub const STATE_DIR: &str = "mcp_data/sequential_thinking";
