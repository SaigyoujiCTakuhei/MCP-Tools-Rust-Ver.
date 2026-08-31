# KazeMiMiRin MCP Server (Rust)

> 基于 Rust 的 MCP 服务器，实现 **MCP 规范修订版 2026-07-28（社区俗称 "MCP 2.0"）**，
> 同时保留 **Legacy 2024-11-05 HTTP+SSE** 兼容通道（供 llama.cpp UI 等旧客户端使用）。
> 工具为**子进程插件**（改动 → 重编译 → 热重载，无需重启服务器），
> 另提供文件驱动的**提示词与资源**。内置 Web 管理面板。作者：风见血月

---

## 1. 项目概述

| 维度 | 说明 |
|------|------|
| 工具 | 30 个 = 23 通用（`tools/`）+ 7 域插件（`plugins/`：pdf_reader 2、sequential_thinking 1、memory 4），全部 `kzm-*` 子进程插件，启动自动发现；失败写 ERROR 日志 |
| 热重载 | 改工具源码 → `cargo build` → WebUI「⟳ 重载」即生效，不影响其他工具、不重启服务器 |
| 协议面 | 现代 2026-07-28（`POST /mcp`）+ Legacy 2024-11-05（`GET /sse` + `POST /message`） |
| 提示词/资源 | 文件驱动（`mcp_data/prompts`、`mcp_data/resources`），热重载并经协议列出（`prompts/list`、`resources/list`） |
| 输入校验 | JSON Schema（jsonschema crate，注册时预编译） |
| 鉴权 | 可选 Bearer Token（config / 环境变量） |
| WebUI | 工具卡片（单击筛选日志）、提示词/资源页签、重载按钮、断连横幅 |
| 结构 | Cargo workspace：`server`（服务器）+ `tool_kit`（插件契约）+ `tools`（23 个插件二进制） |

---

## 2. 目录结构

```
New_Architecture_v00/
├── Cargo.toml                 # workspace（members: server, tool_kit, tools）
├── config.yaml                # 运行时配置（端口/鉴权/Origin 白名单/插件目录/超时/mcp 数据目录）
├── mcp_data/
│   ├── prompts/               # 提示词（*.json 或带 YAML front matter 的 *.md，兼容 v11 Skills 格式）
│   └── resources/             # 资源（*.json；file 型资源每次读取最新内容）
├── server/                    # 服务器本体
│   └── src/
│       ├── main.rs            # 入口：配置→插件发现→路由→浏览器→优雅关闭
│       ├── config.rs
│       ├── executor/mod.rs    # ToolExecutor trait（子进程插件实现于 mcp/plugins.rs）
│       ├── registry/
│       │   ├── tool_definition.rs / tool_registry.rs
│       │   └── prompts.rs     # PromptRegistry / ResourceRegistry（文件驱动、热重载）
│       ├── mcp/
│       │   ├── handler.rs     # 协议核心（现代 + legacy 分发；工具/提示词/资源方法）
│       │   ├── transport.rs   # 双时代传输 + 订阅长流（tools/prompts/resources list_changed）
│       │   └── plugins.rs     # 插件发现/探测/执行/热重载
│       └── dashboard/         # WebUI（api.rs + html.rs）
├── tool_kit/                  # 插件契约：ToolDecl / ToolOutput / kzm_tool! 宏
├── tools/src/bin/             # 23 个通用工具插件（kzm-*.rs，一个工具一个二进制）
├── plugins/                   # 域插件目录（按功能一域一 crate，对应 v11 的 Lib/<模块>/）
│   ├── pdf_reader/            # v11 pdf_reader 移植：pdf_read_local / pdf_read_url
│   │   ├── src/pdf_utils.rs   # 公共函数（对应 v11 的 scripts/pdf_utils.py）
│   │   └── src/bin/           # kzm-pdf-read-local、kzm-pdf-read-url
│   ├── sequential_thinking/   # v11 sequential_thinking 移植：sequentialthinking（含状态）
│   │   ├── src/thinking_core.rs  # 纯 def 库（对应 scripts/thinking_core.py，含单元测试）
│   │   ├── src/config.rs         # 常量（对应 scripts/config.py）
│   │   └── src/bin/              # kzm-sequentialthinking（对应 scripts/tool_register.py）
│   └── memory/                # 长期记忆（PG + pgvector）：kzm-memory-{remember,recall,list,forget}
├── 长期记忆功能方案.md          # memory 设计方案（P1 已按决策落地）
```

> **新增域插件**：在 `plugins/<域名>/` 建一个 crate（members 加一行），每个工具一个
> `kzm-*` 二进制；依赖按域隔离（PDF 解析库只存在于 pdf_reader）。
>
> **侧车定义文件（define 单独成文件）**：在每个工具二进制旁放 `<bin名>.decl.json`
> （如 `kzm-pdf-read-local.decl.json`，完整 ToolDecl），即可覆盖内置定义——
> 改标题/描述/参数 schema **无需重编译**，服务器「⟳ 重载」后生效；删掉即恢复内置定义。

---

## 3. 插件工具工作流（核心特性）

工具 = 独立可执行文件（`target/debug/kzm-add` 等）。服务器与工具之间的契约：

```text
kzm-add decl   # stdout 输出 ToolDecl JSON（name/title/description/annotations/inputSchema）
kzm-add call   # stdin 读 JSON 参数 → stdout 输出 ToolOutput JSON（success/data/error）
```

日常循环（**不重启服务器**）：

```bash
# 1. 改动任意工具源码，例如 tools/src/bin/hello_world.rs
# 2. 增量编译（只重编改动项，约 3 秒）
cargo build --bin kzm-hello-world
# 3. WebUI 点「⟳ 重载」，或：
curl -X POST http://127.0.0.1:58081/api/tools/hello_world/reload
# 4. 下一次调用即使用新代码；其他工具不受任何影响
```

- **加载失败可见**：启动发现或重载时若工具无法唤起（二进制缺失/崩溃/输出非法），
  一律写 ERROR 日志（WebUI 实时可见），且不阻断服务器启动。
- **新增工具**：在 `tools/src/bin/`（或 `plugins/<域>/src/bin/`）下用 `tool_kit::kzm_tool!`
  宏写一个新二进制 → `cargo build` → WebUI 点「🔍 扫描新插件」或
  `POST /api/tools/rescan` → 新工具立即登记并推送 `tools/list_changed`，全程无需重启。
- 单次调用超时（`tools.default_timeout`，默认 30 秒）到点会连同子进程一起终止。

---

## 4. 提示词与资源（MCP prompts / resources）

文件驱动，改文件 + 重载即生效；协议层经 `prompts/list`、`prompts/get`、
`resources/list`、`resources/read` 列出与读取（现代与 legacy 通道均支持）。

**提示词**（`mcp_data/prompts/`，两种格式）：

```json
{
  "name": "code-review",
  "description": "按规范评审代码",
  "arguments": [{"name": "code", "description": "待评审代码", "required": true}],
  "template": "请评审：{{code}}"
}
```

或 Markdown + YAML front matter（**兼容 v11 的 Anthropic Skills 格式**，`params` 即参数声明，
正文为模板）——把 v11 `Lib/prompts/skills/*.md` 复制进来即可使用。

**资源**（`mcp_data/resources/*.json`）：`text` 内联内容或 `file` 相对路径
（file 型资源每次 `resources/read` 都读磁盘最新内容，改文件即热更新）。

重载入口：`POST /api/prompts/reload`、`POST /api/resources/reload`（WebUI 页签内按钮），
重载后按订阅过滤器推送 `notifications/prompts/list_changed`、`notifications/resources/list_changed`。

---

## 5. HTTP 路由表

| Method | Path | 说明 |
|--------|------|------|
| `POST` | `/mcp` | 现代 MCP 端点（2026-07-28，tools + prompts + resources） |
| `GET` | `/mcp` | 已移除的流端点 → 405 |
| `GET` | `/sse` | Legacy SSE 长流（首条 `endpoint` 事件 + 变更通知） |
| `POST` | `/message?sessionId=xxx` | Legacy JSON-RPC（initialize 握手） |
| `GET` | `/` | WebUI 管理面板 |
| `GET/POST` | `/api/tools*`、`/api/prompts*`、`/api/resources*`、`/api/logs*` | Dashboard 私有 API |

WebUI：左侧「工具 / 提示词 / 资源」三页签；工具卡片**单击 = 日志按该工具筛选，再点/点筛选条取消**；
右上横幅在日志流断开（服务器关闭）时显示「服务器已断开」。

---

## 6. 现代 MCP 端点（2026-07-28）要点

每个 `POST /mcp` 必须携带：头 `MCP-Protocol-Version`（= `2026-07-28`，须与 `_meta` 一致）、
`Mcp-Method`（= body method；`tools/call` 另需 `Mcp-Name` = `params.name`）；
body `params._meta` 必含 `io.modelcontextprotocol/protocolVersion` 与
`io.modelcontextprotocol/clientCapabilities`。服务端强制校验 Origin（回环/白名单）、
头体一致性（-32020）、版本（-32022）、必需 `_meta`（-32602）；通知 → 202；未知方法 → 404。

```bash
META='"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}'
curl -X POST http://127.0.0.1:58081/mcp \
  -H 'Content-Type: application/json' \
  -H 'MCP-Protocol-Version: 2026-07-28' -H 'Mcp-Method: prompts/list' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"prompts/list\",\"params\":{$META}}"
```

## 7. Legacy 通道（2024-11-05，llama.cpp UI）

`GET /sse` → `event: endpoint`（`data: /message?sessionId=xxx`）→ `POST /message` 走
initialize 握手；`prompts/list`、`resources/list` 等方法同样可用。llama.cpp UI 的来源
加入 `allowed_origins` 以放行浏览器直连。

## 8. 安全模型

| 层 | 机制 |
|----|------|
| 网络 | 默认绑定 127.0.0.1；Origin 回环/白名单校验（防 DNS rebinding） |
| 鉴权 | 可选 Bearer Token（`MCP_AUTH_TOKEN` > `config.server.auth_token`），覆盖 MCP 端点 |
| 输入 | 全部工具入参按 inputSchema 做 JSON Schema 校验（失败 → isError:true） |
| 执行 | 单次调用超时（连子进程终止）；工具可热禁用；插件崩溃只影响单次调用 |
| CORS | 白名单 = 回环同端口来源 + `allowed_origins` |

> ⚠️ `run_command` 未做命令级限制（既定取舍）；暴露到非本机环境请启用 token 并自行收紧。

## 9. 构建与运行

```bash
cargo build                # 全 workspace（server + tool_kit + 全部插件二进制）
cargo build --bin kzm-add  # 只编译单个工具插件（热重载日常用）
cargo server               # 运行服务器（= cargo run -p kazemimirin-mcp-server 的本地别名）
cargo test                 # 单元测试
```

> workspace 共 33 个二进制，裸 `cargo run` 无法确定目标；`cargo server` 别名
> 定义在 `.cargo/config.toml`，等价写法：`cargo run -p kazemimirin-mcp-server`。

要求 Rust ≥ 1.85（edition 2024）。Windows / Linux 双端无系统库依赖（reqwest 用 rustls）。
config.yaml 查找顺序：exe 同目录 → 工作目录。关闭：Ctrl+C / SIGTERM 触发优雅排水，
keep-alive 长连接最多拖延 10 秒后强制退出。

## 10. 与 v11 (Playground) 的对应

| v11（Python, 147 工具 + 35 提示词） | Rust 版 |
|---|---|
| 基础通用工具（Lib/tools/scripts 同名项） | ✅ 23 个插件，全部热重载 |
| 文件夹自动发现工具 | ✅ kzm-* 自动发现 + 重载 + 手动扫描新插件 |
| 提示词（Anthropic Skills .md） | ✅ 协议 + 文件驱动，.md 直接兼容 |
| 资源（ResourceRegistry） | ✅ 协议 + 文件驱动 |
| 工具加载失败可见 | ✅ ERROR 日志 |
| pdf_reader（PyPDF2：pdf_read_local / pdf_read_url） | ✅ `plugins/pdf_reader/`（pdf-extract 纯 Rust），首个按功能分目录的域插件 |
| sequential_thinking（thread_local 状态） | ✅ `plugins/sequential_thinking/`；状态改为显式 `sessionId` 句柄 + `mcp_data/sequential_thinking/` 持久化（MCP Stateful Tools 规范模式），thinking_core 纯库含 4 个单元测试 |
| memory（长期记忆，Markdown+ChromaDB） | ✅ `plugins/memory/` P1：`memory_remember/recall/list/forget`，复用 dsh 记忆插件的 `memory_chunks` 表（PG 17 + pgvector HNSW，现 188 条存量），本地 bge-small-zh-v1.5 嵌入（CLS 池化，与存量向量完全兼容，同文重嵌入 score=1.0）；P2/P3（api 嵌入、混合检索、自动提炼）见方案文档 |
| ai_bridge / netease / fanqie | ⏳ 后续按需以 `plugins/<域>/` 插件形式移植 |
| python_eval / evolution / create_tool 等 Python 机制 | ➖ 不移植（Rust 编译期注册已替代其框架职责） |
