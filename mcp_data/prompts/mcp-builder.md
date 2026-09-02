---
name: mcp-builder
title: "MCP 服务器构建"
category: "开发流程"
description: "构建 MCP 服务器的步骤与规范。"
---
params:
  - name: project_type

# MCP 服务器构建

## 概述

系统化设计、实现、测试和部署 Model Context Protocol 服务器的方法论。

**核心原则：** 明确 Tools vs Resources vs Prompts 分工，遵循 MCP 协议规范。

<EXTREMELY-IMPORTANT>
执行操作 → Tool | 读取数据 → Resource | 引导交互 → Prompt
</EXTREMELY-IMPORTANT>

## 铁律

```
先注册再 connect，先校验再处理，先测试再部署
```

## 1. 协议核心概念

MCP 定义三种原语：

- **Tools（工具）**：AI 助手主动调用的函数，有副作用。如搜索、创建、删除操作。
- **Resources（资源）**：AI 助手只读访问的数据源，用 URI 标识。如 `users://{id}/profile`。
- **Prompts（提示词模板）**：预定义交互模板，引导用户触发工作流。

**选择原则：** 执行操作 → Tool | 读取数据 → Resource | 引导交互 → Prompt

## 2. 项目结构规范

### TypeScript

```
my-mcp-server/
├── src/
│   ├── index.ts          # 入口，注册 tools/resources
│   ├── tools/             # 按功能拆分
│   ├── resources/
│   └── lib/               # 客户端封装、校验逻辑
├── tests/
├── package.json
└── tsconfig.json
```

关键依赖：`@modelcontextprotocol/sdk` + `zod`

### Python

```
my-mcp-server/
├── src/my_mcp_server/
│   ├── server.py
│   ├── tools/
│   └── lib/
├── tests/
└── pyproject.toml
```

关键依赖：`mcp` + `pydantic`

## 3. Tool 设计原则

### 命名
- `snake_case` 格式，动词开头：`search_users`、`create_issue`、`delete_file`
- 名称自解释，AI 助手靠名称选工具，模糊命名导致误调用

### 参数
- 每个参数有类型约束和 `.describe()` 描述
- 可选参数给默认值，减少 AI 决策负担
- 用枚举代替布尔开关

### 描述
说明**用途 + 返回内容 + 限制**，这是 AI 选择工具的关键依据

### 输出
- 结构化数据 → JSON，人类可读内容 → Markdown
- 始终用 `content: [{ type: "text", text: "..." }]` 格式返回

## 4. 输入验证和错误处理

用 Zod/Pydantic 做 Schema 级校验，业务级校验放 handler 开头

**错误处理四原则：**
1. 永远不让服务器崩溃 — try/catch 包裹所有外部调用
2. 返回可操作的错误信息 — 告诉 AI 问题是什么、能做什么
3. 使用 `isError: true` — 让 AI 知道调用失败
4. 区分错误类型 — 参数错误、权限不足、资源不存在、服务不可用

## 5. 资源管理和生命周期

关键点：使用连接池、所有外部调用设超时、优雅关闭清理资源

## 6. 测试策略

### 单元测试 — 业务逻辑与 MCP 注册分离
### 集成测试 — 用 SDK Client 做端到端验证
### MCP Inspector — 交互式调试

**测试要点：** 每个 Tool 覆盖正常 + 异常路径、边界值、外部服务失败模拟

## 7. 安全考虑

**权限控制：**
- 最小权限原则，读写 Tool 分离
- 危险操作要求确认参数（如 `confirm: true`）

**输入安全：**
- SQL 注入 → 参数化查询，绝不拼接
- 路径遍历 → 校验路径，禁止 `../`
- 命令注入 → 用 `execFile` 而非 `exec`

**敏感数据：**
- 密钥通过环境变量传入，不硬编码
- 日志不打印完整敏感信息
- 返回数据做脱敏处理

## 8. 调试技巧

**关键：MCP 用 stdio 通信，不能用 `console.log`，会破坏协议流。**

| 症状 | 原因 | 解决 |
|
