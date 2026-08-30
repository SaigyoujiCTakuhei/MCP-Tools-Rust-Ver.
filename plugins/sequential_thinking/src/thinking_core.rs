//! thinking_core — 核心状态机逻辑（纯库模块，只有被调用的定义/函数，不暴露为工具）
//!
//! 对应 v11 的 scripts/thinking_core.py（迁移自原 TypeScript SequentialThinkingServer）：
//! 线性推进、分支探索与自我修正；字段名保持 v11 的 camelCase 以兼容历史状态文件。
//!
//! 与 v11 的差异：本模块不做任何 I/O——状态由调用方（工具薄壳）加载/持久化，
//! 日志由调用方格式化输出。纯函数化便于单元测试与状态外置。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::config::{DEFAULT_TOTAL_THOUGHTS, MAX_HISTORY_LENGTH};

// ==================== 输出 Schema ====================

/// 思考步骤快照输出结构（对应 v11 ThoughtSnapshot TypedDict）
#[derive(Debug, Clone, Serialize)]
pub struct ThoughtSnapshot {
    #[serde(rename = "thoughtNumber")]
    pub thought_number: i64,
    #[serde(rename = "totalThoughts")]
    pub total_thoughts: i64,
    #[serde(rename = "nextThoughtNeeded")]
    pub next_thought_needed: bool,
    pub branches: Vec<String>,
    #[serde(rename = "thoughtHistoryLength")]
    pub thought_history_length: usize,
}

// ==================== 数据模型 ====================

/// 思考步骤数据模型（对应 v11 ThoughtData dataclass）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThoughtData {
    pub thought: String,
    pub thought_number: i64,
    pub total_thoughts: i64,
    #[serde(default)]
    pub is_revision: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revises_thought: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_from_thought: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    #[serde(default)]
    pub needs_more_thoughts: bool,
    #[serde(default = "default_true")]
    pub next_thought_needed: bool,
}

fn default_true() -> bool {
    true
}

// ==================== 状态管理器 ====================

/// 单会话的思考状态管理器（对应 v11 ThoughtState；持久化由调用方负责）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThoughtState {
    #[serde(default)]
    pub thought_history: Vec<ThoughtData>,
    #[serde(default)]
    pub branches: HashMap<String, Vec<ThoughtData>>,
}

impl ThoughtState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一步思考：动态调整总数（编号超过预估时上修）+ 分支归档 + 历史截断
    pub fn add_thought(&mut self, mut data: ThoughtData) {
        if data.thought_number > data.total_thoughts {
            data.total_thoughts = data.thought_number;
        }
        if let (Some(_), Some(id)) = (data.branch_from_thought, data.branch_id.clone()) {
            self.branches.entry(id).or_default().push(data.clone());
        }
        self.thought_history.push(data);
        if self.thought_history.len() > MAX_HISTORY_LENGTH {
            let overflow = self.thought_history.len() - MAX_HISTORY_LENGTH;
            self.thought_history.drain(0..overflow);
        }
    }

    /// 获取状态快照
    pub fn get_snapshot(&self, next_needed: bool) -> ThoughtSnapshot {
        let last = self.thought_history.last();
        ThoughtSnapshot {
            thought_number: last.map(|d| d.thought_number).unwrap_or(0),
            total_thoughts: last.map(|d| d.total_thoughts).unwrap_or(0),
            next_thought_needed: next_needed,
            branches: self.branches.keys().cloned().collect(),
            thought_history_length: self.thought_history.len(),
        }
    }
}

// ==================== 核心处理函数 ====================

/// 处理单次思考步骤：解析参数（含 v11 的缺省与宽松类型转换）→ 记录 → 返回快照。
/// 校验失败返回 Err（调用方转成 ErrorSnapshot / isError:true）。
pub fn process_thought(
    state: &mut ThoughtState,
    input_data: &Value,
) -> Result<ThoughtSnapshot, String> {
    let thought = coerce_str(input_data.get("thought")).unwrap_or_default();
    let thought_number = coerce_i64(input_data.get("thoughtNumber")).unwrap_or(1);
    let total_thoughts = coerce_i64(input_data.get("totalThoughts"))
        .unwrap_or(DEFAULT_TOTAL_THOUGHTS);
    let next_needed = coerce_bool(input_data.get("nextThoughtNeeded"), true);
    let is_revision = coerce_bool(input_data.get("isRevision"), false);
    let revises_thought = coerce_i64_opt(input_data.get("revisesThought"));
    let branch_from = coerce_i64_opt(input_data.get("branchFromThought"));
    let branch_id = input_data
        .get("branchId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let needs_more = coerce_bool(input_data.get("needsMoreThoughts"), false);

    let data = ThoughtData {
        thought,
        thought_number,
        total_thoughts,
        is_revision,
        revises_thought,
        branch_from_thought: branch_from,
        branch_id,
        needs_more_thoughts: needs_more,
        next_thought_needed: next_needed,
    };

    state.add_thought(data);
    Ok(state.get_snapshot(next_needed))
}

// ==================== 展示格式化（供薄壳输出到 stderr） ====================

/// 格式化思考步骤框图（对应 v11 的 _format_and_log）
pub fn format_thought_box(data: &ThoughtData) -> String {
    let prefix = if data.is_revision {
        "[🔄 Revision]"
    } else if data.branch_from_thought.is_some() {
        "[🌿 Branch]"
    } else {
        "[💭 Thought]"
    };
    let context = if data.is_revision && data.revises_thought.is_some() {
        format!(" (revising thought {:?})", data.revises_thought)
    } else if let Some(from) = data.branch_from_thought {
        format!(" (from thought {from}, ID: {:?})", data.branch_id)
    } else {
        String::new()
    };

    let header = format!("{} {}/{}{}", prefix, data.thought_number, data.total_thoughts, context);
    let width = header.chars().count().max(data.thought.chars().count()) + 4;
    let border = "─".repeat(width);
    let padded: String = {
        let pad = width.saturating_sub(2).saturating_sub(data.thought.chars().count());
        format!("{}{}", data.thought, " ".repeat(pad))
    };
    format!(
        "\n┌{border}┐\n│ {header} │\n├{border}┤\n│ {padded} │\n└{border}┘"
    )
}

// ==================== 宽松类型转换（对应 v11 的 Pydantic Union[int,str] 等） ====================

fn coerce_str(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
        None => None,
    }
}

fn coerce_i64(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

fn coerce_i64_opt(v: Option<&Value>) -> Option<i64> {
    match v {
        None | Some(Value::Null) => None,
        other => coerce_i64(other),
    }
}

fn coerce_bool(v: Option<&Value>, default: bool) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) if s.eq_ignore_ascii_case("true") => true,
        Some(Value::String(s)) if s.eq_ignore_ascii_case("false") => false,
        _ => default,
    }
}

// ==================== 单元测试（对应 v11 的 test_thinking_core.py） ====================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(n: i64, total: i64) -> Value {
        json!({ "thought": format!("step {n}"), "thoughtNumber": n, "totalThoughts": total })
    }

    #[test]
    fn linear_progression() {
        let mut st = ThoughtState::new();
        st.add_thought(serde_json::from_value(json!({
            "thought": "a", "thoughtNumber": 1, "totalThoughts": 3
        })).unwrap());
        st.add_thought(serde_json::from_value(json!({
            "thought": "b", "thoughtNumber": 2, "totalThoughts": 3
        })).unwrap());
        let snap = st.get_snapshot(false);
        assert_eq!(snap.thought_number, 2);
        assert_eq!(snap.thought_history_length, 2);
        assert!(snap.branches.is_empty());
    }

    #[test]
    fn dynamic_total_adjustment() {
        // 编号超过预估 → 总数上修（v11 行为）
        let mut st = ThoughtState::new();
        st.add_thought(serde_json::from_value(json!({
            "thought": "big", "thoughtNumber": 5, "totalThoughts": 3
        })).unwrap());
        assert_eq!(st.get_snapshot(false).total_thoughts, 5);
    }

    #[test]
    fn branches_recorded() {
        let mut st = ThoughtState::new();
        st.add_thought(serde_json::from_value(json!({
            "thought": "alt", "thoughtNumber": 2, "totalThoughts": 3,
            "branchFromThought": 1, "branchId": "branch_a"
        })).unwrap());
        let snap = st.get_snapshot(false);
        assert_eq!(snap.branches, vec!["branch_a".to_string()]);
        assert_eq!(st.branches["branch_a"].len(), 1);
    }

    #[test]
    fn process_parses_loose_types() {
        // 字符串数字 / 字符串布尔 → 宽松转换（对应 v11 Pydantic Union）
        let mut st = ThoughtState::new();
        let snap = process_thought(&mut st, &json!({
            "thought": "  hello  ", "thoughtNumber": "1", "totalThoughts": "3",
            "nextThoughtNeeded": "false"
        })).unwrap();
        assert_eq!(snap.thought_number, 1);
        assert!(!snap.next_thought_needed);
    }
}
