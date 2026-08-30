use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};
use std::process::{Command, Stdio};

fn run(args: Value) -> ToolOutput {
    let Some(command) = args["command"].as_str() else {
        return ToolOutput::err("缺少 command 参数");
    };
    match run_shell(command) {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            ToolOutput::ok(json!({
                "exit_code": output.status.code().unwrap_or(-1),
                "stdout": stdout,
                "stderr": stderr,
            }))
        }
        Err(e) => ToolOutput::err(format!("命令执行失败: {e}")),
    }
}

/// 平台 Shell 封装：
/// - Windows: powershell -NoProfile -Command，先切 UTF-8 输出编码（防 GBK 控制台乱码）
/// - Unix: bash -c；bash 不存在时（Alpine 等精简环境）回退 /bin/sh
fn run_shell(command: &str) -> std::io::Result<std::process::Output> {
    #[cfg(windows)]
    {
        let wrapped = format!("[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; {command}");
        Command::new("powershell")
            .args(["-NoProfile", "-Command", &wrapped])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    }
    #[cfg(not(windows))]
    {
        let attempt = Command::new("bash")
            .args(["-c", command])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();
        match attempt {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Command::new("/bin/sh")
                .args(["-c", command])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
            other => other,
        }
    }
}

kzm_tool!(
    ToolDecl {
        name: "run_command".into(),
        title: Some("执行 Shell 命令".into()),
        description: "执行系统 Shell 命令（Windows: PowerShell，Linux/macOS: bash）".into(),
        annotations: Some(ToolAnnotations::destructive()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "要执行的 Shell 命令"},
                "timeout": {"type": "integer", "description": "超时秒数，默认 30（由服务器统一兜底）", "default": 30}
            },
            "required": ["command"]
        }),
    },
    run
);
