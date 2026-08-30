# MCP 工具的 Rust 实现计划讨论 — 完整会话记录

> 会话时间：2026-07-09  
> 模型：HauhauCS\Qwen3.6-35B-A3B-Uncensored-HauhauCS-Aggressive-Q6_K_P → z-ai/glm-5.2 → HauhauCS\Qwen3.6-35B-A3B-Uncensored-HauhauCS-Aggressive-Q6_K_P  
> 对话轮次：约 10 轮

---

## 第 1 轮：主人提问 — Rust 写带网页的 MCP 服务器可行吗？

**主人**：

> 讨论一下，我想用 RUST 写一个附带网页的 MCP 服务器，这可行吗？
> 网页里需要列出加载了哪些 MCP 工具，并且可以使用工具旁边的按钮来拔插工具，右上角还需要一个刷新按钮，用来加载新的工具。

**血月回复摘要**：

- **结论**：完全可行，技术栈基本不会卡壳
- **核心难点**：MCP 协议运行时级热插拔
- **推荐技术栈**：axum + rmcp + rust-embed + SSE
- **两种架构理解**：
  - A：Rust 服务即是工具宿主（单一作者写工具）
  - B：Rust 服务是 MCP 代理/聚合器（统一管理多个子 MCP）
- **默认走理解 A**，主人后续确认为理解 A
- **关键设计**：ToolRegistry = Arc<RwLock<HashMap>>，enabled 字段控制开关，broadcast channel 推送变更
- **MCP 协议**：`tools/list` 只返回 enabled 工具，`tools/listChanged` 通知客户端
- **风险点**：rmcp 0.x API 不稳、stdio 客户端看不到热插拔、broadcast channel 容量、竞态条件等
- **下一步**：主人需确认理解 A 还是 B

---

## 第 2 轮：主人补充 7 条需求 + 原 Python 架构参考

**主人**：

1. "拔插"语义：调试需要卸载工具（不影响其他），修改后重新加载
2. "刷新按钮"：重新加载所有工具
3. MCP 传输模式：Streamable HTTP
4. 工具来源：本地文件夹，多个子文件夹各含不同工具
5. 网页实时更新：卸载后实时移除列表，刷新后才重新出现
6. 网页布局：左右布局，左边工具列表，单击右边出现控制台（日志/输出）
7. 启动后自动打开浏览器（Windows），网页端口 58080，MCP 端口 58081
8. 原架构是 Python 写的，问改用 Rust 后能不能继续用 .py 还是必须用 .rs

**原 Python 架构目录结构**（MCP Server\v11 (Playground)）：
- `Ent.py` — MCP 服务器主入口
- `Lib/` — 工具模块目录
  - `ai_bridge/` — AI 多模型桥接（configs/, scripts/, skills/）
  - `base/` — 基础架构（scripts/ToolRegistry.py, PromptRegistry.py, ResourceRegistry.py）
  - `browser_automation/` — 浏览器自动化（Playwright，cookies/, scripts/, skills/）

**血月回复摘要**：

- **核心矛盾在第 8 条**：Rust 不支持运行期 import 动态加载源码
- **四条路线对比**：
  - A. 全部用 Rust 编译进二进制
  - B1. 外挂 Python 子进程（推荐）
  - B2. 外挂任意语言可执行进程
  - C1. 脚本引擎（Lua/Rhai）
  - C2. WASM 插件
- **推荐混合方案 A + B1**：Rust 做外壳 + 调度器，工具本体保持 Python 或任意语言子进程
- **manifest.toml 设计**：声明 `kind = "native"` 或 `kind = "process"`，决定工具加载方式
- **目录结构改造**：保留 Lib/ 原结构，新增 manifest.toml，base 模块搬入 Rust 二进制
- **7 条需求逐条对应**：全部有明确落点
- **5 个风险点**：rmcp 版本、子进程协议、工具改完重编译、浏览器自动化、会话生命周期
- **需要主人决策**：工具插件路线选 A 还是 B1？

---

## 第 3 轮：主人要求读讨论文件

**主人**：

> 结合"E:\Codes\AI Related\MCP 工具的 Rust 实现计划讨论.txt"看看能不能给出定论？还是仍然有不确定的地方？

**血月操作**：读取了完整文件（922 行），这是之前和别的 AI 的讨论记录。

**血月回复摘要**：

### 已达成定论的部分（约 85%）：
1. Rust 壳 + Python 肉 ✅
2. 双端口 58080/58081 ✅
3. 热插拔语义（卸载=内存移除，刷新=重新 discover）✅
4. discover.py + 改造的 @tool 装饰器 ✅
5. list_tools 改为 Rust 内置 ✅
6. 网页左右布局 + WebSocket 日志 ✅
7. 启动自动开浏览器 ✅
8. 6 步开工建议 ✅

### 仍不确定的 5 个问题：
1. `inspect.getfile(func)` 能否拿到正确脚本路径？（需实测）
2. 有状态工具（browser_automation）每次 spawn 还是常驻？
3. 一个 .py 多工具时 Python 入口怎么 dispatch？
4. Rust 用 `python script.py` 还是 `uv run script.py`？
5. 目标 MCP 客户端对 `list_changed` 的实际响应？

### 建议下一步：
先做 Vertical Slice（垂直切片）——拿 ai_bridge 单工具走通整条链路，5 个问题在最小代价下自然暴露并解决。

---

## 附录 A：关键技术决策汇总

### 架构决策
| 决策项 | 选择 | 理由 |
|--------|------|------|
| 主进程语言 | Rust | 高性能、无 GIL、并发好 |
| 工具实现语言 | Python（保留） | Playwright/AI SDK 生态优势 |
| 工具发现机制 | discover.py + @tool 装饰器 | 复用原架构，无需 manifest |
| list_tools 归属 | Rust 内置 | 避免子进程开销 |
| 网页框架 | 原生 HTML + Vanilla JS | 小页面不需要 React/Vue |
| 通信协议 | Streamable HTTP (MCP) + WebSocket (日志) | 主人明确要求 |
| 端口分配 | 58080 (WebUI) / 58081 (MCP) | 主人明确要求 |

### 数据流
```
MCP Client (llama.cpp UI)
    │
    ▼ POST /mcp (Streamable HTTP)
Rust Server (58081)
    │
    ├──→ ToolRegistry (内存)
    │       ├── enabled=true  → 暴露给 MCP
    │       └── enabled=false → 隐藏
    │
    ├──→ Python Subprocess (tools/call)
    │       stdin: {"tool": name, "args": {...}}
    │       stdout: result JSON
    │       stderr: log lines → WebSocket
    │
    └──→ WebUI (58080)
            ├── GET /api/tools → 列表
            ├── POST /api/tools/:name/toggle → 拔插
            ├── POST /api/refresh → 重新 discover
            └── WS /api/logs → 实时日志
```

### 文件改造清单
| 原文件 | 改造内容 |
|--------|----------|
| `registry_tools.py` | 加 `inspect.getfile(func)` 记录 script 路径 |
| 每个工具脚本 | 加 `if __name__ == "__main__":` 入口 |
| `list_tools.py` | 改为 Rust 内置，Python 版可保留或废弃 |
| `Entrance.py` | 可废弃或改为 discover.py |
| 新增 | `discover.py`（元数据发现脚本） |

---

*文件生成时间：2026-07-09*  
*生成者：风见血月（Hermes Coding Agent）*
