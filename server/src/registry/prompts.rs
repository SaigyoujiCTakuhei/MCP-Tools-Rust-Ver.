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
                json!({
                    "name": p.name,
                    "title": p.title,
                    "description": p.description,
                    "arguments": p.arguments,
                })
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
    let mut out = template.to_string();
    if let Some(map) = arguments.as_object() {
        for (k, v) in map {
            let s = v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
            out = out.replace(&format!("{{{{{k}}}}}"), &s);
        }
    }
    Ok(out)
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
