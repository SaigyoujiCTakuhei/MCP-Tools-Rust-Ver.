/// 上下文看板查询 — 只读访问 ZCode 引擎库与会话流水
///
/// 三层数据源：
/// 1. db.sqlite `model_usage`（逐 API 请求粒度）：当前上下文、增长曲线、计费估算
/// 2. db.sqlite `turn_usage` / `session`：轮次明细、压缩原生时间戳
/// 3. rollout 流水文件：压缩事件结构化解析（仅匹配顶层 type/event 字段，
///    绝不做子串搜索——2026-09-28 实测正文噪音在本会话就有 191 处假阳性）
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use anyhow::{anyhow, Context};
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Timelike};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

use crate::config::CtxBoardConfig;

// ==================== 全局配置（main 启动时注入，不可变） ====================

static CFG: OnceLock<CtxBoardConfig> = OnceLock::new();

pub fn init(cfg: CtxBoardConfig) {
    let _ = CFG.set(cfg);
}

fn cfg() -> &'static CtxBoardConfig {
    static FALLBACK: OnceLock<CtxBoardConfig> = OnceLock::new();
    CFG.get().unwrap_or_else(|| FALLBACK.get_or_init(CtxBoardConfig::default))
}

/// 展开 `~/` 前缀
fn expand_home(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return Path::new(&home).join(rest);
        }
    }
    PathBuf::from(p)
}

// ==================== 数据结构 ====================

#[derive(Debug, Serialize)]
pub struct BoardSession {
    pub session_id: String,
    pub title: String,
    pub time_updated_ms: i64,
    /// 客户端列表已移除（壳侧 archived 或 deleted）——看板单独成栏
    pub archived: bool,
    /// 壳侧 tasks.deleted=1（客户端里明确删除，本地副本仍在）
    pub deleted: bool,
    /// 子代理会话（session.parent_id 非空）
    pub subagent: bool,
    /// 当前上下文 = 最新请求 input_tokens（含缓存命中，与客户端「上下文容量」同口径）；无请求记录时为 null
    pub context_tokens: Option<i64>,
    pub model: Option<String>,
    pub provider_id: Option<String>,
    /// 自动压缩触发线 = 窗口 − min(最大输出, 21k) − 13k
    pub trigger_tokens: Option<i64>,
    pub context_window: i64,
    /// 原生压缩时间戳（session.time_compacting；db 只存最后一次）
    pub time_compacting_ms: Option<i64>,
    /// 估算消耗（积分，含时段乘数；非权威账单）
    pub points_estimate: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    /// 平均缓存命中率（token 加权，0-1）
    pub cache_hit_rate: Option<f64>,
    pub request_count: i64,
}

#[derive(Debug, Serialize)]
pub struct TurnRow {
    pub turn_id: String,
    pub started_at_ms: i64,
    pub duration_ms: Option<i64>,
    pub ttft_ms: Option<i64>,
    pub model_request_count: i64,
    pub tool_call_count: i64,
    pub tool_error_count: i64,
    /// 该轮输入合计（计费口径 = 回合内各请求之和）
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub points_estimate: f64,
    pub status: String,
    pub context_exceeded: bool,
    pub cancelled_by_user: bool,
    pub error_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CurvePoint {
    pub t_ms: i64,
    /// 该请求的完整上下文（含缓存命中）
    pub input_tokens: i64,
    /// zcode-agent = 主会话，其余为子代理
    pub agent: String,
    pub turn_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactionEvent {
    pub t_ms: i64,
    pub kind: String,
}

#[derive(Debug, Serialize)]
pub struct SessionDetail {
    pub session_id: String,
    /// 最新请求的模型（画参考线用的窗口/触发线由此模型参数推出）
    pub model: Option<String>,
    pub context_window: i64,
    pub trigger_tokens: Option<i64>,
    pub turns: Vec<TurnRow>,
    pub curve: Vec<CurvePoint>,
    /// 原生压缩时间戳（session.time_compacting）
    pub time_compacting_ms: Option<i64>,
    /// 流水结构化解析到的压缩事件（best-effort，见 scan_compactions 注释）
    pub compactions: Vec<CompactionEvent>,
    pub rollout_file: Option<String>,
    pub scan_note: Option<String>,
}

// ==================== 计费估算 ====================

/// 夜间畅用活动期（GLM 官方：2026-09-03 ~ 10-07 每日 23:00–09:00 Flash 零消耗）；
/// 日期过界自动失效，无需手动摘除
const NIGHT_FREE_START: NaiveDate = NaiveDate::from_ymd_opt(2026, 9, 3).unwrap();
const NIGHT_FREE_END: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
/// 庆双节全天半价（2026-09-25 ~ 10-07）
const PROMO_START: NaiveDate = NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
const PROMO_END: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();

fn is_flash(model: &str) -> bool {
    model.contains("Flash")
}

/// 时段乘数：夜间畅用(Flash×0) > 双节全天半价(×0.5) > 常规（工作日 14–18 点 ×1，其余 ×0.5）
fn time_multiplier(model: &str, t: DateTime<Local>) -> f64 {
    let d = t.date_naive();
    let minutes = t.hour() * 60 + t.minute();
    if is_flash(model) && d >= NIGHT_FREE_START && d <= NIGHT_FREE_END {
        // 23:00–09:00 跨 midnight
        if minutes >= 23 * 60 || minutes < 9 * 60 {
            return 0.0;
        }
    }
    if d >= PROMO_START && d <= PROMO_END {
        return 0.5;
    }
    let weekday = t.weekday().number_from_monday(); // 1=周一 … 7=周日
    if weekday <= 5 && (14 * 60..18 * 60).contains(&minutes) {
        1.0
    } else {
        0.5
    }
}

fn model_ctx(model: &str) -> crate::config::ModelCtx {
    let c = cfg();
    c.models.get(model).copied().unwrap_or(c.fallback_model)
}

/// 单请求估算积分
fn row_points(
    model: &str,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_creation: i64,
    t_ms: i64,
) -> f64 {
    let c = cfg();
    let coef = c.pricing.get(model).copied().unwrap_or(c.fallback_pricing);
    let mult = match Local.timestamp_millis_opt(t_ms) {
        chrono::LocalResult::Single(dt) => time_multiplier(model, dt),
        _ => 1.0,
    };
    let per_m = |tok: i64, rate: f64| tok as f64 / 1_000_000.0 * rate;
    // 缓存写（cache_creation）按输入价计，无官方口径，偏保守
    mult * (per_m(input, coef[0])
        + per_m(cache_read, coef[1])
        + per_m(cache_creation, coef[0])
        + per_m(output, coef[2]))
}

// ==================== DB 打开与行结构 ====================

fn open_db() -> anyhow::Result<Connection> {
    let path = expand_home(&cfg().db_path);
    Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("只读打开引擎库失败: {}", path.display()))
}

/// model_usage 行（聚合 / 曲线 / 计费共用的最小集）
struct UsageRow {
    session_id: String,
    turn_id: String,
    model_id: String,
    provider_id: String,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_creation: i64,
    started_at_ms: i64,
    agent: String,
}

fn load_usage_rows(conn: &Connection) -> anyhow::Result<Vec<UsageRow>> {
    let mut stmt = conn.prepare(
        "SELECT session_id, COALESCE(turn_id,''), COALESCE(model_id,''), COALESCE(provider_id,''), \
                COALESCE(input_tokens,0), COALESCE(output_tokens,0), \
                COALESCE(cache_read_input_tokens,0), COALESCE(cache_creation_input_tokens,0), \
                COALESCE(started_at,0), COALESCE(agent,'') \
         FROM model_usage",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(UsageRow {
                session_id: r.get(0)?,
                turn_id: r.get(1)?,
                model_id: r.get(2)?,
                provider_id: r.get(3)?,
                input: r.get(4)?,
                output: r.get(5)?,
                cache_read: r.get(6)?,
                cache_creation: r.get(7)?,
                started_at_ms: r.get(8)?,
                agent: r.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ==================== 一级：会话总览 ====================

/// 桌面壳会话软标记（tasks-index.sqlite 的 tasks 表）：archived / deleted。
/// 客户端的「存档/删除」只写壳侧标记，引擎库行与流水原样保留（=本地副本）；
/// 引擎库 session.time_archived 在本部署恒为 NULL，真标记在壳侧。
/// 文件缺失/表结构变化时返回空集（全部按活跃处理，功能降级不报错）。
fn load_shell_marks() -> HashMap<String, (bool, bool)> {
    let path = expand_home(&cfg().tasks_index_path);
    let mut map = HashMap::new();
    let Ok(conn) = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return map;
    };
    let Ok(mut stmt) = conn.prepare("SELECT task_id, deleted, archived FROM tasks") else {
        return map;
    };
    if let Ok(rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    }) {
        for (task_id, deleted, archived) in rows.flatten() {
            map.insert(task_id, (deleted != 0, archived != 0));
        }
    }
    map
}

pub fn list_sessions() -> anyhow::Result<Vec<BoardSession>> {
    let conn = open_db()?;
    let usage = load_usage_rows(&conn)?;
    let shell_marks = load_shell_marks();

    // 每会话一遍聚合；第 6 项 = 最新一条请求（上下文口径）
    let mut agg: HashMap<String, (i64, i64, i64, i64, f64, Option<UsageRow>)> = HashMap::new();
    for r in &usage {
        let e = agg
            .entry(r.session_id.clone())
            .or_insert_with(|| (0, 0, 0, 0, 0.0, None));
        e.0 += 1;
        e.1 += r.input;
        e.2 += r.output;
        e.3 += r.cache_read;
        e.4 += row_points(&r.model_id, r.input, r.output, r.cache_read, r.cache_creation, r.started_at_ms);
        match &e.5 {
            Some(prev) if prev.started_at_ms >= r.started_at_ms => {}
            _ => e.5 = Some(UsageRow {
                session_id: r.session_id.clone(),
                turn_id: r.turn_id.clone(),
                model_id: r.model_id.clone(),
                provider_id: r.provider_id.clone(),
                input: r.input,
                output: r.output,
                cache_read: r.cache_read,
                cache_creation: r.cache_creation,
                started_at_ms: r.started_at_ms,
                agent: r.agent.clone(),
            }),
        }
    }

    let mut stmt = conn.prepare(
        "SELECT id, COALESCE(title,''), COALESCE(time_updated,0), \
                COALESCE(time_compacting,0), time_archived IS NOT NULL, parent_id IS NOT NULL \
         FROM session ORDER BY time_updated DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, bool>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut out = Vec::with_capacity(rows.len());
    for (sid, title, updated, compacting, _engine_archived, subagent) in rows {
        let (req_count, total_input, total_output, total_cache_read, points, latest) =
            agg.remove(&sid).unwrap_or_default();
        let model = latest.as_ref().map(|l| l.model_id.clone());
        let mctx = model_ctx(model.as_deref().unwrap_or(""));
        let trigger = latest
            .as_ref()
            .map(|_| mctx.context_window - mctx.max_output.min(21_000) - 13_000);
        // 存档/删除以壳侧标记为准（引擎列恒空）；engine archived 仅作兜底或
        let (shell_deleted, shell_archived) = shell_marks.get(&sid).copied().unwrap_or((false, false));
        out.push(BoardSession {
            context_tokens: latest.as_ref().map(|l| l.input),
            trigger_tokens: trigger,
            context_window: mctx.context_window,
            session_id: sid,
            title,
            time_updated_ms: updated,
            archived: shell_archived,
            deleted: shell_deleted,
            subagent,
            model,
            provider_id: latest.as_ref().map(|l| l.provider_id.clone()),
            time_compacting_ms: (compacting > 0).then_some(compacting),
            points_estimate: points,
            total_input_tokens: total_input,
            total_output_tokens: total_output,
            cache_hit_rate: (total_input > 0).then(|| total_cache_read as f64 / total_input as f64),
            request_count: req_count,
        });
    }
    Ok(out)
}

// ==================== 二级：轮次明细 + 曲线 + 压缩事件 ====================

/// session_id 合法性（防路径拼接意外）：sess_ + UUID 字符集
fn valid_session_id(id: &str) -> bool {
    id.starts_with("sess_")
        && id.len() <= 64
        && id[5..].bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

pub fn session_detail(session_id: &str) -> anyhow::Result<SessionDetail> {
    if !valid_session_id(session_id) {
        return Err(anyhow!("非法 session_id"));
    }
    let conn = open_db()?;

    // 曲线 + 每轮估算积分（按回合内各请求时刻取时段乘数）
    let mut turn_points: HashMap<String, f64> = HashMap::new();
    let mut curve: Vec<CurvePoint> = Vec::new();
    for r in load_usage_rows(&conn)?
        .into_iter()
        .filter(|r| r.session_id == session_id)
    {
        *turn_points.entry(r.turn_id.clone()).or_default() += row_points(
            &r.model_id,
            r.input,
            r.output,
            r.cache_read,
            r.cache_creation,
            r.started_at_ms,
        );
        curve.push(CurvePoint {
            t_ms: r.started_at_ms,
            input_tokens: r.input,
            agent: r.agent,
            turn_id: r.turn_id.clone(),
        });
    }
    curve.sort_by_key(|p| p.t_ms);

    let mut stmt = conn.prepare(
        "SELECT turn_id, COALESCE(started_at,0), duration_ms, time_to_first_token_ms, \
                COALESCE(model_request_count,0), COALESCE(tool_call_count,0), COALESCE(tool_error_count,0), \
                COALESCE(input_tokens,0), COALESCE(output_tokens,0), COALESCE(cache_read_input_tokens,0), \
                COALESCE(status,''), COALESCE(context_exceeded,0), COALESCE(cancelled_by_user,0), error_type \
         FROM turn_usage WHERE session_id=?1 ORDER BY started_at",
    )?;
    let turns = stmt
        .query_map([session_id], |r| {
            Ok(TurnRow {
                turn_id: r.get(0)?,
                started_at_ms: r.get(1)?,
                duration_ms: r.get(2)?,
                ttft_ms: r.get(3)?,
                model_request_count: r.get(4)?,
                tool_call_count: r.get(5)?,
                tool_error_count: r.get(6)?,
                input_tokens: r.get(7)?,
                output_tokens: r.get(8)?,
                cache_read_tokens: r.get(9)?,
                status: r.get(10)?,
                context_exceeded: r.get(11)?,
                cancelled_by_user: r.get(12)?,
                error_type: r.get(13)?,
                points_estimate: 0.0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let turns = turns
        .into_iter()
        .map(|mut t| {
            t.points_estimate = turn_points.get(&t.turn_id).copied().unwrap_or(0.0);
            t
        })
        .collect();

    // 原生压缩时间戳
    let time_compacting_ms = conn
        .prepare("SELECT time_compacting FROM session WHERE id=?1")?
        .query_row([session_id], |r| r.get::<_, Option<i64>>(0))
        .ok()
        .flatten();

    // 最新模型（参考线参数来源）
    let model = conn
        .prepare(
            "SELECT model_id FROM model_usage WHERE session_id=?1 AND model_id IS NOT NULL \
             ORDER BY started_at DESC, id DESC LIMIT 1",
        )?
        .query_row([session_id], |r| r.get::<_, String>(0))
        .ok();
    let mctx = model_ctx(model.as_deref().unwrap_or(""));

    // 流水压缩事件（best-effort）
    let rollout_file = expand_home(&cfg().rollout_dir).join(format!("model-io-{session_id}.jsonl"));
    let (compactions, scan_note) = if rollout_file.exists() {
        match scan_compactions(&rollout_file) {
            Ok(v) => (v, None),
            Err(e) => (Vec::new(), Some(format!("流水解析失败: {e:#}"))),
        }
    } else {
        (
            Vec::new(),
            Some("流水文件已清理，压缩事件无法追溯（原生压缩时间戳仍有效）".into()),
        )
    };

    let trigger = model
        .as_ref()
        .map(|_| mctx.context_window - mctx.max_output.min(21_000) - 13_000);
    Ok(SessionDetail {
        session_id: session_id.to_string(),
        model,
        context_window: mctx.context_window,
        trigger_tokens: trigger,
        turns,
        curve,
        time_compacting_ms,
        compactions,
        rollout_file: rollout_file
            .exists()
            .then(|| rollout_file.display().to_string()),
        scan_note,
    })
}

// ==================== 流水压缩事件解析 ====================

struct ScanCacheEntry {
    len: u64,
    mtime: SystemTime,
    events: Vec<CompactionEvent>,
}

fn scan_cache() -> &'static Mutex<HashMap<PathBuf, ScanCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, ScanCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 结构化解析压缩事件（按 (len, mtime) 缓存，大文件不重复扫）。
///
/// 判定 = 顶层对象的 type/event 字段值含 compact（不区分大小写）。
/// 刻意不做子串搜索：流水每行含完整请求体，正文提到 compact 一词全是噪音
/// （2026-09-28 本会话实测 191 处假阳性）。事件行很小，且请求体大行先按
/// 首字段预筛（事件记录以 {"type"/{"event" 开头，请求记录以 {"completedAt"
/// 等开头），避免对 GB 级文件做全量 JSON 解析。
/// 流式逐行读取：最大流水 190MB+（Pi4），整读进内存不可接受。
/// ⚠️ 当前全库零压缩（74 会话，2026-09-28），真实事件行形态未经样本验证；
///    若客户端升级后形态变化，以 session.time_compacting 原生列为准。
fn scan_compactions(path: &Path) -> anyhow::Result<Vec<CompactionEvent>> {
    let meta = fs::metadata(path)?;
    let len = meta.len();
    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    if let Some(hit) = scan_cache().lock().unwrap().get(path) {
        if hit.len == len && hit.mtime == mtime {
            return Ok(hit.events.clone());
        }
    }

    let file = fs::File::open(path).with_context(|| format!("读取流水失败: {}", path.display()))?;
    let mut reader = io::BufReader::with_capacity(1 << 20, file);
    let mut events = Vec::new();
    let mut buf: Vec<u8> = Vec::with_capacity(1 << 20);
    loop {
        buf.clear();
        // 手动 read_until：事件行只可能是每行开头 {"type"/{"event"，其余行仅做字节预筛
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        let t = trim_start(&buf);
        if !(t.starts_with(b"{\"type\"") || t.starts_with(b"{\"event\"")) {
            continue;
        }
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(t) else {
            continue;
        };
        for key in ["type", "event"] {
            if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
                if s.to_ascii_lowercase().contains("compact") {
                    let ts = v
                        .get("timestamp")
                        .and_then(|x| x.as_i64())
                        .or_else(|| v.get("time").and_then(|x| x.as_i64()))
                        .unwrap_or(0);
                    events.push(CompactionEvent { t_ms: ts, kind: s.to_string() });
                }
            }
        }
    }
    let result = events.clone();
    scan_cache().lock().unwrap().insert(
        path.to_path_buf(),
        ScanCacheEntry { len, mtime, events },
    );
    Ok(result)
}

fn trim_start(mut b: &[u8]) -> &[u8] {
    while let Some(&f) = b.first() {
        if f == b' ' || f == b'\t' || f == b'\r' {
            b = &b[1..];
        } else {
            break;
        }
    }
    b
}
