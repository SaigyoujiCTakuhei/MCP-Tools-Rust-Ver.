# 纯 Rust MCP Server 架构设计文档

> **For Hermes:** 本文档为架构设计蓝图，非执行计划。后续实现前需转换为 bite-sized task 的执行计划。
>
> **设计者：** 风见血月
> **创建时间：** 2026-07-22
> **参考基线：** v11 (Playground) Python 版 MCP Server（147 工具 / 35 提示词）
> **目标：** 基于 Rust 从头构建高性能 MCP Server，服务 llama.cpp UI，支持浏览器自动化、实时日志、工具热插拔与可选 WebUI

---

## 一、设计目标

| 目标 | 说明 |
|------|------|
| **纯 Rust 二进制** | 单 `.exe` 部署，无需 Python 运行时 |
| **MCP Streamable HTTP** | 完全兼容 MCP 协议，通过 `rmcp` crate |
| **内置浏览器自动化** | `chromiumoxide` 替代 Playwright，覆盖核心操作 |
| **实时日志推送** | WebSocket 广播，WebUI 控制台实时展示 |
| **工具热插拔** | 运行时启用/禁用工具，无需重启 |
| **配置驱动** | `config.yaml` 集中管理端口、路径、特性开关 |
| **分层可插拔** | Rust trait 抽象工具执行器，后期可补 Python 子进程 |

---

## 二、系统架构总图

```
┌────────────────────────────────────────────────────────────────┐
│                    llama.cpp UI (MCP Client)                    │
│  WebUI :58082  │  MCP :58081  │  WebSocket :58081/api/logs    │
└──────────────────────────────┬─────────────────────────────────┘
                               │
                               ▼
┌────────────────────────────────────────────────────────────────┐
│                   Rust MCP Server (单二进制)                     │
│                                                                │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  Axum HTTP Router (:58081)                               │  │
│  │  ├── POST /mcp              → rmcp service (MCP 协议)    │  │
│  │  ├── GET  /                 → WebUI (rust-embed)         │  │
│  │  ├── GET  /api/tools        → 工具 JSON 列表              │  │
│  │  ├── POST /api/tools/:name/toggle → 工具开关              │  │
│  │  ├── POST /api/refresh        → 热重载工具                 │  │
│  │  ├── GET  /api/logs          → 日志分页查询               │  │
│  │  └── WS   /api/logs/stream   → 实时日志推送               │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  ToolRegistry (DashMap<String, Arc<ToolDefinition>>)      │  │
│  │  ├── register() / unregister() / get() / list()          │  │
│  │  ├── toggle()  → enabled/disabled 热切换                 │  │
│  │  └── notify()  → WebSocket 推送变更事件                  │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                │
│  ┌──────────┐  ┌──────────────┐  ┌──────────────────────────┐  │
│  │ Executor │  │ Browser Svc  │  │ LogSystem (tracing+broadcast)│  │
│  │ (native) │  │ (chromiumoxide)│  │ ├── FileAppender          │  │
│  │          │  │              │  │ └── WebSocketForwarder    │  │
│  └──────────┘  └──────────────┘  └──────────────────────────┘  │
└────────────────────────────────────────────────────────────────┘
```

### 端口分配

| 服务 | 端口 | 协议 |
|------|------|------|
| MCP Server | 58081 | Streamable HTTP |
| WebUI | 58081 | HTTP（同端口，路由区分） |
| WebSocket | 58081 | `/api/logs/stream` 升级 |

> **说明**：纯 Rust 架构只需一个端口，MCP、WebUI、WebSocket 统一在 Axum 路由中区分，不再需要浏览器服务的独立端口（浏览器操作由 `chromiumoxide` 在进程内完成）。

---

## 三、Cargo 项目结构

```
mcp-server/
├── Cargo.toml                    # 项目定义 + 依赖
├── config.yaml                   # 运行时配置
├── src/
│   ├── main.rs                   # 入口：启动 tokio runtime + Axum
│   ├── config.rs                 # 配置解析 (serde_yaml)
│   ├── registry/
│   │   ├── mod.rs                # 模块声明
│   │   ├── tool_registry.rs      # 工具注册表 (DashMap)
│   │   ├── tool_definition.rs    # ToolDefinition 数据结构
│   │   ├── resource_registry.rs  # 资源注册表（提示词等）
│   │   └── hotplug.rs           # 热插拔逻辑（toggle/refresh/notify）
│   ├── mcp/
│   │   ├── mod.rs              # MCP 协议适配层
│   │   └── handler.rs           # POST /mcp → rmcp service
│   ├── executor/
│   │   ├── mod.rs              # tool execution trait
│   │   ├── native.rs            # 原生工具执行器
│   │   └── subprocess.rs       # Python 子进程执行器（Phase 2 fallback）
│   ├── browser/
│   │   ├── mod.rs              # chromiumoxide 封装
│   │   ├── browser_mgr.rs      # 浏览器生命周期管理
│   │   └── tools.rs             # browser_* 工具实现
│   ├── tools/                    # 通用工具实现（每种功能一个文件）
│   │   ├── mod.rs
│   │   ├── add.rs
│   │   ├── file_ops.rs         # read_file, write_file, delete_file
│   │   ├── directory_ops.rs     # list_directory, create_directory
│   │   ├── search.rs           # grep, search_files, complex_search等
│   │   ├── shell.rs            # run_command
│   │   ├── net.rs              # web_fetch, web_search, download_file
│   │   ├── system.rs           # get_system_info, get_time, get_date
│   │   ├── git.rs              # git_clone, git_pull
│   │   ├── metrics.rs        # count_lines, file_info
│   │   └── ai_sdk.rs           # AI SDK 桥接（reqwest + SSE）
│   ├── webui/
│   │   ├── mod.rs              # WebUI 服务 + rust-embed
│   │   └── static/             # HTML/CSS/JS（编译期内嵌）
│   ├── ws/
│   │   └── mod.rs              # WebSocket 日志广播
│   └── logging/
│       ├── mod.rs              # tracing 初始化 + 自定义 layer
│       └── ws_layer.rs          # 将 tracing 事件转发到 WebSocket
├── webui/                        # 开发模式下前端源码
│   ├── index.html
│   ├── style.css
│   └── app.js
└── tools_prompts/                # MCP prompts 模板（.md 文件）
    ├── brainstorming.md
    ├── systematic-debugging.md
    └── ... (35 个)
```

---

## 四、核心数据结构

### 4.1 ToolDefinition（对应 Python 的 `ToolDefinition` dataclass）

```rust
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// MCP Tool 的完整定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// 工具唯一标识 (对应 Python 版 `name` 字段)
    pub name: String,
    /// 功能描述
    pub description: String,
    /// JSON Schema for 入参
    pub input_schema: serde_json::Value,
    /// 可读标题
    pub title: Option<String>,
    /// MCP 协议注解
    pub annotations: Option<ToolAnnotations>,
    /// 是否启用（热插拔开关）
    pub enabled: bool,
    /// 来源模块标识（用于分组）
    pub source: ToolSource,
}

/// MCP 协议注解（对标 ToolAnnotations）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolAnnotations {
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
    pub idempotent_hint: Option<bool>,
    pub open_world_hint: Option<bool>,
}

/// 工具来源分类
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolSource {
    Native,          // Rust 原生实现
    Browser,         // 浏览器（chromiumoxide）
    Subprocess,      // Python 子进程（fallback）
    Plugin,           // 外部插件
}
```

### 4.2 执行器 Trait（类比 Python 装饰器注册的 `func`）

```rust
use async_trait::async_trait;
use std::sync::Arc;

/// 工具执行结果
#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// 工具执行器 trait
/// —— 类比Python的" @tool装饰器注册的函数"，但不绑定到特定语言。
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// 执行工具的异步入口
    async fn execute(&self, args: serde_json::Value) -> ToolResult;
}

/// 工具函数签名（用于原生 Rust 实现）
pub type AsyncToolFn = Arc<
    dyn Fn(serde_json::Value) -> Pin<Box<dyn Future<Output = ToolResult> + Send>>
           + Send + Sync,
>;
```

### 4.3 注册中心（类比 Python 的 `ToolRegistry`）

```rust
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct ToolRegistry {
    /// 工具名 → 工具定义 + 执行函数
    tools: DashMap<String, Arc<ToolEntry>>,
    /// 变更通知通道（热插拔 → WebSocket）
    notify_tx: broadcast::Sender<ToolChangeEvent>,
}

pub struct ToolEntry {
    pub definition: ToolDefinition,
    pub executor: Box<dyn ToolExecutor>,
}

#[derive(Debug, Clone)]
pub enum ToolChangeEvent {
    Registered { name: String },
    Unregistered { name: String },
    Toggled { name: String, enabled: bool },
}
```

---

## 五、模块清单与 Roff 实现对应表

以下表格列出 v11 的每一个模块的 Rust 实现策略：

| # | Python 模块 (v11) | 工具数 | Rust 实现策略 | 档位 |
|---|-------------------|--------|-------------|------|
| 1 | `Lib/tools/` — 通用工具集 | 37 | Rust `src/tools/` 目录，每个功能一个文件 | MVP |
| 2 | `Lib/browser_automation/` | 20 | `src/browser/` + `bromiumoxide` | MVP |
| 3 | `Lib/ai_bridge/` | 2 | `src/tools/ai_bridge.rs` — reqwest + SSE 流式 | MVP |
| 4 | `Lib/prompts/` | 35 | `plugins_prompts/*.md` → 文件读取 + MCP prompt 注册 | MVP |
| 5 | `Lib/fanqie je` | 10 | `src/tools/fanqie.rs` — reqwest + serde | 扩展 |
| 6 | `Lib/netease/` | 13 | `src/tools/netease.rs` — reqwest + serde | 扩展 |
| 7 | `Lib/pdf_reader/` | 2 | `lopdf` crate | 扩展 |
| 8 | `Lib/sequential_thinking/` | 1 | 纯逻辑，`src/tools/` 内 | 扩展 |
| 9 | `Lib/memory/` | 13 | Rust trait + 向量索引（Phase 2） | Phase 2 |
| 10 | `Lib/evolution/` | 61 | 不迁移于 Phase 2 或考虑 Python 子进程 fallback | Phase 2 |

---

## 六、WebUI 设计

### 6.1 功能范围

WebUI 是一个**管理面板 + 实时控制台**，类似于 llama.cpp 的 server dashboard：

- **工具列表页**：树形分组显示所有工具，开关 toggle，来源标识
- **实时日志控制台**：WebSocket 直播所有工具调用日志
- **MCP 状态**：连接状态、端口信息、运行时间
- **Prompt 列表**：35 个 MCP 提示词浏览

### 6.2 技术栈

| 组件 | 选型 |
|------|------|
| HTML/CSS | 原生 + 现代布局 |
| JS | VanillaJS + WebSocket API |
| 交付 | `rust-embed` 内嵌二进制 |

### 6.3 开发模式

```yaml
# Cargo.toml
[features]
default = []
dev = []  # 开发模式：从磁盘读再编 webui/
```

```rust
// 发布模式 → rust-embed
// 开发模式 → 读取 /webui/ 目录
#[cfg(feature = "dev")]
fn find_webui(base: &str) -> { ... }
```

---

## 七、Cargo.toml 依赖清单

```toml
[package]
name = "kazemimirin-mcp-server"
version = "0.1.0"
edition = "2021"

[dependencies]
# 异步运行时
tokio = { version = "1", features = ["full"] }
# HTTP 服务器
axum = { version = "0.7", features = ["ws"] }
# MCP 协议
rmcp = "0.4"
# 配置解析
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
serde_json = "1"
# 并发哈希表
dashmap = "6"
# 异步 trait
async-trait = "0.1"
# 静态资源嵌入
rust-embed = { version = "8", features = ["debug-embed"] }
# 日志
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
# WebSocket 广播
tokio-stream = "0.1"
# 文件系统操作（更好的错误处理）
fs-err = "3"
anyhow = "1"
thiserror = "2"
# HTTP 客户端（AI SDK 桥接）
reqwest = { version = "0.12", features = ["json", "stream"] }
futures = "0.3"
# 浏览器自动化
chromiumoxide = { version = "0.6", features = ["tokio-runtime"] }
regex = "1"
# PDP
lopdf = "0.33"  # GIF Phase 2
# 命令行解析（WebUI 内部使用）
clap = { version = "4", features = ["derive"] }

# 开发模式静态文件
[features]
dev = []
```

---

## 八、核心流程

### 8.1 启动流程

```
main()
  ├── 1. 读取 run(config.yaml)    → AppConfig
  ├── 2. 初始化 tracing subscriber → 日志系统
  ├── 3. 创建 ToolRegistry        → DashMap 初始化
  ├── 4. 注册所有 native 工具      → ToolRegistry::register(...)
  │      ├── tools/*
  │      ├── browser/* (chromiumoxide)
  │      └── ai_bridge/* (reqwest)
  ├── 5. 注册 prompts (35 个 .md) → ToolRegistry::prompts
  ├── 6. 启动 chromiumoxide         → BrowserManager::launch()
  ├── 7. 构建 Axum Router           → 路由表
  ├── 8. 启动 WebSocket 广播        → broadcast channel
  └── 9. axum::serve() + 自动打开浏览器 (若配置)
```

### 8.2 工具调用流程

```
Client → POST /mcp {tool: "add", args: {a:1,b:2}}
  ├── Axum 路由 → rmcp 协议解码 → 获取 tool 定义
  ├── ToolRegistry::get("add")
  ├── ToolEntry::execute(args)
  │     └── tool_fn(args).await → ToolResult { success: true, data: 3 }
  ├── 日志写入 tracing (→ ws_layer → 广播)
  └── 响应 JSONRPC to Client
```

### 8.3 热插拔流程

```
WebUI → POST /api/tools/browser_navigate/toggle
  ├── ToolRegistry::toggle("browser_navigate")
  │     ├── tool.enabled 元素标注
  │     └── notify_tx.send(ToolChangeEvent::Toggled{...})
  ├── WS 广播 → 所有 WebUIsi 实时更新列表
  └── 响应 200 OK
```

---

## 九、与 Python v11 的逐一映射

| Python 概念 | Rust 实现 | `std:detection` |
|-------------|---------|-----------------|
| `@tool decorder` | 注册函数 → ToolRegistry | Rust `fn register_tools()` 手动注册 |
| `ToolRegistry.get_all()` | `tool_registry.list()` | |
| `_auto_scan_lib_tools()` | 不需要（工具注册是声明式） | |
| `Context` 注入 | 无需（日志内建） | Rust tracing 全层覆盖 |
| `async def` | `async fn` + `tokio` | Rust 原生无 GIL |
| `playwright` | `chromiumoxide` | CDP 直接控制 |
| `openai/anthropic SDK` | `reqwest` + SSE 解析 | 更轻量 |
| `py工具函数` | Rust `async fn tool_fn(args) -> ToolResult` | |
| `_cache` and `over腕` function | 无需（Rust 不需要） | |
| `signal_handler` | `tokio::signal::ctrl_c()` | |

---

## 十、MVP 阶段交付物

按以下顺序实现，每一步一个 commit：

| 阶段 | 内容 | 预计估 |
|------|------|--------|
| **S1** | `Cargo.toml` + `config.yaml` + `src/main.rs` 骨架 + `src/config.rs` 解析 | 中 |
| **S2** | `ToolRegistry` + `ToolDefinition` + 注册函数 | 中 |
| **S3** | Axum Router + POST /mcp (rmcp handler) + GET /api/tools | 中 |
| **S4** | `add` 工具完整调用流程 | 中 |
| **S5** | `tracing` + WebSocket 日志广播 (ws layer) | 中 |
| **S6** | `chromiumoxide` 浏览器管理器 + `browser_navigate` | 高 |
| **S7** | WebUI (HTML/CSS/JS) + rust-embed + 与日志集成 | 高 |
| **S8** | 热插拔 toggle + 刷新 API | 中 |
| **S9** | discovery: 通用工具完整迁移 (tools/*.rs) | 多 |
| **S10** | browser_* 工具完整实现 (14 个剩余) | 多 |

---

## 十一、架构决策记录 (ADR)

| ID | 决策 | 理由 |
|----|------|------|
| 1 | 单端口 (58081) 承载 MCP + WebUI + WS | 简化部署、减少端口冲突、不额外server |
| 2 | 纯 Rust 内嵌浏览器（chromiumiumoxide） | 去除 Python 进程管理复杂度 |
| 3 | DashMap 存储 Registry | 并发读写安全，无锁竞争 |
| 4 | tracing + broadcast 日志 | 完全兼容 JSON structured logging，实时推送 |
| 5 | Prompt 模板保留为 `.md` 文件 | 与 Agent skills 规范对齐，无需数据结构化 |
| 6 | 不给除 chromium 外的多浏览器支持 | Chrome/Edge/Chromium 占 80% 使用场景；Phase 2 可加 |
| 7 | 不迁移 evolution/memory 为 MVP | 这两模块独立于 MCP 框架，可后做 |

---

## 十二、风险与 trade-off

| 风险 | 缓解措施 |
|------|---------|
| chromiumium 功能不及 Playwright | 接受 75% 覆盖率，缺失用 `browser_evaluate` JS 注入弥补 |
| 内存占用（Rust + Chromium 单进程） | Chrome headless 内存可约 200MB，可接受 |
| `lopdf` 功能有限 vs pymupdf | PDF 模块 Phase 2 降级优先级 |
| AI SDK SSE 解析复杂 | `reqwest-events` 提供流解析 |

---

**文档生成时间：2026-07 -22**
**生成者：风见血月（Hermes Coding Agent）✨**