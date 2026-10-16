/// 编号会话台账 — 解析「会话整理」文件夹，盘点迄今为止所有带编号的会话
///
/// 数据源（只读，每次请求现读现算——台账随增补轮持续生长，新鲜度优先；整夹 <300KB）：
/// 1. `00-附录-会话清单全表.md` 表体：主会话行标题自带编号（「05-安卓运维。- 05」）
///    或注记编号（「**=整理主线 - 02**」）——编号命名与主线（cur）组以这里为准
/// 2. 各组编号 MD（`NN-组名.md`）的「涉及会话」表：首列 `sess_…`（- NN）标记——
///    补 00 表标题未带编号的追认行（如 11-01，组 MD 首列有标记而标题是原始指令）
/// 3. 00 头注挂账段：`` `短id` 名称。- NN（…，组NN）`` ——已编号还没进正表的会话，打未登记标
///    （另：00 正表行内带「自登记」字样的行 = 会话自写的行，打自登记标；无字样的正表行
///    = 主线已代登记/已过手，打已登记标，cur 主线行恒归此档。p 拍板口径：收编只走整理
///    主线一条线；10-15 增拍第三档——只要会话还没存档就该有标在，三档都是「还开着」的
///    提示；同日定名三字：未登记/自登记/已登记；10-16 换字规矩入主线提示词、
///    cur 恒自登记随三档语义作废
/// 4. 壳侧真名（最高优先）：引擎库 session.title = p 在壳侧编辑的名字——编号样式且
///    序号一致即采纳；未编辑的用完即弃会话是原始指令式标题，不覆盖台账命名
/// 5. 壳侧已编号未入册：db 里标题是编号样式、但 MD 台账还没登记的会话（编号现场
///    可见，打 unregistered 标；MD 正式登记后自然转正）
///
/// 三档标（自登记/已登记/未登记）只属于还开着的会话：壳侧已存档/删除 = 完结，一律熄标。
///
/// 条目命名优先级：壳侧真名（编号样式）> 台账记录名 > 组前缀构造（仅追认行）。
/// 组内早期行的旧式命名（如「A股投资与量化。- 03」无「11-」前缀）原样保留，
/// 不做时代错置的归一。解析全部手写字符串扫描，不引 regex 依赖。

use std::collections::BTreeMap;
use std::fs;

use serde::Serialize;

use super::query::{cfg, expand_home, open_db, shell_finished_ids};

#[derive(Debug, Serialize)]
pub struct NumberedLedger {
    pub groups: Vec<LedgerGroup>,
    pub total: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LedgerGroup {
    /// "01".."13" / "cur"（排序键：数字组升序，cur 殿后）
    pub key: String,
    pub label: String,
    pub count: usize,
    pub items: Vec<LedgerItem>,
}

#[derive(Debug, Serialize)]
pub struct LedgerItem {
    pub ordinal: u32,
    /// 条目名：壳侧真名（编号样式，最高优先）> 台账记录名 > 组前缀构造
    pub name: String,
    /// 完整会话 id（可下钻看板详情；头注挂账行只有 8 位短 id 时为 null）
    pub session_id: Option<String>,
    /// 短 id 线索（挂账行），前端按「包含匹配」兜底下钻
    pub id_hint: Option<String>,
    /// 自登记 = 行已由会话自写入 00 正表（行内带「自登记」字样），待主线过手；
    /// 主线过手清字后换亮已登记标
    pub self_registered: bool,
    /// 已登记 = 行在 00 正表且无「自登记」字样（主线代登记或已过手），10-15 新增中间态：
    /// 会话还开着就该有标，存档即熄
    pub collected: bool,
    /// 未登记 = 台账文件里还没有行（00 头注挂账段 / 仅壳侧改名两来源）
    pub unregistered: bool,
}

/// 解析中间态：同一条编号会话可能被多个来源命中，字段互补不互覆
#[derive(Debug, Default, Clone)]
struct RawItem {
    name: Option<String>,
    session_id: Option<String>,
    id_hint: Option<String>,
    self_registered: bool,
    collected: bool,
    unregistered: bool,
}

/// 壳侧会话快照：id → (title, task_type)，来源 4/5 共用一次查询
type ShellSessions = std::collections::HashMap<String, (String, String)>;

struct GroupAcc {
    label: String,
    items: BTreeMap<u32, RawItem>,
}

pub fn list_numbered() -> NumberedLedger {
    let mut warnings = Vec::new();
    let dir = expand_home(&cfg().ledger_dir);
    let mut groups: BTreeMap<String, GroupAcc> = BTreeMap::new();

    if !dir.is_dir() {
        return NumberedLedger {
            groups: Vec::new(),
            total: 0,
            warnings: vec![format!("台账目录不存在: {}", dir.display())],
        };
    }

    // 来源 2：各组编号 MD（`NN-*.md`，跳过 00 与 README）
    let mut md_stems: BTreeMap<String, String> = BTreeMap::new();
    let mut md_files: Vec<_> = fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    md_files.sort_by_key(|e| e.file_name());
    for entry in md_files {
        let name = entry.file_name().to_string_lossy().to_string();
        let stem = name.trim_end_matches(".md").to_string();
        let Some(key) = group_key_of(&stem) else { continue };
        md_stems.insert(key.clone(), stem.clone());
        match fs::read_to_string(entry.path()) {
            Ok(txt) => parse_group_md(&txt, key, stem, &mut groups),
            Err(e) => warnings.push(format!("读 {} 失败: {e}", name)),
        }
    }

    // 来源 1 + 3：00 全表（表体权威命名 + 头注挂账）
    let ledger00 = dir.join("00-附录-会话清单全表.md");
    match fs::read_to_string(&ledger00) {
        Ok(txt) => parse_ledger00(&txt, &md_stems, &mut groups),
        Err(e) => warnings.push(format!(
            "读 00-附录-会话清单全表.md 失败（组 MD 标记仍可用）: {e}"
        )),
    }

    // 来源 4 + 5：壳侧真名（最高优先，覆盖已有条目）+ 壳侧已编号未入册（新增条目）
    if let Some(shell) = load_shell_sessions(&mut warnings) {
        apply_shell_titles(&mut groups, &shell);
        add_unregistered_from_db(&mut groups, &shell, &md_stems);
    }

    // 存档熄标：p 在壳侧存档/删除 = 会话已完结，历史自登记行不再亮「自登记」、
    // 未登记的也不再亮「未登记」——标只留给还开着的会话
    let finished = shell_finished_ids();
    if !finished.is_empty() {
        for acc in groups.values_mut() {
            for raw in acc.items.values_mut() {
                let done = raw
                    .session_id
                    .as_deref()
                    .map(|id| finished.contains(id))
                    .unwrap_or(false)
                    || raw
                        .id_hint
                        .as_deref()
                        .map(|h| finished.iter().any(|id| id.len() >= 13 && &id[5..13] == h))
                        .unwrap_or(false);
                if done {
                    raw.self_registered = false;
                    raw.collected = false;
                    raw.unregistered = false;
                }
            }
        }
    }

    // 组装：数字组升序、cur 殿后；渲染名按组内最大编号前缀归一
    let mut keys: Vec<String> = groups.keys().cloned().collect();
    keys.sort_by(|a, b| {
        let ka = a.parse::<u32>().unwrap_or(u32::MAX);
        let kb = b.parse::<u32>().unwrap_or(u32::MAX);
        ka.cmp(&kb).then_with(|| a.cmp(b))
    });
    let mut out_groups = Vec::new();
    let mut total = 0usize;
    for key in keys {
        let acc = groups.get_mut(&key).unwrap();
        let prefix = acc
            .items
            .values()
            .rev()
            .find_map(|it| it.name.as_deref().and_then(name_prefix))
            .unwrap_or_else(|| format!("{}。", acc.label));
        let items = acc
            .items
            .iter()
            .map(|(ord, raw)| LedgerItem {
                ordinal: *ord,
                name: display_name(raw.name.as_deref(), &prefix, *ord),
                session_id: raw.session_id.clone(),
                id_hint: raw.id_hint.clone(),
                self_registered: raw.self_registered,
                collected: raw.collected,
                unregistered: raw.unregistered,
            })
            .collect::<Vec<_>>();
        total += items.len();
        out_groups.push(LedgerGroup {
            key,
            label: acc.label.clone(),
            count: items.len(),
            items,
        });
    }

    NumberedLedger {
        groups: out_groups,
        total,
        warnings,
    }
}

/// 组键：MD 文件名前两位数字（"01".."13"）
fn group_key_of(stem: &str) -> Option<String> {
    let b = stem.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_digit() && b[1].is_ascii_digit() {
        let key = &stem[..2];
        if key != "00" {
            return Some(key.to_string());
        }
    }
    None
}

/// 「涉及会话」表行：首列 `sess_…`（- NN）标记取编号；标题列尽量取自带编号名
fn parse_group_md(txt: &str, key: String, stem: String, groups: &mut BTreeMap<String, GroupAcc>) {
    for line in txt.lines() {
        if !line.starts_with("| `") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').collect();
        let Some(first) = cells.get(1) else { continue };
        let Some(ordinal) = ordinal_from_marker(first) else { continue };
        let session_id = find_sess_id(first);
        // 标题列：| 会话 | 时间 | 标题 | 流水 |（标题含裸「|」时中段拼回，同 00 表口径）
        if cells.len() < 6 {
            continue;
        }
        let title = cells[3..cells.len() - 2].join("|");
        let name = numbered_name_before_paren(&title).or_else(|| annotated_name(&title));
        let acc = groups.entry(key.clone()).or_insert_with(|| GroupAcc {
            label: stem.clone(),
            items: BTreeMap::new(),
        });
        let raw = acc.items.entry(ordinal).or_default();
        if raw.name.is_none() {
            raw.name = name;
        }
        if raw.session_id.is_none() {
            raw.session_id = session_id;
        }
    }
}

/// 00 全表：表体行（主会话 + 编号名/注记）+ 头注挂账段
fn parse_ledger00(
    txt: &str,
    md_stems: &BTreeMap<String, String>,
    groups: &mut BTreeMap<String, GroupAcc>,
) {
    for line in txt.lines() {
        if line.starts_with("> ⚠️") {
            parse_pending_headnote(line, md_stems, groups);
            continue;
        }
        if !line.starts_with("| ") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').collect();
        // | # | id | 日期 | 类型 | 标题 | 分组 | 流水 |
        // 标题内可能含裸「|」（如「R/|MDD|」风险写法，#122 实例）——两端定长、中段拼回：
        // 分组 = 倒数第 3 段，标题 = 第 6 段到分组前按「|」原样重组
        if cells.len() < 8 {
            continue;
        }
        if !cells[4].contains("主会话") {
            continue;
        }
        let group_cell = cells[cells.len() - 3].trim();
        let title = cells[5..cells.len() - 3].join("|");
        let title = title.trim();
        let Some(name) = annotated_name(title).or_else(|| numbered_name_before_paren(title))
        else {
            continue;
        };
        let Some(ordinal) = extract_ordinal(&name) else { continue };
        let Some((key, fallback_label)) = group_of_00(group_cell) else {
            continue;
        };
        let label = md_stems
            .get(&key)
            .cloned()
            .unwrap_or_else(|| fallback_label.unwrap_or_else(|| key.clone()));
        let session_id = find_sess_id(cells[2]).map(|id| {
            // 00 表 id 列省略 sess_ 前缀
            if id.starts_with("sess_") {
                id
            } else {
                format!("sess_{id}")
            }
        });
        // 自登记：非 cur 行看「自登记」字样（主线过手清字后即换亮已登记标）。
        // cur 行（会话整理主线）恒归已登记档——主线行=「主线已登记」的定义本身，
        // 10-14 两档时代的「cur 恒自登记」随三档语义作废（10-16 p 期望主线行翻绿拍板）；
        // 存档即熄标不变。
        let self_reg = key != "cur" && title.contains("自登记") && !title.contains("已收编");
        let acc = groups.entry(key).or_insert_with(|| GroupAcc {
            label,
            items: BTreeMap::new(),
        });
        let raw = acc.items.entry(ordinal).or_default();
        // 00 表为权威命名（组 MD 先扫，其标题可能是截断的原始指令），无条件覆盖
        raw.name = Some(name);
        if self_reg {
            raw.self_registered = true;
        } else {
            raw.collected = true;
        }
        if raw.session_id.is_none() {
            raw.session_id = session_id;
        }
    }
}

/// 头注挂账段：`` `8位短id` 名称。- NN（…，组NN）`` 三要素齐备才收
fn parse_pending_headnote(
    line: &str,
    md_stems: &BTreeMap<String, String>,
    groups: &mut BTreeMap<String, GroupAcc>,
) {
    let mut i = 0;
    while let Some(pos) = line[i..].find('`') {
        let start = i + pos + 1;
        let Some(close) = line[start..].find('`') else { break };
        let id = &line[start..start + close];
        if id.len() == 8 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
            // 闭引号后到下一个全角左括号 = 候选名
            let after = &line[start + close + 1..];
            if let Some(paren) = after.find('（') {
                let cand = after[..paren].trim();
                if let Some(ordinal) = extract_ordinal(cand) {
                    // 组归属：括号开头 40 个字符内的 组NN（char 边界安全的窗口）
                    let ctx: String = after[paren..].chars().take(40).collect();
                    if let Some(gpos) = ctx.find("组") {
                        let gbytes = ctx.as_bytes();
                        let mut j = gpos + "组".len();
                        let mut gkey = String::new();
                        while j < gbytes.len() && gbytes[j].is_ascii_digit() && gkey.len() < 2 {
                            gkey.push(gbytes[j] as char);
                            j += 1;
                        }
                        if !gkey.is_empty() {
                            let label = md_stems
                                .get(&gkey)
                                .cloned()
                                .unwrap_or_else(|| format!("组{gkey}"));
                            let acc = groups.entry(gkey).or_insert_with(|| GroupAcc {
                                label,
                                items: BTreeMap::new(),
                            });
                            let raw = acc.items.entry(ordinal).or_default();
                            if raw.name.is_none() {
                                raw.name = Some(cand.to_string());
                            }
                            raw.id_hint = Some(id.to_string());
                            raw.unregistered = true;
                            i = start + close + 1 + paren;
                            continue;
                        }
                    }
                }
            }
        }
        i = start + close.max(1);
    }
}

/// 壳侧会话快照：一次只读查询，供真名覆盖与未入册新增两处使用
fn load_shell_sessions(warnings: &mut Vec<String>) -> Option<ShellSessions> {
    let conn = match open_db() {
        Ok(c) => c,
        Err(e) => {
            warnings.push(format!("引擎库打不开（全部按台账名展示）: {e:#}"));
            return None;
        }
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT id, COALESCE(title,''), COALESCE(task_type,'') FROM session",
    ) else {
        return None;
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    }) else {
        return None;
    };
    let mut m = ShellSessions::new();
    for (id, title, task_type) in rows.flatten() {
        m.insert(id, (title, task_type));
    }
    Some(m)
}

/// 来源 4（壳侧真名，最高优先）：p 在壳侧编辑过的会话名以壳侧为准。
/// 采纳门槛 = 真名本身是编号样式且序号与本条一致：原始指令式标题（用完即弃、未编辑的
/// 会话）不覆盖台账命名；台账无名且真名也不编号 → 留空走组前缀构造
fn apply_shell_titles(groups: &mut BTreeMap<String, GroupAcc>, shell: &ShellSessions) {
    let mut by_hint: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for id in shell.keys() {
        if id.len() >= 13 && id.starts_with("sess_") {
            by_hint.insert(id[5..13].to_string(), id.clone());
        }
    }
    for acc in groups.values_mut() {
        for (ord, raw) in acc.items.iter_mut() {
            let id = raw
                .session_id
                .clone()
                .or_else(|| raw.id_hint.as_deref().and_then(|h| by_hint.get(h).cloned()));
            let Some(id) = id else { continue };
            let Some((title, _)) = shell.get(&id) else { continue };
            if extract_ordinal(title) == Some(*ord) {
                raw.name = Some(title.clone());
                raw.session_id = raw.session_id.clone().or(Some(id));
            }
        }
    }
}

/// 来源 5：壳侧已编号、MD 台账还没登记的会话——标题「NN-组名。- 序号」样式即入账
/// （组号取标题前两位，且该槽位未被 MD/00 来源占据），打 unregistered 标；
/// MD 正式登记后同名条目被台账来源先占，这里自动跳过 = 转正
fn add_unregistered_from_db(
    groups: &mut BTreeMap<String, GroupAcc>,
    shell: &ShellSessions,
    md_stems: &BTreeMap<String, String>,
) {
    for (id, (title, task_type)) in shell {
        if matches!(task_type.as_str(), "subagent_child" | "workflow_child") {
            continue;
        }
        let Some(ordinal) = extract_ordinal(title) else { continue };
        let head = title.trim().as_bytes();
        // 组号 = 标题开头的「NN-」前缀；无前缀的编号名（如「Rust 学习。- 01」）归不了组，留给 MD 登记
        if head.len() < 3 || !head[0].is_ascii_digit() || !head[1].is_ascii_digit() || head[2] != b'-' {
            continue;
        }
        let key = title.trim()[..2].to_string();
        let acc = groups.entry(key.clone()).or_insert_with(|| GroupAcc {
            label: md_stems
                .get(&key)
                .cloned()
                .unwrap_or_else(|| format!("组{key}")),
            items: BTreeMap::new(),
        });
        if !acc.items.contains_key(&ordinal) {
            acc.items.insert(
                ordinal,
                RawItem {
                    name: Some(title.trim().to_string()),
                    session_id: Some(id.clone()),
                    id_hint: None,
                    self_registered: false,
                    collected: false,
                    unregistered: true,
                },
            );
        }
    }
}

/// 00 分组列 →（组键，兜底标签）："05·安卓运维" / "cur·当前" / "04·设备评估与娱乐（10-08 自 05 并入）"
fn group_of_00(cell: &str) -> Option<(String, Option<String>)> {
    let head = cell.split('·').next()?.trim();
    if head == "cur" {
        return Some(("cur".to_string(), Some("会话整理主线".to_string())));
    }
    if head.len() == 2 && head.bytes().all(|b| b.is_ascii_digit()) {
        let label = cell
            .split('·')
            .nth(1)
            .map(|rest| rest.split('（').next().unwrap_or(rest).trim().to_string());
        return Some((head.to_string(), label));
    }
    None
}

/// 「05-安卓运维。- 05（82 msgs…」→ Some("05-安卓运维。- 05")；无编号返回 None
fn numbered_name_before_paren(title: &str) -> Option<String> {
    let name = title.split('（').next()?.trim();
    if extract_ordinal(name).is_some() {
        Some(name.to_string())
    } else {
        None
    }
}

/// 「…（**=整理主线 - 02**：…」→ Some("整理主线 - 02")；只认 **= 开头的粗体注记
fn annotated_name(title: &str) -> Option<String> {
    let start = title.find("**=")? + "**=".len();
    let end = title[start..].find("**")? + start;
    let cand = title[start..end].trim();
    if extract_ordinal(cand).is_some() {
        Some(cand.to_string())
    } else {
        None
    }
}

/// 组 MD 首列的编号标记：「`sess_…`（- 01）」→ Some(1)；标记有全角括号包着，不能走尾部数字判定
fn ordinal_from_marker(s: &str) -> Option<u32> {
    let p = s.find("（- ")? + "（- ".len();
    let rest = &s[p..];
    let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    if rest[digits..].trim_start().starts_with('）') {
        rest[..digits].parse().ok()
    } else {
        None
    }
}

/// 名字末尾的编号："05-安卓运维。- 13" → Some(13)；"Selection side chat ×2" → None
/// 门槛：去尾号后必须以 '-' 收尾（「- 13」的连字符），挡住 ×2 / ×12 这类计数
fn extract_ordinal(s: &str) -> Option<u32> {
    let t = s.trim_end();
    let digits = t.bytes().rev().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let head = t[..t.len() - digits].trim_end();
    if head.ends_with('-') {
        t[t.len() - digits..].parse().ok()
    } else {
        None
    }
}

/// 名字 → 组渲染前缀："05-安卓运维。- 13" → "05-安卓运维。"；"整理主线 - 02" → "整理主线"
fn name_prefix(name: &str) -> Option<String> {
    let t = name.trim_end();
    let digits = t.bytes().rev().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    Some(
        t[..t.len() - digits]
            .trim_end()
            .trim_end_matches('-')
            .trim_end()
            .to_string(),
    )
}

/// 组前缀 + 序号 → 构造名：句号收尾的组名直接接「- NN」，否则补空格（只用于台账未给名的追认行）
fn render_name(prefix: &str, ordinal: u32) -> String {
    if prefix.ends_with('。') {
        format!("{prefix}- {ordinal:02}")
    } else {
        format!("{prefix} - {ordinal:02}")
    }
}

/// 条目渲染名：台账有名字就原样（保留「A股投资与量化。- 03」这类旧式命名的原貌），
/// 没名字才按组前缀构造（追认行）
fn display_name(recorded: Option<&str>, prefix: &str, ordinal: u32) -> String {
    match recorded {
        Some(n) => n.to_string(),
        None => render_name(prefix, ordinal),
    }
}

/// 首个 sess_… / 裸 UUID（8-4-4-4-12 十六进制）；多 id 单元格取第一个
fn find_sess_id(cell: &str) -> Option<String> {
    if let Some(p) = cell.find("sess_") {
        let rest = &cell[p + "sess_".len()..];
        let id: String = rest
            .chars()
            .take_while(|c| c.is_ascii_hexdigit() || *c == '-')
            .collect();
        if !id.is_empty() {
            return Some(format!("sess_{id}"));
        }
    }
    // 裸 UUID：8-4-4-4-12 形态；窗口两端必须落在 char 边界（单元格含中文）
    let b = cell.as_bytes();
    for i in 0..b.len() {
        let end = i + 36;
        if end > b.len() || !cell.is_char_boundary(i) || !cell.is_char_boundary(end) {
            continue;
        }
        let seg = &cell[i..end];
        let hyphens: Vec<usize> = seg
            .bytes()
            .enumerate()
            .filter(|(_, c)| *c == b'-')
            .map(|(j, _)| j)
            .collect();
        if hyphens == [8, 13, 18, 23]
            && seg.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-')
        {
            return Some(seg.to_string());
        }
    }
    None
}

// ==================== 测试（合成台账片段，口径锚定） ====================

#[cfg(test)]
mod tests {
    use super::*;

    const MD: &str = r#"## 涉及会话

| 会话 | 时间 | 标题 | 流水 |
|---|---|---|---|
| `sess_11111111-1111-4111-8111-111111111111` | 10-08 | 开山。（159 msgs） | 在 |
| `sess_22222222-2222-4222-8222-222222222222`（- 01） | 10-09 | 05-安卓运维。- 01（133 msgs） | 在 |
| `sess_33333333-3333-4333-8333-333333333333`（- 02） | 10-10 | 政策梳理与筛选（210 msgs） | 在 |
"#;

    const LEDGER00: &str = r#"# 附录

> ⚠️ 尚有会话未入册：`eb27a134` zcode 主模型实验（09-24，组05，附属）、`6f013e7d`/`7b112dc9` Selection side chat ×2（09-28 深夜，组01 附属侧聊）、`abcd1234` Rust 学习。- 01（09-26~28，组09）、`91225874` 10-零散咨询任务。- 05（10-09 20:34 起，组10，129 msgs 在线）。

| # | 会话 ID（sess_ 前缀已略） | 日期 | 类型 | 标题 | 分组 | 流水 |
|---|---|---|---|---|---|---|
| 1 | `44444444-4444-4444-8444-444444444444` | 10-12 晚 | 主会话 | 阅读"AI Related\会话整理"下的 00 和 11，之后立个项（**=11-A 股投资与量化。- 07**：190 msgs，19:50） | 11·A股投资与量化 | 在 |
| 2 | `55555555-5555-4555-8555-555555555555` | 10-14 | 主会话 | 05-安卓运维。- 14（65 msgs，12:18~13:14，自登记） | 05·安卓运维 | 在 |
| 3 | `66666666-6666-4666-8666-666666666666` | 10-14 | 主会话 | 整理会话总结并同步MD文件。- 03（进行中） | cur·当前 | 在 |
| 5 | `88888888-8888-4888-8888-888888888888` | 10-14 | 主会话 | 阅读 00 和 06，之后复核（**=06-战地6武器数据。- 03**：判读=v0.1 四胜 R/|MDD| 16.44 全表最高，自登记） | 06·战地6数据 | 在 |
| 4 | `77777777-7777-4777-8777-777777777777` | 08-11 | 主会话 | hello? | 10·零散咨询 | 已清理 |
"#;

    fn feed() -> BTreeMap<String, GroupAcc> {
        let mut groups = BTreeMap::new();
        parse_group_md(MD, "05".into(), "05-安卓运维".into(), &mut groups);
        let mut stems = BTreeMap::new();
        stems.insert("05".to_string(), "05-安卓运维".to_string());
        stems.insert("11".to_string(), "11-A股投资与量化".to_string());
        parse_ledger00(LEDGER00, &stems, &mut groups);
        groups
    }

    #[test]
    fn group_md_ordinals_and_ids() {
        let groups = feed();
        let g05 = groups.get("05").unwrap();
        // 开山行无（- NN）不入账；00 表补 -14 后共 3 条
        assert_eq!(g05.items.len(), 3);
        let it = g05.items.get(&2).unwrap();
        // 标题「政策梳理与筛选」不带编号 → 名字留空，渲染层按组前缀归一补齐
        assert_eq!(it.name, None);
        assert!(it.session_id.as_deref().unwrap().starts_with("sess_33333333"));
        let it14 = g05.items.get(&14).unwrap();
        assert!(it14.session_id.as_deref().unwrap().starts_with("sess_55555555"));
        // 行内带「自登记」→ 自登记档
        assert!(it14.self_registered);
    }

    #[test]
    fn ledger00_annotation_direct_and_cur() {
        let groups = feed();
        let g11 = groups.get("11").unwrap();
        assert_eq!(
            g11.items.get(&7).unwrap().name.as_deref(),
            Some("11-A 股投资与量化。- 07")
        );
        // 行内无「自登记」字样的正表行 = 主线代登记 → 已登记档（不亮自登记）
        let it7 = g11.items.get(&7).unwrap();
        assert!(it7.collected && !it7.self_registered);
        let cur = groups.get("cur").unwrap();
        assert_eq!(cur.label, "会话整理主线");
        // cur 行（会话整理主线）恒已登记：主线行=「主线已登记」的定义本身，存档才熄标
        let it3 = cur.items.get(&3).unwrap();
        assert!(it3.collected && !it3.self_registered);
        assert_eq!(
            cur.items.get(&3).unwrap().name.as_deref(),
            Some("整理会话总结并同步MD文件。- 03")
        );
    }

    #[test]
    fn mainline_and_collected_wording_light_collected_only() {
        // 主线代写的正态行 + 清字后的「已登记」字样行都只亮已登记档
        let txt = "| 30 | `99999999-9999-4999-8999-999999999999` | 10-15 | 主会话 | 11-A 股投资与量化。- 12（主线补，正态未自登） | 11·A股投资与量化 | 在 |\n\
                   | 31 | `aaaa0000-0000-4000-8000-000000000000` | 10-15 | 主会话 | 05-安卓运维。- 16（已登记：主线过手） | 05·安卓运维 | 在 |\n";
        let mut groups = BTreeMap::new();
        let mut stems = BTreeMap::new();
        stems.insert("11".to_string(), "11-A股投资与量化".to_string());
        stems.insert("05".to_string(), "05-安卓运维".to_string());
        parse_ledger00(txt, &stems, &mut groups);
        let it12 = groups.get("11").unwrap().items.get(&12).unwrap();
        assert!(it12.collected && !it12.self_registered);
        let it16 = groups.get("05").unwrap().items.get(&16).unwrap();
        assert!(it16.collected && !it16.self_registered);
    }

    #[test]
    fn pending_headnote_only_numbered_with_group() {
        let groups = feed();
        // 组 05 = MD 2 条 + 00 表 -14；zcode 主模型实验（无编号）被拒
        assert_eq!(groups.get("05").unwrap().items.len(), 3);
        // Selection side chat ×2（×计数非编号）不得创建组 01
        assert!(groups.get("01").is_none());
        let g09 = groups.get("09").unwrap();
        let it = g09.items.get(&1).unwrap();
        assert!(it.unregistered);
        assert_eq!(it.id_hint.as_deref(), Some("abcd1234"));
        assert_eq!(it.name.as_deref(), Some("Rust 学习。- 01"));
        let g10 = groups.get("10").unwrap();
        assert_eq!(g10.items.get(&5).unwrap().id_hint.as_deref(), Some("91225874"));
    }

    #[test]
    fn pipe_in_title_row_still_flags_self_registered() {
        let groups = feed();
        let g06 = groups.get("06").unwrap();
        let it = g06.items.get(&3).unwrap();
        // 标题里的「R/|MDD|」把行切成 11 段：分组列右锚后仍归组 06，且「自登记」照常点亮
        assert!(it.self_registered);
        assert!(it.session_id.as_deref().unwrap().starts_with("sess_88888888"));
    }

    #[test]
    fn unnumbered_rows_skipped() {
        let groups = feed();
        // hello? 无编号不入组 10
        assert!(groups.get("10").unwrap().items.get(&1).is_none());
    }

    #[test]
    fn ordinal_guards() {
        assert_eq!(extract_ordinal("05-安卓运维。- 13"), Some(13));
        assert_eq!(extract_ordinal("整理主线 - 02"), Some(2));
        assert_eq!(extract_ordinal("Selection side chat ×2"), None);
        assert_eq!(extract_ordinal("崩铁数据库。"), None);
        assert_eq!(extract_ordinal("识别图中文字。"), None);
        // 组 MD 首列标记：全角括号包着的编号（尾部是「）」不能走尾部数字判定）
        assert_eq!(ordinal_from_marker(" `sess_22222222`（- 01） "), Some(1));
        assert_eq!(ordinal_from_marker(" `sess_x`（- 14）"), Some(14));
        assert_eq!(ordinal_from_marker(" `sess_开山` | 10-08"), None);
        assert_eq!(ordinal_from_marker(" `sess_x`（- 无）"), None);
    }

    #[test]
    fn render_normalizes_by_group_prefix() {
        assert_eq!(render_name("05-安卓运维。", 5), "05-安卓运维。- 05");
        assert_eq!(render_name("整理主线", 2), "整理主线 - 02");
        assert_eq!(name_prefix("05-安卓运维。- 13").as_deref(), Some("05-安卓运维。"));
        assert_eq!(name_prefix("整理主线 - 02").as_deref(), Some("整理主线"));
    }

    #[test]
    fn display_prefers_recorded_name_verbatim() {
        // 台账原名优先：旧式无前缀命名保留原貌，不做时代错置的归一
        assert_eq!(
            display_name(Some("A股投资与量化。- 03"), "11-A 股投资与量化。", 3),
            "A股投资与量化。- 03"
        );
        // 台账未给名的追认行才构造
        assert_eq!(
            display_name(None, "11-A 股投资与量化。", 1),
            "11-A 股投资与量化。- 01"
        );
    }

    #[test]
    fn bare_uuid_and_sess_prefix() {
        assert_eq!(
            find_sess_id("`44444444-4444-4444-8444-444444444444` | 10-12").as_deref(),
            Some("44444444-4444-4444-8444-444444444444")
        );
        assert_eq!(
            find_sess_id("`sess_abc-def` x").as_deref(),
            Some("sess_abc-def")
        );
        // 中文紧邻 UUID 不得 panic（char 边界守卫）
        assert_eq!(
            find_sess_id("父=abcd1234-5678-4abc-9def-123456789abc）").as_deref(),
            Some("abcd1234-5678-4abc-9def-123456789abc")
        );
    }
}

