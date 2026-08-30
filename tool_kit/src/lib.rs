/// tool_kit — 工具子进程契约（server 与 kzm-* 工具二进制共用的协议类型与 CLI 骨架）
///
/// 契约（服务器与工具之间的进程间协议）：
/// - `<bin> decl`   → stdout 输出一行 ToolDecl JSON（工具定义）
/// - `<bin> call`   → stdin 读取 JSON 参数，stdout 输出 ToolOutput JSON（执行结果）
///
/// 工具改动后重新编译，二进制落盘即生效；服务器端「重载」会重新探测 decl 并登记。
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;

/// 工具定义（decl 子命令输出）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDecl {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
    pub input_schema: Value,
}

/// 工具注解（与 MCP ToolAnnotations 对齐，camelCase 序列化）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolAnnotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

impl ToolAnnotations {
    pub fn read_only() -> Self {
        Self { read_only_hint: Some(true), ..Default::default() }
    }
    pub fn destructive() -> Self {
        Self { destructive_hint: Some(true), ..Default::default() }
    }
    pub fn open_world_read_only() -> Self {
        Self { read_only_hint: Some(true), open_world_hint: Some(true), ..Default::default() }
    }
    pub fn open_world() -> Self {
        Self { open_world_hint: Some(true), ..Default::default() }
    }
    pub fn writes() -> Self {
        Self { read_only_hint: Some(false), ..Default::default() }
    }
}

/// 工具执行结果（call 子命令输出；字段与服务器端 executor::ToolResult 对齐）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolOutput {
    pub fn ok(data: impl Serialize) -> Self {
        Self {
            success: true,
            data: Some(serde_json::to_value(data).unwrap_or_default()),
            error: None,
        }
    }
    pub fn err(msg: impl Into<String>) -> Self {
        Self { success: false, data: None, error: Some(msg.into()) }
    }
}

/// 从 stdin 读取 call 参数（空/非法输入 → Null，由工具的参数检查报错）
pub fn read_call_args() -> Value {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    serde_json::from_str(&input).unwrap_or(Value::Null)
}

/// 工具主程序骨架：
///
/// ```ignore
/// use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};
/// use serde_json::{json, Value};
///
/// fn run(args: Value) -> ToolOutput { /* ... */ ToolOutput::ok(json!({})) }
///
/// kzm_tool!(
///     ToolDecl {
///         name: "add".into(),
///         title: Some("加法计算".into()),
///         description: "…".into(),
///         annotations: Some(ToolAnnotations::read_only()),
///         input_schema: json!({ "type": "object" }),
///     },
///     run
/// );
/// ```
#[macro_export]
macro_rules! kzm_tool {
    ($decl:expr, $run:path) => {
        fn main() {
            let mode = std::env::args().nth(1).unwrap_or_else(|| "call".to_string());
            match mode.as_str() {
                "decl" => {
                    let mut decl: $crate::ToolDecl = $decl;
                    // 侧车定义文件：<二进制>.decl.json 存在则覆盖内置定义——
                    // "define 单独保存为一个文件，由工具调用"：改定义无需重编译，
                    // 服务器「重载」后即生效（解析失败时回退内置定义并在 stderr 提示）
                    if let Ok(exe) = std::env::current_exe() {
                        let sidecar = exe.with_extension("decl.json");
                        if sidecar.exists() {
                            match std::fs::read_to_string(&sidecar)
                                .ok()
                                .and_then(|s| serde_json::from_str::<$crate::ToolDecl>(&s).ok())
                            {
                                Some(custom) => decl = custom,
                                None => eprintln!(
                                    "[tool_kit] 侧车定义文件解析失败，使用内置定义: {}",
                                    sidecar.display()
                                ),
                            }
                        }
                    }
                    let body = serde_json::to_string(&decl).expect("序列化 ToolDecl 失败");
                    println!("{body}");
                }
                _ => {
                    let args = $crate::read_call_args();
                    let out = $run(args);
                    let body = serde_json::to_string(&out).expect("序列化 ToolOutput 失败");
                    println!("{body}");
                }
            }
        }
    };
}
