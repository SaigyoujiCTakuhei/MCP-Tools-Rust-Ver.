/// 上下文看板模块 — 聚合 ZCode 引擎库（db.sqlite）与会话流水的上下文 / 轮次 / 计费数据
///
/// 查询逻辑独立成模块：WebUI 端点（dashboard/api.rs）与将来的 MCP 工具形态（kzm_ctx_*）共用。
/// 数据口径 2026-09-28 与 ZCode 客户端逐位对表验证：
/// - 当前上下文 = model_usage 最新一行的 input_tokens（含缓存命中；turn_usage 是回合累计计费口径，勿混用）
/// - 自动压缩触发线 = 上下文窗口 − min(最大输出, 21k) − 13k（1M 窗 ≈ 96.6 万）
pub mod query;
