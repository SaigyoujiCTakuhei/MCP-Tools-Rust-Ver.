//! pdf_utils — 公共 PDF 处理函数（对应 v11 的 scripts/pdf_utils.py，消除两个工具的重复）
//!
//! 依赖 pdf-extract（纯 Rust），替代 v11 的 PyPDF2。

/// 从 PDF 字节流提取全文（逐页用换行连接，去除首尾空白）。
pub fn extract_text_from_bytes(bytes: &[u8]) -> Result<String, String> {
    let text = pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| format!("PDF 解析失败: {e}"))?;
    Ok(text.trim().to_string())
}

/// 从本地文件提取全文（一次性读入内存，避免句柄/seek 问题——与 v11 同思路）。
pub fn extract_text_from_file(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取 PDF 文件失败: {e}"))?;
    extract_text_from_bytes(&bytes)
}
