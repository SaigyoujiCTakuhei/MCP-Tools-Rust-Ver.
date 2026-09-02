use serde_json::{json, Value};
use tool_kit::{kzm_tool, ToolAnnotations, ToolDecl, ToolOutput};
use std::process::{Command, Stdio};

fn run(args: Value) -> ToolOutput {
    let repo_path = args["repo_path"].as_str().unwrap_or(".");
    match Command::new("git")
        .args(["-C", repo_path, "pull"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(output) => ToolOutput::ok(json!({
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "exit_code": output.status.code().unwrap_or(-1),
        })),
        Err(e) => ToolOutput::err(format!("git pull 失败: {e}")),
    }
}

kzm_tool!(
    ToolDecl {
        name: "git_pull".into(),
        title: Some("拉取仓库更新".into()),
        description: "在指定目录执行 git pull 拉取最新代码".into(),
        annotations: Some(ToolAnnotations::destructive()),
        category: Some("Git".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "repo_path": {"type": "string", "description": "仓库本地路径，默认为当前目录", "default": "."}
            }
        }),
    },
    run
);
