# KazeMiMiRin MCP Server — MCP 2.0 合规性 & Windows/Linux 兼容性审查报告

> 审查日期：2026-08-30
> 审查对象：`New_Architecture_v00`（kazemimirin-mcp-server 0.1.0，Rust）
> 对照标准：MCP 规范修订版 **2026-07-28**（社区俗称 "MCP 2.0" / 下一代 MCP）
> 方法：`cargo check` 实测 + 全量源码阅读 + 逐条对照官方规范（basic / versioning / transports/streamable-http / server/discover / server/tools / patterns/subscriptions）

---

## 0. 结论速览

1. **项目当前无法编译**：`cargo check` 报 18 个错误，全部来自 2026-08-28 对 `src/mcp/handler.rs` 与 `src/mcp/sse.rs` 的 "MCP 2.0" 重写；即使修好编译，`main.rs` 合并路由时 `/mcp` POST 路由重复，axum 启动即 panic。
2. **协议方向正确、细节大量错位**：2026-07-28 确实取消了 `initialize` 握手、引入每请求 `_meta`、强制 `server/discover`、`resultType`、`subscriptions/listen`——`技术规格文档.md` 的迁移清单方向是对的；但实现里 `_meta` 键名、版本协商错误码、响应信封、HTTP 状态码、订阅机制形状等关键细节与规范不符，标准客户端无法与之互通。
3. **双端兼容性总体良好**（reqwest 用 rustls、tokio/fs 全跨平台抽象），但有 5 个实际问题：Windows PowerShell 输出编码（GBK 乱码）、glob 模式反斜杠拼接、超时机制完全未实现、非 UTF-8 文件读取失败、config.yaml 相对路径依赖 CWD。

---

## 1. "MCP 2.0" 是什么（对照基准）

- 官方名称：**MCP 规范修订版 2026-07-28**（2026-07-28 发布），配套 TS/Python/Go/C# SDK 同步升级到 v2.0 世代；Rust 官方 SDK（rmcp）目前仍是 legacy 时代实现，**尚无 2026-07-28 版本**。
- 核心变化：
  - **无握手、无会话**：删除 `initialize`/`notifications/initialized`，删除 `Mcp-Session-Id` 会话模型；每个请求自带 `_meta`（`io.modelcontextprotocol/protocolVersion`、`io.modelcontextprotocol/clientCapabilities` 必填，`clientInfo` 建议带）。
  - **版本协商**：不支持的版本必须返回 `-32022` `UnsupportedProtocolVersionError`（`data.supported` 列出支持的版本），HTTP 400；`server/discover` 为服务器 **MUST** 实现。
  - **Streamable HTTP 重定义**：单一 MCP 端点仅 POST；**GET 流端点被移除**（GET → 405）；服务端可回 `application/json` 或请求级 `text/event-stream`；新增强制头 `MCP-Protocol-Version`、`Mcp-Method`、`Mcp-Name`，头/体不一致 → 400 `-32020`；通知 POST → 202 无 body；**必须校验 Origin**（DNS rebinding，非法 → 403）。
  - **MRTR（多轮请求）**：服务端不再主动发请求，sampling/elicitation 改为在 result 里带 `resultType: "input_required"` + `inputRequests`。
  - **订阅**：唯一入口是 `subscriptions/listen`（POST，响应即长连 SSE 流），过滤参数 `{toolsListChanged, promptsListChanged, resourcesListChanged, resourceSubscriptions}`；流上第一条必须是 `notifications/subscriptions/acknowledged`；`subscriptionId` = listen 请求的 JSON-RPC id；取消 = 关闭流。
  - **错误码分区**：`-32020` HeaderMismatch / `-32021` MissingRequiredClientCapability / `-32022` UnsupportedProtocolVersion；`-32000~-32019` 为 legacy 区，新实现 **SHOULD NOT** 使用。
  - **结果必须带 `resultType`**（响应 result 对象必须有该字段）；响应必须回显请求 id；result 与 error 互斥（JSON-RPC 规则）。
  - 工具执行错误 → result `isError: true`；未知工具 → `-32602`。

---

## 2. 问题清单与解决方案（按优先级）

### P0 — 阻断级（不修则一切免谈）

**P0-1 项目无法编译（18 个错误）**
- 位置：`src/mcp/handler.rs`、`src/mcp/sse.rs`（2026-08-28 重写引入）
- 典型错误：
  - `sse.rs:74` 调用不存在的 `handle_subscriptions_listen`（E0425）
  - `sse.rs:34-35` 从 `dashboard::api` 私有导入 `AppState`、从 `handler` 导入私有函数 `handle_mcp_request`（E0603）
  - `sse.rs:158` 在普通 async 块里用 `yield`（E0658/E0727，async coroutines 未稳定）
  - `sse.rs:148-170` SSE 构造错误（async block 未实现 `Stream`、`Response::new` 类型不匹配）
  - `sse.rs:46` `AllowMethods/AllowHeaders` 不能从 `Vec<HeaderValue>` 构造
  - `handler.rs:371` `&str`/`String` 类型不匹配；`429/592` 对非 async 函数用 `.await`；`551` 对 `Response` 做 `serde_json::from_str`；`303/568` `Option<&Value>`/`Value` 混用
- 方案：以 `handler.rs` 为唯一协议实现重写 `sse.rs`（或反过来，二选一），统一 `AppState`（只用 `mcp::handler::AppState`）、把 `handle_mcp_request` 设为 `pub`、SSE 流改用 `async_stream::stream!` 宏或 `axum::response::sse::Sse` + 正确的 Stream 实现；`error_response` 参数统一 `String`；删除对 Response 的二次 JSON 解析（直接在构造处组装 JSON）。两个被重写文件实际上是两套互相矛盾的半成品，建议按本报告 P1/P2 项一次性重写为一个模块。

**P0-2 路由重复，启动即 panic**
- 位置：`main.rs:65-71` 同时 merge `sse::build_streamable_router`（注册 `POST /mcp`）与 `handler::build_mcp_router`（也注册 `POST /mcp`）
- axum 对重复路由在 Router 构建时 panic。
- 方案：只保留一个 `/mcp` 挂载点（推荐保留 sse.rs 的 `GET+POST /mcp` 并在其中分发），删除 `handler::build_mcp_router` 中重复的 `/mcp` 注册（`build_subscriptions_router` 目前未挂载，其 `/subscriptions` 与 sse.rs 的 `/subscriptions` 也构成潜在冲突，一并清理）。

**P0-3 测试模块编译失败**
- 位置：`sse.rs:173-193` `#[cfg(test)]` 引用不存在的 `Config::default()`、在非 async 测试里 `.await`、调用签名错误。
- 方案：删除或重写为针对 `handler::handle_request` 的集成测试（构造 `AppState::new(...)`）。

### P1 — 协议硬伤（标准客户端无法互通）

**P1-1 请求 `_meta` 字段名错误，且不拒绝缺失必需字段的请求**
- 位置：`handler.rs:63-96`（`RequestMeta::parse`）
- 现状：读 `params._meta.protocolVersion / clientCapabilities / clientInfo`（裸键名，还接受 `params.meta` 回退）。
- 规范：必需键为 `params._meta["io.modelcontextprotocol/protocolVersion"]` 与 `["io.modelcontextprotocol/clientCapabilities"]`（`clientInfo` 建议带）；**缺失必需字段 = 恶意请求，必须返回 JSON-RPC `-32602` + HTTP 400**。
- 方案：解析改用带前缀键；启动校验，缺失即 `error_response(-32602)` 并将 HTTP 状态设为 400。注意 `handler.rs` 中已有的 `ResponseMeta` 结构体（serde rename 为带前缀键）反而是对的——统一以它为准。

**P1-2 版本协商错误：静默回退而非 `-32022` 拒绝**
- 位置：`handler.rs:88-95`（`negotiate_version`）
- 现状：客户端请求不支持的版本时静默按 2026-07-28 处理；`SUPPORTED_PROTOCOL_VERSIONS` 还声明支持 `2025-11-25`（legacy 握手时代版本，实际并未实现 legacy 行为）。
- 规范：不支持 → 必须 `-32022`，`data: {supported: [...], requested: ...}`，HTTP 400。
- 方案：`supportedVersions` 只报 `["2026-07-28"]`（除非真的实现双时代）；协商失败返回 `-32022`；不支持的版本一律拒绝而非回退。

**P1-3 响应信封错误：id 不回显、result/error 并存、`_meta` 位置与键名错**
- 位置：`handler.rs:284-308`（`build_json_with_meta`）、`337-358`（tools/list 硬编码 `id: Null`）、`490-506`
- 现状：成功响应为 `{jsonrpc, id, result, error: null, _meta}` —— ① 请求 id 被丢弃（tools/list、server/discover 恒 `id: null`），违反"响应必须包含与请求相同的 id"；② `error: null` 与 `result` 并存违反 JSON-RPC（二者互斥）；③ `_meta` 挂在 JSON-RPC 顶层而规范要求放在 `result._meta` 内，且键名未加 `io.modelcontextprotocol/` 前缀（同文件内 `ResponseMeta` 用了前缀，`build_json_with_meta` 却用裸键，自相矛盾）；④ 回显 `clientCapabilities/clientInfo` 无必要。
- 方案：重写响应组装：`{jsonrpc: "2.0", id: <回显请求 id>, result: { resultType: "complete", ...业务字段, _meta: { "io.modelcontextprotocol/serverInfo": {...} } }}`；错误响应只含 `error`，无 `result`。

**P1-4 结果缺 `resultType`**
- 位置：tools/list（`handler.rs:337`）与 server/discover（`handler.rs:310`）的 result。
- 规范：所有 result **MUST** 含 `resultType`（`"complete"`；`"input_required"` 为 MRTR 场景，本项目可暂不支持但字段要恒在）。
- 方案：每个成功 result 统一注入 `"resultType": "complete"`（tools/call 已加，其余补齐）。

**P1-5 `server/discover` 响应缺 `supportedVersions`，且虚报未实现的 capability**
- 位置：`handler.rs:310-335`
- 现状：result 只含 `capabilities` + `serverInfo`（serverInfo 还放错位置）；capabilities 声明 `resources: {}`、`prompts: {}` 但服务器没有实现任何 resources/prompts 方法（请求会得到 -32601）——声明了 capability 就必须能响应对应请求。
- 规范：`DiscoverResult = { resultType, supportedVersions: [...], capabilities, _meta: {"io.modelcontextprotocol/serverInfo": {...}}, instructions? }`。
- 方案：补 `supportedVersions: ["2026-07-28"]`；serverInfo 移入 `result._meta`；capabilities 只保留 `{"tools": {"listChanged": <真实值>}}`；可加 `instructions`。

**P1-6 HTTP 状态码全线错误**
- 位置：`handler.rs` 所有错误响应、`sse.rs` 各端点
- 现状：一切错误都返回 HTTP 200 + JSON body。
- 规范（MCP 端点）：`_meta` 缺失/版本不支持/头体不一致 → **400**；未知方法 → **404**（body 为 -32601）；Origin 非法 → **403**；通知 POST 成功 → **202 无 body**；正常请求 → 200（JSON 或 SSE）。
- 方案：让 `handle_request` 返回 `(StatusCode, Json)`，按上述映射设置状态码；通知（无 id 的消息）不再生成 JSON-RPC 响应而直接 202。

**P1-7 客户端通知（无 id 消息）被当作请求处理**
- 位置：`handler.rs:243-267`（对无 id 消息仍回 JSON-RPC 响应/错误）。
- 规范：通知必须不产生响应；HTTP 上成功即 202。
- 方案：`req.id == None` 时执行副作用（如需要）后直接返回 202 空 body。

### P2 — 协议规范 SHOULD/错误码与安全（影响互操作质量与安全）

**P2-1 错误码体系不符**
- 位置：`handler.rs:371-421`
- 现状：未知工具 → `-32601`；工具执行失败 → JSON-RPC `-32000`；工具被禁用 → `-32000`。
- 规范：未知工具 → **`-32602`**；工具执行错误（含参数校验失败等模型可自纠的错误）→ **result `isError: true`**（让 LLM 能拿到错误文本自纠正，而不是协议层报错）；`-32000~-32019` 是 legacy 区，新实现 SHOULD NOT 使用。
- 方案：未知工具/缺 name → `-32602`；执行失败与"已禁用"改为 `result: {resultType: "complete", content: [{type:"text", text: <错误信息>}], isError: true}`。

**P2-2 订阅机制为自造协议，需按 `subscriptions/listen` 重做**
- 位置：`handler.rs:157-180, 425-488, 516-519, 580-625`；`sse.rs:40-171`
- 现状：自造 `subscriptions/subscribe`、`subscriptions/unsubscribe`、`notifications/subscribe_response/unsubscribe_response`，服务器生成 UUID 作 subscriptionId，另设 `/subscriptions` HTTP 路由；GET `/subscriptions` 的 SSE 流推送的是 dashboard 原始日志（非 JSON-RPC 通知、无 acknowledged、无 subscriptionId，且 30 秒后定时器一触发流就断）。GET `/mcp?method=subscriptions/listen` 也非规范形状（2026-07-28 已删除 GET 流端点，GET /mcp 应一律 405；当前实现因 `Query<String>` 提取失败实际返回 400）。
- 规范：唯一机制是 **POST `subscriptions/listen`**，`params.notifications` 为过滤器 `{toolsListChanged?, promptsListChanged?, resourcesListChanged?, resourceSubscriptions?}`；响应即长连 SSE 流，**第一条必须是 `notifications/subscriptions/acknowledged`**，`_meta["io.modelcontextprotocol/subscriptionId"]` = listen 请求的 JSON-RPC id；后续每条通知都带该 `_meta`；取消 = 客户端关闭流；优雅关闭时服务器先发最终响应（`resultType: "complete"` + subscriptionId `_meta`）再关流。
- 方案：删除 subscribe/unsubscribe/subscribe_response 全套与 `/subscriptions` 路由；新增 `subscriptions/listen` 处理器：校验过滤器 → 返回 SSE 流（先发 acknowledged）→ 把 `ToolRegistry` 的 toggle 事件映射为 `notifications/tools/list_changed` 推流；SSE 注释行做 keep-alive，并加 `X-Accel-Buffering: no` 头。

**P2-3 声明了 `listChanged: true` 却从不发送 `notifications/tools/list_changed`**
- 位置：capabilities 声明在 `handler.rs:315-319`；`ToolRegistry.notify_tx` 存在但 `notify_rx()` 从未被调用（README 亦列为其死代码警告）。
- 方案：与 P2-2 一并修复——把注册表 toggle 事件接到 listen 流；若暂不实现，则 capabilities 改为 `{"tools": {}}`（不要声明做不到的行为）。

**P2-4 缺少 MUST 级传输校验：Origin / 协议头 / 头体一致性**
- 位置：`sse.rs`、`main.rs:60-63`（CORS 全开 `Any`）
- 规范：**必须**校验 `Origin`（非法 → 403，防 DNS rebinding）；**必须**校验 `MCP-Protocol-Version` 头存在且与 body `_meta` 一致（不一致 → 400 `-32020` HeaderMismatch）；`Mcp-Method`（所有请求）、`Mcp-Name`（tools/call）头与 body 值一致性校验 → `-32020`；本地运行绑定 127.0.0.1（已满足）。
- 方案：在 `/mcp` POST handler 前加校验层（中间件或 handler 内前置检查）；CORS 从 `Any` 收紧为固定 Origin 列表（WebUI 自身来源）；忽略（不铸造）`Mcp-Session-Id`、`Last-Event-ID` 头（无状态要求，当前已天然满足）。

**P2-5 无任何鉴权 + 高危工具无防护**
- 位置：全局；`tools/shell.rs`（`run_command` 任意命令）、`tools/file_ops.rs`（`delete_file`）、`tools/git.rs`、`tools/net.rs`（`download_file` 可写任意路径）
- 规范：HTTP 传输 SHOULD 遵循 MCP 授权框架；服务器 **MUST**：校验输入、访问控制、限流、输出净化。
- 方案：最低限度加 Bearer Token 校验（读环境变量/config，Dashboard 与 MCP 端点分别可配）；`run_command` 增加命令白名单/黑名单或专用确认开关；文件/下载工具限制根目录（workspace 沙箱）；工具级简单限流（如每秒 N 次）。README 已把"工具权限沙箱"列 P3，建议提级。

**P2-6 工具输入未按 inputSchema 校验；超时配置不生效**
- 位置：`handler.rs:360-422`（直接 `args["x"]` 取值）；`config.rs` `default_timeout` 无人消费；`shell.rs:26` `_timeout_secs` 弃用。
- 规范：MUST 校验工具输入。
- 方案：用 `jsonschema` crate 对 arguments 做 inputSchema 校验（失败 → isError 结果或 -32602）；把 `default_timeout` 接入执行器（`tokio::time::timeout` 包裹 execute），`run_command` 的 `timeout` 参数真正生效（见 P3-W2）。

**P2-7 tools/list 缺分页与非确定性顺序**
- 位置：`handler.rs:337-358`（不读 `params.cursor`）；`ToolRegistry::list` 基于 DashMap 迭代（顺序随机）。
- 规范：支持 cursor 分页（至少接受并忽略 cursor、不返回 nextCursor 也可）；顺序 SHOULD 确定性（利于客户端缓存与 prompt cache 命中）。
- 方案：`list()` 后按 name 排序；实现简单 cursor（offset 或 name 起始页），23 个工具一页装得下，但接口要能收 `cursor` 参数。

### P3 — 质量 / 结构 / 文档

**P3-1 工具元数据不完整**：`to_mcp_tool_json()` 只输出 name/description/inputSchema；`ToolDefinition` 已有 `title`/`annotations` 字段但从不赋值、不序列化。方案：`register()` 接受 title/annotations（`read_only_hint`、`destructive_hint` 等对宿主 UI 很有用），tools/list 输出它们；可为工具补 `outputSchema` + `structuredContent`（结果目前把内部信封 `{success, data}` pretty-print 成唯一 text content，建议 content 只放数据、加 `structuredContent` 字段）。

**P3-2 `initialize` 拒绝方式可改进**：现代-only 服务器收到 `initialize` 应返回 JSON-RPC 错误并在 message/data 中列出支持的版本（legacy 客户端唯一的诊断来源）。当前返回通用 -32601，建议错误 data 带 `supported`。

**P3-3 死依赖与重复代码**：`rmcp`（注释"占位"）、`tokio-stream`、`thiserror`、`http`、`bytes` 均未使用 → 从 Cargo.toml 移除（rmcp 0.4 是 legacy 实现，等其支持 2026-07-28 再考虑启用）；`ToolResult` 在 `executor/mod.rs` 与 `mcp/handler.rs` 重复定义 → 合并为一个；`handler.rs` 同时存在 `handle_request` 与私有 `handle_mcp_request` 双入口 → 合一。

**P3-4 `block_in_place + block_on` 反模式**（`handler.rs:387-389`）：工具本来就是 async，直接 `entry.executor.execute(args).await`；当前写法在 current_thread runtime 下会 panic，且每次调用占死一个 worker 线程。

**P3-5 日志静默丢失**：`handler.rs:383/397/413/419` 四处 `state.logs.log(...)` 漏 `.await`（async fn 返回的 future 被直接丢弃）→ MCP 侧日志永远不进缓冲区/推送。补 `.await`。

**P3-6 配置失效**：`config.yaml` 的 `logging.level/format` 被忽略（main.rs 硬编码 `pretty` 层 + 默认 `info` filter）。方案：用配置值构造 `EnvFilter` 与 fmt 层。

**P3-7 文档过期**：README 仍描述 SSE transport（`GET /sse`/`POST /message`）、`protocolVersion 2024-11-05`、handler.rs 231 行等，与代码严重不符；`技术规格文档.md` 中"现状"列也过时。方案：随本次修复一并更新（本报告可直接作为素材）。

---

## 3. Windows / Linux 双端兼容性专项

总体：Rust 侧技术选型（tokio + reqwest(rustls) + fs-err/tokio::fs + webbrowser + hostname）跨平台良好，无 OpenSSL 系统依赖，edition 2024 需 Rust ≥ 1.85（两平台一致）。以下为实际发现：

**W1（重要）PowerShell 输出编码乱码（Windows）**
- `shell.rs:45` 用 `String::from_utf8_lossy` 解码命令输出；Windows PowerShell 5.1 默认按控制台代码页输出（zh-CN 为 GBK/CP936），中文输出必乱码。
- 方案：Windows 分支改为 `powershell -NoProfile -Command "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; <command>"`（或检测 `pwsh` 优先使用）；或按系统代码页手动解码。

**W2（重要）超时机制未实现（双平台）**
- `shell.rs:26` `_timeout_secs` 弃用、`.output()` 无限等待，`kill_on_drop(true)` 对 `.output()` 无意义；`config.default_timeout` 无人消费；`net.rs`/`git.rs` 的网络与 git 命令同样无超时（`download_file` 用裸 `reqwest::get`，无超时且全量读内存）。
- 方案：统一用 `tokio::time::timeout(duration, child.wait_with_output())` 包裹，超时 `child.kill().await`；`download_file` 改 `Client::builder().timeout(...)` + 流式写入文件（Cargo 已启用 `stream` feature，未用上）。

**W3（中）glob 模式用字符串拼接路径，Windows 反斜杠有风险**
- `search.rs:82` `format!("{}/{}", path, pattern)`、`search.rs:127` `format!("{}/**/*", directory)`：若 Windows 客户端传 `C:\data` 这类反斜杠路径，`\` 在 glob 模式语法中兼作转义符/分隔符，不同版本行为不一，易匹配失败。
- 方案：路径统一规范化：`Path::new(path).join(pattern)` 后 `to_slash_lossy()`（或 `replace('\\', "/")`) 再进 glob；`complex_search` 同理。

**W4（中）非 UTF-8 文件读取直接失败（双平台，Windows 更常见）**
- `file_ops.rs:31`(read_file)、`search.rs:33`(grep)、`search.rs:144`(complex_search 跳过)、`metrics.rs:26`(count_lines) 均用 `read_to_string`：GBK/GBK18030/Latin-1 文件报错或被当作二进制跳过。
- 方案：改 `read()` + `String::from_utf8_lossy`（对 grep/count 明确无损）；需要严格编码语义的工具在错误信息中注明"非 UTF-8"。

**W5（中）config.yaml 相对路径依赖进程 CWD（双平台，Windows 服务场景易踩）**
- `main.rs:34` `PathBuf::from("config.yaml")`：从快捷方式/计划任务/服务启动时 CWD 不是 exe 目录 → 静默回退默认配置（端口恰好同值不易察觉，`auto_open_browser` 默认 false 等差异会被吞掉）。
- 方案：优先 `std::env::current_exe()?.parent()/config.yaml`，其次 CWD，最后默认；回退时 warn 已有，建议日志中打印实际采用的路径。

**W6（低）Linux 缺 SIGTERM 优雅退出**
- `main.rs:103-106` 只处理 `ctrl_c`；systemd/docker stop 发 SIGTERM 时无法优雅关闭。
- 方案：`#[cfg(unix)]` 分支监听 `tokio::signal::unix::signal(SignalKind::terminate())`，与 ctrl_c `tokio::select!`。

**W7（低）bash 不可用环境（Linux 精简容器）**
- `shell.rs:32` 非 Windows 恒用 `bash`；Alpine 等只有 `sh`。
- 方案：探测 `bash` 不存在时回退 `/bin/sh -c`（或在配置中允许指定 shell）。

**W8（低）delete_file 在 Windows 的已占用/只读文件**
- `remove_file` 对被占用文件（ERROR_SHARING_VIOLATION）与只读文件失败。方案：只读文件先清 `permissions.readonly()` 再删；错误信息保持原样透出即可。

**W9（低）阻塞调用混入 async**
- `main.rs:95` `webbrowser::open`（阻塞）在 tokio::spawn 中未用 `spawn_blocking`；`complex_search` 的 `glob::glob` 同步迭代直接在 async 上下文执行，大目录会卡住 worker。方案：分别改 `spawn_blocking`。

**W10（化妆级）** 控制台日志含 emoji（🚀/📋/✓/✗），老式 Windows 控制台（CP936，非 Windows Terminal）下乱码；`tracing` 输出本身不受影响。可在 Windows 上降级为纯 ASCII 前缀。

其余检查项均通过：`hostname`/`chrono`/`glob`/`fs-err` 双端正常；`cfg!(windows)` shell 分支方向正确；路径参数接受绝对/相对路径，Windows 盘符与正反斜杠 Rust std 均可处理；`webbrowser` 无显示环境时已有 warn 兜底；`output.status.code().unwrap_or(-1)` 在 Unix 信号终止时的 -1 语义可接受。

---

## 4. 建议修复路线

1. **第一步（P0）**：合并 handler.rs / sse.rs 为单一协议模块，消除 18 个编译错误与重复路由，恢复可构建。
2. **第二步（P1）**：按规范重写请求解析与响应组装——`io.modelcontextprotocol/*` 键、id 回显、result/error 互斥、resultType、server/discover 补 supportedVersions、HTTP 状态码映射（400/403/404/202/200）。
3. **第三步（P2）**：错误码归位（-32602 / isError）、`subscriptions/listen` 长流 + acknowledged + list_changed、Origin/协议头校验、最小鉴权、超时与输入校验。
4. **第四步（P3）**：结构化内容/注解/分页/排序、平台项 W1-W6、清理死依赖、更新 README 与技术规格文档。
5. **中期**：跟踪官方 Rust SDK（rmcp）对 2026-07-28 的支持；一旦可用，评估以 SDK 替换手写协议层（当前 rmcp 0.4 仅支持 legacy 时代协议，暂不可用）。

---

## 附：主要依据

- 官方规范总览：https://modelcontextprotocol.io/specification/2026-07-28
- Versioning（无握手、-32022、双时代矩阵）：https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning
- Streamable HTTP（GET 移除、强制头、校验、405/400/403/202）：https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http
- server/discover（DiscoverResult 形状）：https://modelcontextprotocol.io/specification/2026-07-28/server/discover
- Tools（resultType/isError/-32602/分页/listChanged）：https://modelcontextprotocol.io/specification/2026-07-28/server/tools
- Subscriptions（listen/acknowledged/subscriptionId）：https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/subscriptions
- 官方发布博客：https://blog.modelcontextprotocol.io/posts/2026-07-28/

---

## 附：修复记录（2026-08-30 修复轮次）

### 已修复并通过实测

**P0 全部**：
- 重写 `src/mcp/handler.rs`（协议核心层）与 `src/mcp/sse.rs`（Streamable HTTP 传输层），消除全部 18 个编译错误；删除自造的 `SubscriptionManager`、`build_mcp_router`/`build_subscriptions_router` 双路由与 `/subscriptions` 端点
- `/mcp` 现仅由 `sse::build_streamable_router` 注册一次（POST + GET→405），修复 axum 重复路由启动 panic
- 删除 `sse.rs` 中编不过的测试模块，替换为 3 个可运行单元测试（Origin 校验、值解码、_meta 解析）——`cargo test` 3/3 通过

**P1 全部**：
- `_meta` 改用 `io.modelcontextprotocol/protocolVersion|clientCapabilities`（必需）与 `clientInfo`（可选）前缀键解析；缺失 → `-32602` + HTTP 400
- 版本协商：`supportedVersions` 仅报 `["2026-07-28"]`（不再虚报 2025-11-25）；不支持版本 → `-32022` + `data:{supported,requested}` + HTTP 400；头/体版本不一致 → `-32020` + 400
- 响应信封：id 回显、result/error 互斥（成功响应不再有 `error: null`）、`_meta` 移入 `result._meta` 且 serverInfo 带规范前缀、所有 result 恒带 `resultType:"complete"`
- `server/discover` 补齐 `supportedVersions`，capabilities 只声明 `tools.listChanged`（不再虚报 resources/prompts）；`initialize` 返回 404 + 带支持版本列表的提示
- HTTP 状态码映射：校验失败 400 / Origin 非法 403 / 未知方法 404 / 通知 202 无 body / 正常 200；`GET /mcp` 一律 405（带 `Allow: POST`）
- 通知（无 id 消息）不再生成 JSON-RPC 响应；`id: null` 拒绝（-32600 + 400）

**P2 部分**：
- 错误码归位：未知工具/已禁用工具 → `-32602`；工具执行错误 → `result.isError: true`；彻底停用 -32000 区
- 订阅机制按规范重做：唯一入口 `subscriptions/listen`（POST → SSE 长流），首条消息 `notifications/subscriptions/acknowledged`（`subscriptionId` = listen 请求 id），工具热插拔推送 `notifications/tools/list_changed`（实测：ack + 2 条变更通知全部到达）
- 传输 MUST 校验：Origin 回环校验（防 DNS rebinding）、`MCP-Protocol-Version`/`Mcp-Method`/`Mcp-Name`（含 `=?base64?...?=` 哨兵解码）头体一致性校验
- 确定性输出：tools/list 按 name 排序；接受 cursor 参数；`title`/`annotations`（camelCase）在存在时输出
- 工具执行超时接入 `config.tools.default_timeout`（`tokio::time::timeout`）；移除 `block_in_place` 反模式；修复 handler 内 4 处漏 `.await` 的日志静默丢失
- CORS 从 `Any` 收紧为本机回环来源白名单（随 config 端口生成）
- `structuredContent` 随工具结果返回（content text 保留序列化 JSON 兼容旧客户端）

**Windows/Linux 双端（W 项部分）**：
- config.yaml 定位改为「exe 同目录 → 工作目录」两级回退，规避服务/计划任务 CWD 漂移
- 日志初始化读取 `logging.level`/`logging.format`（实测 json 格式生效）
- SIGTERM 处理（Unix）+ 关闭排水截止：实测发现 axum 0.7 的 graceful shutdown 会被浏览器 keep-alive 长连接无限拖延（进程滞留、端口已释放），采用「信号 + 10 秒截止 → select 强制退出」方案，实测 SIGTERM 后 10.0 秒干净退出
- `webbrowser::open` 移入 `spawn_blocking`；移除 6 个未使用依赖（rmcp/tokio-stream/thiserror/bytes/uuid/futures-util）

**实测冒烟（19 项 curl 用例全通过）**：discover、tools/list（23 工具排序）、tools/call 成功/未知/禁用/执行错误、`-32020/-32022/-32602/-32600` 全部分支、通知 202、GET 405、未知方法 404、initialize 404、恶意 Origin 403、base64 头解码、SSE 订阅 ack + list_changed 推送、SIGTERM 优雅退出。

### 遗留问题（下一轮）

| 优先级 | 事项 |
|--------|------|
| P2 | 鉴权（Bearer Token）、工具权限沙箱、限流；inputSchema 的 JSON Schema 校验（规范 MUST） |
| P2 | `notifications/progress`、`logging`（`io.modelcontextprotocol/logLevel`）工具性通知 |
| P3 | outputSchema 定义与填充；title/annotations 目前恒为 None，需在各工具 def 中补值 |
| P3 | 分页 nextCursor（当前单页全量返回，接口已可收 cursor） |
| P3 | W1 PowerShell UTF-8 输出编码、W3 glob 反斜杠路径、W4 非 UTF-8 文件读取、W5 已修但 exe 目录回退日志仍建议显式打印 |
| P3 | README / 技术规格文档 全面更新（本报告可作素材）；`webbrowser` 产生的僵尸子进程可在退出时统一收割 |
| 说明 | legacy 客户端（如 llama.cpp UI 的 HTTP+SSE 2024-11-05 传输）将无法连接本服务器——这是 2026-07-28 modern-only 的既定取舍，如需兼容要实现双时代服务器 |

---

## 附：修复记录 · 第二轮（2026-08-30，按用户决策执行）

用户决策：可选 Bearer Token；**兼容 llama.cpp UI（加双时代层）**；新增 jsonschema crate；run_command 保持不限。

### 已完成（全部实测通过）

**双文件处理**
- `src/mcp/sse.rs` → **`src/mcp/transport.rs`** 重命名（消除与旧 HTTP+SSE 传输的混淆），同时承载双时代路由
- `README.md` **全量重写**（双协议面用法、安全模型、配置说明、集成指引）；`技术规格文档.md` 顶部加过时警示横幅

**双时代兼容层（llama.cpp UI）**
- Legacy 2024-11-05 HTTP+SSE：`GET /sse`（首条 `endpoint` 事件为纯 URI 字符串——修正旧实现发 JSON 的违规形状）+ `POST /message?sessionId=xxx`（initialize 握手返回 `2024-11-05`、notifications → 202、tools/list、tools/call、ping；结果不带 resultType/_meta 注入，符合旧语义）
- 工具启停经 legacy 通道推送 `notifications/tools/list_changed`（实测 2 次推送到达）
- CORS/Origin 白名单可配置：`config.server.allowed_origins`（回环始终放行）——llama.cpp UI 浏览器跨 Origin 直连时把它的来源加进配置即可（实测白名单 Origin 200、恶意 Origin 403）

**安全与校验**
- 可选 Bearer Token：`MCP_AUTH_TOKEN` 环境变量 > `config.server.auth_token`，非空即启用，覆盖 `/mcp`、`/sse`、`/message`（实测：无 token/错 token 401、正确 token 200、legacy 通道同样生效）
- 输入校验（规范 MUST）：jsonschema crate（0.52，`validator_for` 预编译存入 `ToolEntry`），失败 → `result.isError:true` + schema 错误文本（实测 `"x" is not of type "integer"`）
- 移除 `webbrowser` 依赖：改用 `tokio::process` 直启 xdg-open/open/cmd start，孤儿进程由 tokio 回收器收割（消除僵尸子进程）

**协议补全**
- tools/list 输出 `title` 与 `annotations`（camelCase；23 个工具的只读/破坏性/开放世界标注集中在 `main.rs::tool_meta`）
- cursor 分页：`nextCursor`（单页 50 条，当前 23 工具单页返回）

**双端兼容（W 项收尾）**
- W1：Windows PowerShell 命令前置 `[Console]::OutputEncoding=UTF8` + `-NoProfile`（GBK 控制台不再乱码）
- W3：glob 模式路径统一 `/` 分隔（Windows 反斜杠路径不再有转义歧义）
- W4：read_file/grep/count_lines/complex_search 改字节读取 + `from_utf8_lossy`（GBK 文件不再报错/被跳过）
- W7：bash 缺失时回退 `/bin/sh`（Alpine 等）
- W9：complex_search 全流程移入 `spawn_blocking`（不再阻塞异步 worker）

**实测冒烟（第二轮 17 项全通过）**：现代端点回归（discover/list/call/校验失败/Origin 403）、tools/list 含 title+annotations、jsonschema 校验消息、Legacy 全流程（endpoint 事件、握手、202、旧形状 tools/call、list_changed 推送）、Token 401/200（现代+legacy 双通道）、Origin 白名单放行。`cargo test` 3/3。

### 遗留问题（更新）

| 优先级 | 事项 |
|--------|------|
| P2 | `notifications/progress`、`logging`（`io.modelcontextprotocol/logLevel`）工具性通知 |
| P2 | 请求级 SSE 响应（当前全部 application/json，规范允许）；限流 |
| P3 | outputSchema 定义与填充（structuredContent 已返回但无 schema 声明） |
| P3 | Token 比较非常量时间；Dashboard /api/* 未纳入鉴权（浏览器无法带 header，需 WebUI 登录方案） |
| 说明 | 双时代为既定折中：llama.cpp UI 升级到 2026-07-28 后可移除 `/sse`、`/message` 路由 |

---

## 附：修复记录 · 第三轮（2026-08-30，第一阶段功能落地）

针对用户五项需求，全部实现并实测：

**① 自动唤起浏览器 + 优雅关闭 + 断连横幅**
- 唤起/优雅关闭此前已达成；本次补 WebUI「⛔ 服务器已断开」横幅：Dashboard 页面挂日志 SSE，
  服务器关闭→流断开→横幅显示 + 状态点变红；服务器恢复后 EventSource 自动重连并隐藏横幅。
- 「关闭打开的标签页」经浏览器安全模型不可行（服务端无法关闭 OS 唤起的标签页），横幅为用户拍板的替代方案。

**② 工具热重载（卸载 → 改动 → 重载，不重启服务器）——架构级重构**
- 项目重构为 **Cargo workspace**：`server`（服务器）+ `tool_kit`（子进程契约 ToolDecl/ToolOutput + `kzm_tool!` 宏）+ `tools`（23 个 `kzm-*` 工具二进制）。
- 工具与服务器契约：`<bin> decl` 输出定义 JSON；`<bin> call` stdin 读参数、stdout 回结果。
- 服务器 `mcp/plugins.rs`：启动扫描（exe 目录 + `tools.discovery_path`）自动发现；**探测/重载失败一律 ERROR 日志**且不阻断启动；每次调用独立子进程（崩溃隔离、Windows 无 DLL 文件锁、kill_on_drop 超时连子进程终止）。
- 实测闭环：改工具源码 → `cargo build --bin kzm-hello-world`（3.6 秒增量）→ `POST /api/tools/hello_world/reload` → 新行为即刻生效；二进制缺失时重载 → 409 + ERROR 日志；恢复后重载 → 200。其他工具全程不受影响。
- 原 23 个原生工具全部迁移为插件；日志系统补结构化 `tool` 字段。

**③ 日志按工具筛选**
- `LogEntry` 增加 `tool: Option<String>`（工具相关日志全部结构化标注）；WebUI 工具卡片单击 = 筛选该工具日志（含历史），再点卡片或点「[筛选中，点击取消]」恢复全部；日志行带工具徽章。

**④ 资源与提示词（v11 第一阶段）**
- 协议方法：`prompts/list`、`prompts/get`（必填参数校验 + `{{arg}}` 模板渲染）、`resources/list`、`resources/read`；现代与 legacy 通道均支持；capabilities 更新为 `tools+prompts+resources`（不再虚报）。
- 文件驱动注册表：`mcp_data/prompts`（`.json` 或带 YAML front matter 的 `.md`，**直接兼容 v11 Anthropic Skills 格式**）、`mcp_data/resources`（`text` 内联或 `file` 相对路径——file 型每次读取磁盘最新内容）；`/api/prompts/reload`、`/api/resources/reload` 热重载；解析失败逐条 ERROR 日志。
- 订阅过滤器扩展：`promptsListChanged`/`resourcesListChanged`（`resourceSubscriptions` 暂不支持，ack 中按规范省略）；legacy `/sse` 同步转发两类变更通知。实测：新增提示词文件 → 重载 → 订阅流收到 `notifications/prompts/list_changed`。
- WebUI 增加提示词/资源页签（列出名称/描述/参数/URI + 从磁盘重载按钮）。

**实测冒烟（第三轮 16 项全通过）**：插件发现 23 个、tools/call 子进程执行、schema 校验、prompts/resources 全方法（含 md 渲染与 file 型资源）、未知资源 -32602、热重载三态（成功/失败日志/恢复）、卸载-重载组合、legacy initialize+prompts+resources、订阅推送、断连横幅依赖的日志流。构建零警告，`cargo test` 通过。

**遗留（更新）**
- P2：`notifications/progress`、`logging` 工具性通知；请求级 SSE 响应；限流。
- P3：outputSchema 声明；Token 常量时间比较；Dashboard /api/* 鉴权（WebUI 登录方案）；v11 域模块（ai_bridge/memory/netease/fanqie/browser_automation）按需以 `plugins/<域>/` 插件形式移植——pdf_reader 与 sequential_thinking 已于第四轮完成，插件架构就绪，新增即建一个域 crate。

