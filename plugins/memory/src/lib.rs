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
