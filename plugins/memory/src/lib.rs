//! memory 域共享库 — 数据库访问 + 本地嵌入（bge-small-zh-v1.5）
//!
//! 数据存储：复用既有表 `public.memory_chunks`
//! （id BIGSERIAL / source / source_date / content / embedding vector(512) / created_at，
//!   HNSW 余弦索引已建，存量 172 条真实记忆）。
//! 嵌入模型：本地 ONNX（bge-small-zh-v1.5，512 维，CLS 池化），
//! 通过 fastembed 的 user-defined 模式直接读取平铺目录，不经 hf-hub 缓存、不联网。
//!
//! 环境变量：
//! - `KZM_MEMORY_DSN`       — PG 连接串；缺省 `postgresql:///Agent_Memories?host=/var/run/postgresql`
//!                            （Unix socket + peer 认证，本机用户 p 与 PG 角色 p 同名，免密）
//! - `KZM_MEMORY_MODEL_DIR` — 模型目录；缺省 `/home/p/AI Related/Embedding Models/bge-small-zh-v1.5/Xenova`
//!                            （须含 model.onnx / tokenizer.json / config.json /
//!                             special_tokens_map.json / tokenizer_config.json）

use anyhow::{anyhow, Context};
use postgres::{Client, NoTls};
use serde_json::{json, Value};
// 本文件被 lib 与 6 个 bin 以 #[path] 方式分别编译，各上下文活跃代码不同，
// 这三个 io trait 在部分上下文只被 dead_code 函数引用——统一豁免
#[allow(unused_imports)]
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

pub const EMBED_DIMS: usize = 512;
pub const DEFAULT_MODEL_DIR: &str =
    "/home/p/AI Related/Embedding Models/bge-small-zh-v1.5/Xenova";
pub const DEFAULT_DSN: &str = "postgresql:///Agent_Memories?host=/var/run/postgresql";

pub fn dsn() -> String {
    std::env::var("KZM_MEMORY_DSN").unwrap_or_else(|_| DEFAULT_DSN.to_string())
}

pub fn model_dir() -> String {
    std::env::var("KZM_MEMORY_MODEL_DIR").unwrap_or_else(|_| DEFAULT_MODEL_DIR.to_string())
}

// ==================== 数据库 ====================

pub fn connect() -> anyhow::Result<Client> {
    Client::connect(&dsn(), NoTls).context("连接 PostgreSQL 失败（KZM_MEMORY_DSN）")
}

/// 把 f32 向量转成 pgvector 的绑定类型（二进制协议序列化）
pub fn to_pg_vector(v: &[f32]) -> pgvector::Vector {
    pgvector::Vector::from(v.to_vec())
}

// ==================== 本地嵌入 ====================

pub struct Embedder {
    model: fastembed::TextEmbedding,
}

impl Embedder {
    /// 加载本地模型（CLS 池化，与 bge-small-zh-v1.5 官方用法一致；512 token 截断）
    pub fn new() -> anyhow::Result<Self> {
        let dir = model_dir();
        let read = |name: &str| -> anyhow::Result<Vec<u8>> {
            std::fs::read(std::path::Path::new(&dir).join(name))
                .with_context(|| format!("读取模型文件失败: {dir}/{name}"))
        };
        let model = fastembed::UserDefinedEmbeddingModel {
            onnx_file: read("model.onnx")?,
            external_initializers: vec![],
            tokenizer_files: fastembed::TokenizerFiles {
                tokenizer_file: read("tokenizer.json")?,
                config_file: read("config.json")?,
                special_tokens_map_file: read("special_tokens_map.json")?,
                tokenizer_config_file: read("tokenizer_config.json")?,
            },
            pooling: Some(fastembed::Pooling::Cls),
            quantization: fastembed::QuantizationMode::None,
            output_key: None,
        };
        let text_model = fastembed::TextEmbedding::try_new_from_user_defined(
            model,
            fastembed::InitOptionsUserDefined::new().with_max_length(512),
        )
        .map_err(|e| anyhow!("加载本地嵌入模型失败（目录: {dir}）: {e}"))?;
        Ok(Self { model: text_model })
    }

    /// 批量嵌入，校验维度 = 512
    pub fn embed(&mut self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        let out = self
            .model
            .embed(texts, None)
            .map_err(|e| anyhow!("嵌入计算失败: {e}"))?;
        for v in &out {
            if v.len() != EMBED_DIMS {
                return Err(anyhow!("嵌入维度异常：期望 {EMBED_DIMS}，实际 {}", v.len()));
            }
        }
        Ok(out)
    }
}

// ==================== 常驻嵌入服务（daemon）与阈值路由 ====================
//
// 规则（用户定义）：
//   - 一次读写超过 RESIDENT_THRESHOLD 条 → 使用常驻服务（不在则孵化，模型加载一次）
//   - 常驻服务空闲 KZM_MEMORY_DAEMON_IDLE_SECS（缺省 600 = 10 分钟）无新调用 → 自行卸载
//   - ≤ 阈值的小读写 → 冷启动；但若常驻服务已在运行则复用（顺便刷新空闲计时）
//   - 常驻服务不可用时一律回退冷启动（降级不失败）

pub const RESIDENT_THRESHOLD: usize = 5;

pub fn daemon_socket_path() -> PathBuf {
    PathBuf::from(
        std::env::var("KZM_MEMORY_DAEMON_SOCK").unwrap_or_else(|_| "/tmp/kzm-memory-daemon.sock".into()),
    )
}

pub fn daemon_idle_secs() -> u64 {
    std::env::var("KZM_MEMORY_DAEMON_IDLE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600)
}

fn daemon_bin_path() -> PathBuf {
    if let Ok(p) = std::env::var("KZM_MEMORY_DAEMON_BIN") {
        return PathBuf::from(p);
    }
    // 守护进程与工具二进制同目录（不带 kzm- 前缀，不参与插件发现）
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("memory-daemon")))
        .unwrap_or_else(|| PathBuf::from("memory-daemon"))
}

/// 探活：socket 可连且 ping 应答
pub fn daemon_ping() -> bool {
    daemon_request(&json!({ "op": "ping" }))
        .map(|v| v.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .unwrap_or(false)
}

/// 单连接单请求：发送 NDJSON 请求行，读取一行响应
fn daemon_request(req: &Value) -> anyhow::Result<Value> {
    use std::io::{BufRead, BufReader, Write};
    let mut stream = UnixStream::connect(daemon_socket_path())
        .with_context(|| format!("连接嵌入服务失败: {}", daemon_socket_path().display()))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(600)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    stream
        .write_all(serde_json::to_string(req)?.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .and_then(|_| stream.flush())
        .context("发送嵌入请求失败")?;
    let mut line = String::new();
    BufReader::new(&stream)
        .read_line(&mut line)
        .context("读取嵌入响应失败")?;
    serde_json::from_str(line.trim()).context("嵌入响应不是合法 JSON")
}

/// 孵化守护进程并等待就绪（模型加载需要数秒，轮询 socket 至多 120 秒）
fn daemon_spawn_and_wait() -> anyhow::Result<()> {
    let bin = daemon_bin_path();
    let sock = daemon_socket_path();
    if !bin.exists() {
        anyhow::bail!("守护进程二进制不存在: {}", bin.display());
    }
    let _ = std::fs::remove_file(&sock); // 清理陈旧 socket
    std::process::Command::new(&bin)
        .arg("serve")
        .spawn()
        .with_context(|| format!("孵化嵌入守护进程失败: {}", bin.display()))?;
    // 孤儿化：Child 立即 drop，由 init 收养，父进程退出不影响
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    while std::time::Instant::now() < deadline {
        if daemon_ping() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    anyhow::bail!("等待嵌入守护进程就绪超时（120 秒）")
}

fn daemon_embed(texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
    let resp = daemon_request(&json!({ "op": "embed", "texts": texts }))?;
    if resp.get("ok").and_then(Value::as_bool) != Some(true) {
        anyhow::bail!(
            "嵌入服务返回错误: {}",
            resp.get("error").and_then(Value::as_str).unwrap_or("未知")
        );
    }
    serde_json::from_value(resp.get("vectors").cloned().unwrap_or(Value::Null))
        .context("嵌入响应 vectors 解析失败")
}

/// 嵌入路由（唯一入口）：
///   > 阈值 → 常驻服务（不在则孵化）；≤ 阈值 → 冷启动，但常驻已在则复用；
///   常驻不可用 → 一律回退冷启动。
pub fn embed_texts(texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let threshold = std::env::var("KZM_MEMORY_RESIDENT_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(RESIDENT_THRESHOLD);
    let alive = daemon_ping();
    if texts.len() > threshold || alive {
        let routed = (|| {
            if !alive {
                daemon_spawn_and_wait()?;
            }
            daemon_embed(&texts)
        })();
        match routed {
            Ok(v) => return Ok(v),
            Err(e) => eprintln!("[memory] 常驻嵌入服务不可用（{e:#}），回退冷启动"),
        }
    }
    Embedder::new()?.embed(texts)
}
