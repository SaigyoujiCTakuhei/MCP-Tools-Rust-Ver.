# MCP Server Rust 实现计划 — 决策与实现指南

> 📅 **创建时间**：2026-07-15  
> 📝 **编写者**：风见血月  
> 📂 **工作区**：`E:\Codes\AI Related\MCP Server\New_Architecture_v00`  
> 🎯 **目标**：基于 Rust 构建高性能 MCP Server，服务于 llama.cpp UI，支持热插拔与 WebUI 管理。

---

## 目录

1. [技术栈总览](#1-技术栈总览)
2. [架构设计](#2-架构设计)
3. [核心决策清单（Q1-Q14）](#3-核心决策清单q1-q14)
4. [配置规范](#4-配置规范)
5. [通信协议](#5-通信协议)
6. [实现路线图](#6-实现路线图)

---

## 1. 技术栈总览

| 组件 | 技术选型 | 说明 |
|------|----------|------|
| **主语言** | Rust | 高性能、无 GIL、并发能力强 |
| **Web 框架** | Axum | 异步 HTTP 服务器，生态成熟 |
| **MCP SDK** | rmcp | Rust 版 MCP 协议实现 |
| **工具实现** | Python | 保留现有生态（Playwright/AI SDK） |
| **执行方式** | `python script.py` | 简单直接，无需 uv 依赖 |
| **WebUI** | 原生 HTML + Vanilla JS | 轻量级，无需 React/Vue |
| **静态资源** | rust-embed | 编译进二进制（发布模式） |
| **配置管理** | YAML (`config.yaml`) | 集中管理端口、路径、开关 |
| **日志传输** | JSON over stderr | 结构化日志，实时推送 |
| **实时通信** | WebSocket | WebUI 日志推送、工具状态更新 |

---

## 2. 架构设计

### 2.1 系统架构图

```
┌─────────────────────────────────────────────────────────────┐
│                      MCP Client (llama.cpp UI)               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │  WebUI (58082)│  │  MCP (58081) │  │  WebSocket (58082)│  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                   Rust Server (58081)                        │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Axum (WebUI + API + WebSocket)                       │  │
│  │  - GET /              → WebUI HTML                    │  │
│  │  - GET /api/tools     → 工具列表                      │  │
│  │  - WS /api/logs       → 实时日志推送                  │  │
│  └───────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  rmcp (MCP Streamable HTTP)                           │  │
│  │  - POST /mcp            → MCP 协议处理                │  │
│  │  - ToolRegistry         → 内存注册表                  │  │
│  └───────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Process Manager                                      │  │
│  │  - spawn python script.py                           │  │
│  │  - 管理生命周期、超时、stderr 日志                    │  │
│  └───────────────────────────────────────────────────────┘  │
└───────────────────────────┬─────────────────────────────────┘
                            │
            ┌───────────────┼───────────────┐
            ▼               ▼               ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐
│ Native Tools  │ │ Python Tools  │ │ Browser Svc   │
│ (Rust 执行)   │ │ (子进程执行)  │ │ (常驻进程)    │
│               │ │               │ │ :58083        │
│ - add         │ │ - browser_*   │ │ - Playwright  │
│ - skill_use   │ │ - ai_bridge   │ │ - 共享实例    │
└───────────────┘ └───────────────┘ └───────────────┘
```

### 2.2 数据流

1. **工具发现**：`discover.py` 扫描 `Library/*/scripts/*.py` → 输出 JSON → Rust 加载到 `ToolRegistry`
2. **工具调用**：MCP Client → `POST /mcp` → Rust 路由 → `python script.py --tool xxx --args '{"a":1}'` → 返回结果
3. **日志推送**：Python stderr → Rust 捕获 → WebSocket 推送 → WebUI 显示
4. **热插拔**：WebUI 点击 Toggle → `POST /api/tools/:name/toggle` → Rust 更新 Registry → WebSocket 推送变更

---

## 3. 核心决策清单（Q1-Q14）

### Q1：脚本路径获取
- **决策**：使用 `func.__code__.co_filename` 获取脚本路径
- **实现**：在 `@tool` 装饰器中自动注入 `script_path` 字段到注册中心
- **理由**：比 `inspect.getfile` 更可靠，支持动态 import 场景

### Q2：有状态工具进程管理
- **决策**：方案 A — 常驻子进程服务
- **实现**：
  - Rust 启动时 spawn 一个 Python 浏览器服务进程（监听 58083）
  - 该进程全局共享 Playwright browser 实例
  - Rust 通过 HTTP POST 调用浏览器操作
  - Rust 退出时发送 SIGTERM 优雅关闭子进程
- **理由**：避免每次调用启动浏览器的开销（~2-5s），保持状态一致

### Q3：多工具 Dispatch
- **决策**：`discover.py` 扫描元数据 + Rust 路由转发 + Python 动态调用
- **实现**：
  - `discover.py` 提取工具名、参数、描述、脚本路径
  - Rust 根据 `tool_name` 路由到对应脚本
  - Python 脚本通过 `importlib` 动态加载模块，`getattr` 获取函数，`asyncio.run` 执行
- **理由**：解耦 Rust 与 Python 实现，支持任意语言工具扩展

### Q4：Python 执行方式
- **决策**：`python script.py`
- **理由**：简单直接，无需 uv 依赖，与现有环境兼容

### Q5：`list_changed` 响应
- **决策**：WebSocket 作为兜底方案
- **实现**：
  - 优先依赖 MCP 协议 `tools/listChanged` 通知
  - 若客户端不支持，Rust 通过 WebSocket `/api/logs` 推送工具列表变更
  - WebUI 收到推送后自动刷新列表
- **理由**：兼容不同 MCP 客户端能力，确保 WebUI 实时性

### Q6：WebUI 交付方式
- **决策**：混合方案
  - **发布模式**：`rust-embed` 将 HTML/CSS/JS 编译进二进制
  - **开发模式**：`--features dev` 切换为从磁盘读取 `webui/` 目录
- **理由**：发布时单二进制部署，开发时改完即生效（无需重新编译）

### Q7：MCP 端点设计
- **决策**：遵循 MCP Streamable HTTP 标准
- **端点**：
  - `POST /mcp` — MCP 协议通信
  - `GET /` — WebUI 页面
  - `GET /api/tools` — 工具列表 API
  - `POST /api/tools/:name/toggle` — 工具开关
  - `POST /api/refresh` — 刷新所有工具
  - `WS /api/logs` — 实时日志推送
- **理由**：标准化、易扩展、前后端分离

### Q8：自动打开浏览器
- **决策**：Rust 层实现
- **实现**：WebUI 服务就绪后，调用 `std::process::Command::new("cmd").args(&["/c", "start", "http://127.0.0.1:58082"])`
- **理由**：跨平台兼容（Windows/macOS/Linux 分别处理），用户体验好

### Q9：Python 通信协议格式
- **决策**：JSON over stdin/stdout，支持扩展
- **格式**：
  ```json
  // Rust → Python
  {"tool_name": "add", "arguments": {"a": 1, "b": 2}, "session_id": "abc123"}
  
  // Python → Rust
  {"success": true, "result": 3, "error": null}
  ```
- **扩展性**：JSON 解析器忽略未知字段，新增字段不影响旧代码

### Q10：日志管道方案
- **决策**：方案 A — stderr 结构化日志
- **实现**：
  - Python 脚本通过 `print('{"level":"INFO","msg":"..."}', file=sys.stderr, flush=True)` 输出日志
  - Rust 读取 stderr，解析 JSON，广播到 WebSocket
- **理由**：实现简单，与现有 `logs_autoLogger.py` 兼容，延迟低

### Q11：并发调用策略
- **决策**：
  - **无状态工具**：每次调用 spawn 独立子进程（完全隔离）
  - **有状态工具**：使用常驻服务（如浏览器服务），天然支持并发
- **理由**：无状态工具 spawn 开销可接受，有状态工具复用实例提升性能

### Q12：配置管理方式
- **决策**：`config.yaml` 配置文件
- **位置**：项目根目录
- **理由**：集中管理、易读易改、支持版本控制

### Q13：`@tool` 装饰器兼容性
- **决策**：方案 A — 修改装饰器，增加 `script_path` 参数
- **实现**：
  ```python
  @tool(name="add", description="...", script_path=__file__)
  async def add(a: int, b: int) -> int:
      return a + b
  ```
- **理由**：简单直接，装饰器自动记录路径，无需 AST 扫描

### Q14：config.yaml 具体内容
- **决策**：
  - MCP 端口：58081
  - WebUI 端口：58082
  - 后续端口依次顺延（如浏览器服务 58083）
  - 配置文件放入项目根目录
- **理由**：端口隔离、易于管理、符合惯例

---

## 4. 配置规范

### 4.1 config.yaml 结构

```yaml
# config.yaml — MCP Server 配置（项目根目录）
server:
  mcp_port: 58081        # MCP Streamable HTTP 端口
  webui_port: 58082      # WebUI 端口（浏览器网页）
  host: "127.0.0.1"      # 监听地址
  auto_open_browser: true # 启动后自动打开浏览器

browser_service:
  enabled: true          # 是否启用浏览器服务
  port: 58083            # 浏览器服务端口（顺延）
  idle_timeout: 1800     # 空闲超时（秒），0 表示永不超时

tools:
  discovery_path: "Library"  # 工具模块根目录
  default_timeout: 30        # 工具调用默认超时（秒）
  max_concurrent: 10         # 最大并发调用数

logging:
  level: "INFO"              # 日志级别
  format: "structured"       # 日志格式（structured = JSON）
  stderr_flush: true         # 是否强制 flush stderr

webui:
  dev_mode: false            # 开发模式（true=外部文件，false=内置）
  features: ["dev"]          # Cargo features（开发模式用）
```

### 4.2 端口分配规则

| 服务 | 端口 | 说明 |
|------|------|------|
| MCP Server | 58081 | Streamable HTTP 端点 |
| WebUI | 58082 | 静态文件 + API |
| Browser Service | 58083 | 常驻 Python 子进程 |
| 后续服务 | 58084+ | 依次顺延 |

---

## 5. 通信协议

### 5.1 Rust ↔ Python 协议

#### 请求格式（Rust → Python）

```json
{
  "tool_name": "add",
  "arguments": {"a": 1, "b": 2},
  "session_id": "abc123"
}
```

**字段说明：**
- `tool_name`（必填）：工具名称
- `arguments`（必填）：参数字典
- `session_id`（可选）：会话 ID，用于有状态工具关联

#### 响应格式（Python → Rust）

**成功：**
```json
{
  "success": true,
  "result": 3,
  "error": null
}
```

**失败：**
```json
{
  "success": false,
  "result": null,
  "error": "参数类型错误: a 必须是整数"
}
```

**字段说明：**
- `success`（必填）：执行是否成功
- `result`（可选）：执行结果
- `error`（可选）：错误信息

### 5.2 日志格式（stderr）

```json
{"level": "INFO", "msg": "开始执行 add 工具", "timestamp": "2026-07-15T10:00:00Z"}
{"level": "ERROR", "msg": "执行失败", "error": "Connection refused", "timestamp": "2026-07-15T10:00:01Z"}
```

**字段说明：**
- `level`（必填）：日志级别（INFO/WARNING/ERROR）
- `msg`（必填）：日志消息
- `error`（可选）：错误详情
- `timestamp`（可选）：时间戳

---

## 6. 实现路线图

### 第一步：创建 Rust 项目骨架
- [ ] 初始化 Cargo 项目
- [ ] 配置 `Cargo.toml`（依赖：axum, rmcp, tokio, rust-embed, serde, serde_yaml）
- [ ] 创建目录结构（`src/`, `webui/`, `config.yaml`）
- [ ] 实现 `config.yaml` 读取模块

### 第二步：实现配置与端口管理
- [ ] 解析 `config.yaml`
- [ ] 端口冲突检测
- [ ] 环境变量覆盖支持（可选）

### 第三步：实现 discover.py + Rust 端工具注册
- [ ] 编写 `discover.py`（AST 扫描工具元数据）
- [ ] Rust 端加载 discover 结果到 `ToolRegistry`
- [ ] 实现 `@tool` 装饰器（注入 `script_path`）

### 第四步：实现 Python 子进程 dispatch + 日志管道
- [ ] Rust 端 `call_python_tool` 实现
- [ ] Python 脚本 `__main__` 入口
- [ ] stderr 日志捕获与 WebSocket 推送

### 第五步：实现 WebUI + WebSocket 实时推送
- [ ] 编写 HTML/CSS/JS（工具列表、控制台）
- [ ] Axum 静态文件服务（内置 + dev 模式）
- [ ] WebSocket 日志推送实现
- [ ] API 端点（`/api/tools`, `/api/refresh`）

### 第六步：实现 browser_service 常驻子进程
- [ ] Python 浏览器服务脚本（Playwright）
- [ ] Rust 进程管理（spawn/kill/健康检查）
- [ ] HTTP 接口（`/navigate`, `/click` 等）

### 第七步：集成测试 + 垂直切片验证
- [ ] 测试 `add` 工具全流程
- [ ] 测试 `browser_navigate` 工具全流程
- [ ] 测试热插拔（toggle + refresh）
- [ ] 测试日志实时推送
- [ ] 测试 WebUI 自动打开浏览器

---

## 附录：关键文件清单

| 文件 | 说明 |
|------|------|
| `config.yaml` | 服务器配置 |
| `discover.py` | 工具元数据发现脚本 |
| `browser_service.py` | 浏览器常驻服务 |
| `webui/index.html` | WebUI 页面（开发模式） |
| `src/main.rs` | Rust 入口 |
| `src/config.rs` | 配置解析 |
| `src/registry.rs` | 工具注册表 |
| `src/dispatch.rs` | Python 子进程调度 |
| `src/webui.rs` | WebUI 服务 |
| `src/ws.rs` | WebSocket 推送 |

---

*文档生成时间：2026-07-15*  
*生成者：风见血月（Hermes Coding Agent）*
