//! kzm-memory-daemon — 常驻嵌入服务（不带 kzm- 前缀，不参与插件发现）
//!
//! 职责：把 94.9MB 的 bge 模型常驻内存，避免每次工具调用冷启动加载（实测冷启动 ~2.3s，
//! 常驻后单条 ~45ms）。协议：Unix domain socket + NDJSON，一连接一请求：
//!
//!   请求 {"op":"embed","texts":["...", ...]}  → {"ok":true,"vectors":[[...], ...]}
//!   请求 {"op":"ping"}                        → {"ok":true}
//!   请求 {"op":"shutdown"}                    → {"ok":true} 后退出
//!
//! 生命周期：`KZM_MEMORY_DAEMON_IDLE_SECS`（缺省 600 = 10 分钟）内没有任何连接
//! → 自行退出卸载模型（需求：常驻 10 分钟无新调用即卸载）。
//! 孤儿安全：绑定失败且 socket 存活 → 退出 0（已有实例）；socket 陈旧 → 清理后重绑。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

// 共享库按二进制分别编译；daemon 只用嵌入部分
#[allow(dead_code)]
#[path = "../lib.rs"]
mod memory;

pub fn socket_path() -> PathBuf {
    PathBuf::from(
        std::env::var("KZM_MEMORY_DAEMON_SOCK").unwrap_or_else(|_| "/tmp/kzm-memory-daemon.sock".into()),
    )
}

pub fn idle_secs() -> u64 {
    std::env::var("KZM_MEMORY_DAEMON_IDLE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600)
}

#[derive(Deserialize)]
struct Request {
    op: String,
    #[serde(default)]
    texts: Vec<String>,
}

#[derive(Serialize)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    vectors: Option<Vec<Vec<f32>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn respond(stream: &mut UnixStream, resp: &Response) {
    let mut line = serde_json::to_string(resp).unwrap_or_default();
    line.push('\n');
    let _ = stream.write_all(line.as_bytes());
    let _ = stream.flush();
}

fn main() {
    let sock = socket_path();
    let idle = Duration::from_secs(idle_secs());

    // 已有存活实例 → 静默退出（并发孵化幂等）
    if UnixStream::connect(&sock).is_ok() {
        eprintln!("[memory-daemon] 已有实例在运行，退出");
        std::process::exit(0);
    }
    let _ = std::fs::remove_file(&sock); // 陈旧 socket

    // 模型加载放绑定了 socket 之后：客户端以 socket 存在作为“就绪”信号
    let mut embedder = match memory::Embedder::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("[memory-daemon] 模型加载失败: {e:#}");
            std::process::exit(1);
        }
    };

    let listener = match UnixListener::bind(&sock) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[memory-daemon] 绑定 {} 失败: {e}", sock.display());
            std::process::exit(1);
        }
    };
    listener.set_nonblocking(true).expect("设置非阻塞失败");
    eprintln!(
        "[memory-daemon] 就绪: {}（空闲 {} 秒后自卸载）",
        sock.display(),
        idle.as_secs()
    );

    let mut idle_since = Instant::now();
    loop {
        // 非阻塞轮询 accept：空闲计时到点 → 自卸载退出
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(600)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(120)));
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => {
                        idle_since = Instant::now();
                        continue;
                    }
                    Ok(_) => {}
                }
                match serde_json::from_str::<Request>(line.trim()) {
                    Ok(req) => match req.op.as_str() {
                        "ping" => respond(&mut stream, &Response { ok: true, vectors: None, error: None }),
                        "shutdown" => {
                            respond(&mut stream, &Response { ok: true, vectors: None, error: None });
                            eprintln!("[memory-daemon] 收到 shutdown，退出");
                            let _ = std::fs::remove_file(&sock);
                            std::process::exit(0);
                        }
                        "embed" => {
                            let resp = match embedder.embed(req.texts) {
                                Ok(vectors) => Response { ok: true, vectors: Some(vectors), error: None },
                                Err(e) => Response { ok: false, vectors: None, error: Some(format!("{e:#}")) },
                            };
                            respond(&mut stream, &resp);
                        }
                        other => respond(
                            &mut stream,
                            &Response { ok: false, vectors: None, error: Some(format!("未知 op: {other}")) },
                        ),
                    },
                    Err(e) => respond(
                        &mut stream,
                        &Response { ok: false, vectors: None, error: Some(format!("请求解析失败: {e}")) },
                    ),
                }
                idle_since = Instant::now(); // 有新调用 → 重置空闲计时
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if idle_since.elapsed() >= idle {
                    // 空闲超时：卸载模型退出（下次调用由工具重新孵化）
                    eprintln!("[memory-daemon] 空闲超时，卸载模型退出");
                    let _ = std::fs::remove_file(&sock);
                    std::process::exit(0);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                eprintln!("[memory-daemon] accept 异常: {e}");
                std::process::exit(1);
            }
        }
    }
}

// 供调试：打印一条请求样例
#[allow(dead_code)]
fn sample_request() -> Value {
    serde_json::json!({ "op": "embed", "texts": ["示例"] })
}
