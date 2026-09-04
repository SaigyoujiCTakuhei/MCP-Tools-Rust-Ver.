/// 提示词与资源注册表 — 文件驱动、可热重载（需求四）
///
/// 目录约定（config.mcp.prompts_path / resources_path）：
/// - prompts/*.json        结构化定义 {name,title,description,arguments,template}
/// - prompts/*.md          Markdown + YAML front matter（兼容 v11 Anthropic Skills 格式：
///                          name / description / params|arguments 列表，正文为模板，{{arg}} 占位）
/// - resources/*.json      {uri,name,description,mimeType,text | file}
///
/// 加载失败（解析错误等）不阻断启动，逐条写 ERROR 日志后跳过该文件。
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

use crate::mcp::handler::LogSystem;

// ==================== 提示词 ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 功能分组（仅 Dashboard 中文界面用；缺省归入「未分类」）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
    #[serde(default)]
    pub template: String,
}

#[derive(Debug)]
pub struct PromptRegistry {
    entries: RwLock<Vec<PromptDecl>>,
}

impl PromptRegistry {
    pub fn new() -> Self {
        Self { entries: RwLock::new(Vec::new()) }
    }

    /// 从目录加载（*.json + *.md），整体替换当前列表并按名称排序
    pub async fn reload_from_dir(&self, dir: &Path, logs: Option<&LogSystem>) -> anyhow::Result<usize> {
        let rd = std::fs::read_dir(dir)
            .with_context(|| format!("读取提示词目录失败: {}", dir.display()))?;
        let mut prompts = Vec::new();
        let mut errors = Vec::new();
        for entry in rd.flatten() {
            let path = entry.path();
            let result = match path.extension().and_then(|e| e.to_str()) {
                Some("json") => std::fs::read_to_string(&path)
                    .context("读取失败")
                    .and_then(|s| serde_json::from_str::<PromptDecl>(&s).context("JSON 解析失败")),
                Some("md") => parse_markdown_prompt(&path),
                _ => continue,
            };
            match result {
                Ok(mut decl) => {
                    if decl.title.is_none() {
                        decl.title = Some(decl.name.clone());
                    }
                    prompts.push(decl);
                }
                Err(e) => errors.push(format!("{}: {:#}", path.display(), e)),
            }
        }
        prompts.sort_by(|a, b| a.name.cmp(&b.name));
        let count = prompts.len();
        *self.entries.write().await = prompts;
        for e in errors {
            match logs {
                Some(l) => l.log("ERROR", format!("提示词加载失败: {e}")).await,
                None => tracing::error!("提示词加载失败: {e}"),
            }
        }
        Ok(count)
    }

    /// prompts/list 输出（不含模板正文）
    pub async fn list(&self) -> Vec<Value> {
        self.entries
            .read()
            .await
            .iter()
            .map(|p| {
                // 注意：官方 SDK 的 zod 校验接受「字段缺省」但不接受显式 null
                // （arguments/title 为 null 会使整个 listPrompts 结果判无效），
                // 因此可选字段在 None 时必须省略而非输出 null
                let mut item = json!({
                    "name": p.name,
                    "description": p.description,
                });
                if let Some(title) = &p.title {
                    item["title"] = json!(title);
                }
                if let Some(category) = &p.category {
                    item["category"] = json!(category);
                }
                if let Some(args) = &p.arguments {
                    item["arguments"] = json!(args);
                }
                item
            })
            .collect()
    }

    /// prompts/get：校验必填参数并渲染 {{arg}} 占位符
    pub async fn get(&self, name: &str, arguments: &Value) -> anyhow::Result<Value> {
        let entries = self.entries.read().await;
        let decl = entries
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| anyhow::anyhow!("Unknown prompt: {name}"))?;
        let rendered = render_template(&decl.template, arguments, &decl.arguments)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(json!({
            "description": decl.description,
            "messages": [{ "role": "user", "content": { "type": "text", "text": rendered } }],
        }))
    }
}

/// 解析 v11 / Anthropic Skills 风格的 Markdown 提示词（YAML front matter + 模板正文）
fn parse_markdown_prompt(path: &Path) -> anyhow::Result<PromptDecl> {
    let content = std::fs::read_to_string(path).context("读取失败")?;
    let content = content.trim_start_matches('\u{feff}');
    let rest = content
        .strip_prefix("---")
        .ok_or_else(|| anyhow::anyhow!("缺少 front matter 起始 ---"))?;
    let (front, body) = rest
        .split_once("\n---")
        .ok_or_else(|| anyhow::anyhow!("缺少 front matter 结束 ---"))?;

    #[derive(Deserialize)]
    struct FrontMatter {
        name: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        category: Option<String>,
        description: String,
        #[serde(default, alias = "arguments")]
        params: Option<Vec<PromptArgument>>,
    }
    let fm: FrontMatter = serde_yaml::from_str(front)
        .with_context(|| "YAML front matter 解析失败")?;

    // 正文去掉紧跟结束符的空行/重复 --- 行
    let mut template = body.trim_start_matches('\n').to_string();
    if template.starts_with("---") {
        template = template[3..].trim_start_matches('\n').to_string();
    }
    Ok(PromptDecl {
        name: fm.name,
        title: fm.title,
        category: fm.category,
        description: fm.description,
        arguments: fm.params.filter(|p| !p.is_empty()),
        template,
    })
}

fn render_template(
    template: &str,
    arguments: &Value,
    decl_args: &Option<Vec<PromptArgument>>,
) -> Result<String, String> {
    if let Some(list) = decl_args {
        for a in list {
            let missing = arguments.get(&a.name).map(Value::is_null).unwrap_or(true);
            if a.required && missing {
                return Err(format!("缺少必填参数: {}", a.name));
            }
        }
    }
    // 收集「名字 → 值」；空串视为未提供（占位符走各模板自带的默认语义）
    let mut map: Vec<(String, String)> = Vec::new();
    if let Some(obj) = arguments.as_object() {
        for (k, v) in obj {
            if v.is_null() {
                continue;
            }
            let s = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            if !s.is_empty() {
                map.push((k.clone(), s));
            }
        }
    }

    let mut out = template.to_string();
    // 第一遍：双花括号 {{name}}（MCP 惯例）
    for (k, v) in &map {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    // 第二遍：单花括号 {name}（v11 模板的 Python format() 惯例，
    // 如 dm-text-game 的 {base}/{opening_text}/{player_name}）
    for (k, v) in &map {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    // v11 嵌套别名：参数值内部的 {{user}}/{user} 指代玩家名
    // （如 opening_text 的开场白含 {{user}} 时替换为玩家名）；
    // 未提供 player_name 时按其参数描述回退为「你」
    let player = map
        .iter()
        .find(|(k, _)| k == "player_name")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "你".to_string());
    out = out.replace("{{user}}", &player).replace("{user}", &player);
    // player_name 未提供时，按其参数描述回退为「你」
    if !map.iter().any(|(k, _)| k == "player_name") {
        out = out.replace("{{player_name}}", &player).replace("{player_name}", &player);
    }
    Ok(out)
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn single_brace_v11_convention() {
        let decl = Some(vec![PromptArgument { name: "base".into(), description: String::new(), required: false }]);
        let out = render_template("基底：{base}", &json!({ "base": "123" }), &decl).unwrap();
        assert_eq!(out, "基底：123");
    }

    #[test]
    fn double_brace_mcp_convention() {
        let decl = Some(vec![PromptArgument { name: "code".into(), description: String::new(), required: true }]);
        let out = render_template("评审：{{code}}", &json!({ "code": "fn main(){}" }), &decl).unwrap();
        assert_eq!(out, "评审：fn main(){}");
    }

    #[test]
    fn user_alias_nested_in_value() {
        // v11 语义：opening_text 的值里可含 {{user}}，替换为玩家名
        let decl = Some(vec![
            PromptArgument { name: "opening_text".into(), description: String::new(), required: false },
            PromptArgument { name: "player_name".into(), description: String::new(), required: false },
        ]);
        let out = render_template(
            "{{opening_text}}",
            &json!({ "opening_text": "{{user}}醒来", "player_name": "血月" }),
            &decl,
        ).unwrap();
        assert_eq!(out, "血月醒来");
    }

    #[test]
    fn player_name_defaults_to_ni() {
        // 参数描述承诺：player_name 为空时默认「你」
        let out = render_template("你是{{player_name}}", &json!({}), &None).unwrap();
        assert_eq!(out, "你是你");
    }

    #[test]
    fn required_missing_rejected() {
        let decl = Some(vec![PromptArgument { name: "code".into(), description: String::new(), required: true }]);
        assert!(render_template("{{code}}", &json!({}), &decl).is_err());
    }
}

// ==================== 资源 ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDecl {
    pub uri: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// 内联文本内容（与 file 二选一）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// 相对于资源目录的文件路径（读取时取最新内容 → 改文件即热更新）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

pub struct ResourceRegistry {
    base_dir: PathBuf,
    entries: RwLock<Vec<ResourceDecl>>,
}

impl ResourceRegistry {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir, entries: RwLock::new(Vec::new()) }
    }

    pub async fn reload_from_dir(&self, logs: Option<&LogSystem>) -> anyhow::Result<usize> {
        let rd = std::fs::read_dir(&self.base_dir).with_context(|| {
            format!("读取资源目录失败: {}", self.base_dir.display())
        })?;
        let mut resources = Vec::new();
        let mut errors = Vec::new();
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read_to_string(&path)
                .context("读取失败")
                .and_then(|s| serde_json::from_str::<ResourceDecl>(&s).context("JSON 解析失败"))
            {
                Ok(r) => resources.push(r),
                Err(e) => errors.push(format!("{}: {:#}", path.display(), e)),
            }
        }
        resources.sort_by(|a, b| a.uri.cmp(&b.uri));
        let count = resources.len();
        *self.entries.write().await = resources;
        for e in errors {
            match logs {
                Some(l) => l.log("ERROR", format!("资源加载失败: {e}")).await,
                None => tracing::error!("资源加载失败: {e}"),
            }
        }
        Ok(count)
    }

    /// resources/list 输出
    pub async fn list(&self) -> Vec<Value> {
        self.entries
            .read()
            .await
            .iter()
            .map(|r| {
                json!({
                    "uri": r.uri,
                    "name": r.name,
                    "description": r.description,
                    "mimeType": r.mime_type,
                })
            })
            .collect()
    }

    /// resources/read：返回 contents 条目（file 型资源每次读取最新内容）
    pub async fn read(&self, uri: &str) -> anyhow::Result<Value> {
        let entries = self.entries.read().await;
        let decl = entries
            .iter()
            .find(|r| r.uri == uri)
            .ok_or_else(|| anyhow::anyhow!("Unknown resource: {uri}"))?;
        let text = if let Some(t) = &decl.text {
            t.clone()
        } else if let Some(f) = &decl.file {
            let path = self.base_dir.join(f);
            match std::fs::read(&path) {
                Ok(b) => String::from_utf8_lossy(&b).to_string(),
                Err(e) => bail!("读取资源文件失败 {}: {e}", path.display()),
            }
        } else {
            bail!("资源 {uri} 未定义 text 或 file 内容");
        };
        Ok(json!({
            "uri": decl.uri,
            "mimeType": decl.mime_type,
            "text": text,
        }))
    }
}
