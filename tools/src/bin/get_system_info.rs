use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};

fn run(_args: Value) -> ToolOutput {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    ToolOutput::ok(json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "hostname": hostname::get().map(|h| h.to_string_lossy().to_string()).unwrap_or_else(|_| "unknown".into()),
        "cwd": cwd,
        "pid": std::process::id(),
    }))
}

kzm_tool!(
    ToolDecl {
        name: "get_system_info".into(),
        title: Some("系统信息".into()),
        description: "获取当前系统信息（操作系统、架构、主机名、工作目录等）".into(),
        annotations: Some(ToolAnnotations::read_only()),
        category: Some("系统与命令".into()),
        input_schema: json!({ "type": "object", "properties": {} }),
    },
    run
);
