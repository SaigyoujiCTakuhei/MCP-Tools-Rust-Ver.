use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};
use std::process::{Command, Stdio};

fn run(args: Value) -> ToolOutput {
    let Some(repo_url) = args["repo_url"].as_str() else {
        return ToolOutput::err("缺少 repo_url 参数");
    };
    let mut cmd = Command::new("git");
    cmd.args(["clone", repo_url]);
    if let Some(dir) = args["target_dir"].as_str() {
        cmd.arg(dir);
    }
    match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output() {
        Ok(output) => ToolOutput::ok(json!({
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "exit_code": output.status.code().unwrap_or(-1),
        })),
        Err(e) => ToolOutput::err(format!("git clone 失败: {e}")),
    }
}

kzm_tool!(
    ToolDecl {
        name: "git_clone".into(),
        title: Some("克隆仓库".into()),
        description: "克隆 Git 仓库到本地".into(),
        annotations: Some(ToolAnnotations::writes()),
        category: Some("Git".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "repo_url": {"type": "string", "description": "仓库 URL"},
                "target_dir": {"type": "string", "description": "目标目录（可选，默认使用仓库名）"}
            },
            "required": ["repo_url"]
        }),
    },
    run
);
