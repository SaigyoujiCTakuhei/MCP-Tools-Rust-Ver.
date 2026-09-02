---
name: report-structure
title: "报告结构规范"
category: "规范与文档"
description: "执行报告的结构规范（HTML/CSS 模板与样式约定）。"
---

# 报告结构规范

> 风见血月 v0.9.0 执行报告标准模板

## 全局CSS变量规范（报告头部需包含）

```css
:root {
  --bg: #f5f7fb;
  --panel: #ffffff;
  --text: #172033;
  --muted: #667085;
  --line: #d9e0ea;
  --good: #147d3f;
  --warn: #b7791f;
  --info: #2151a3;
  --code-bg: #101828;
  --code-text: #e6edf7;
}
```

## 分层报告结构体系

### 固定基础模块（5个，所有场景必须包含）

| 模块 | 格式 | 用途 |
|------|------|------|
| `## 任务元数据` | `## 任务元数据` + `---` + 两列表格 | 指令来源、触发时间、耗时统计 |
| `## 最终结论` | `section class="verdict"` | 核心发现/结论、解决方案 |
| `## 执行摘要` | `section class="verdict/warning"` | 目标达成度、核心产出物 |
| `## 执行过程` | 结构化表格 | 按时间顺序展示执行步骤 |
| `## 归档校验`（footer） | `p class="footer"` | 全量保留声明 + 校验码 |

### 场景可选模块（根据场景类型选择2-4个）

| 场景类型 | 可选模块 |
|---------|---------|
| 分析类（网络/运维/代码） | 架构说明、关键指标、根因分析、技术要点总结 |
| 组织/管理类（组织架构/项目） | 现状描述、人员关系、职责划分、风险项分析 |
| 日常/生活类 | 背景描述、相关人物、时间地点、解决方案、后续计划 |
| 会议/沟通类 | 会议信息、议题列表、讨论要点、决策事项、待办事项 |
| 学习/计划类 | 学习目标、进度跟踪、知识点、复习计划、评估标准 |

### 通用扩展模块（按需添加）

`## 影响分析`、`## 事件时间线`、`## 解决方案`、`## 防止复发建议`、`## 工具轨迹`、`## 工具调用问题`、`## 潜在优化`、`## 附录`

### 模块选择逻辑

LLM在生成报告前，调用 `sequentialthinking` 分析问题类型，根据映射表选择模块组合：
- **固定基础模块**：始终包含（5个）
- **场景可选模块**：选择2-4个
- **通用扩展模块**：根据具体内容按需添加

### 场景示例

| 场景 | 模块组合 |
|------|---------|
| 网络故障排查报告 | 任务元数据 + 最终结论 + 执行摘要 + 架构说明 + 关键指标 + 执行过程 + 影响分析 + 根因分析 + 事件时间线 + 防止复发建议 + 技术要点总结 + 潜在优化 + 归档校验 |
| 组织架构调整报告 | 任务元数据 + 最终结论 + 执行摘要 + 现状描述 + 人员关系 + 职责划分 + 执行过程 + 影响分析 + 风险项分析 + 变更管理 + 优化建议 + 潜在优化 + 归档校验 |
| 旅行规划报告 | 任务元数据 + 最终结论 + 执行摘要 + 背景描述 + 时间地点 + 相关人物 + 执行过程 + 解决方案 + 后续计划 + 潜在优化 + 归档校验 |
| 会议纪要报告 | 任务元数据 + 最终结论 + 执行摘要 + 会议信息 + 议题列表 + 讨论要点 + 执行过程 + 决策事项 + 待办事项 + 下次会议 + 潜在优化 + 归档校验 |

## 固定基础模块详细说明

### `## 任务元数据`
- **格式**：`## 任务元数据` 标题 + `---` 水平分割线 + `<div class="module-card">` 包裹的 `<div class="meta-table-container">` 内嵌两列表格
- **表格结构**：
  - **表头**：`字段` | `值`
  - **必填行**：指令来源、触发时间、工作区、任务类型、状态、摘要（共6行）
  - **扩展行**：耗时统计、工具调用频次、标准文件、目标目录、参考文档等（按需追加）
- **状态标识**：状态行使用规范中定义的 `.status-success` / `.status-warning` / `.status-error` 类
- **内容**：指令来源、触发时间、耗时统计、工具调用频次
- **强制要求**：**每次生成报告时必须包含此模块**，且必须使用两列表格样式（字段/值），禁止使用副标题/卡片/列表/纯文本等替代方案呼嗯~
- **HTML+CSS完整代码**：见下方附录A

### `## 最终结论`
- **格式**：`## 最终结论` 标题 + `<div class="module-card">` 包裹的 `<section class="verdict">` 样式卡片
- **内容**：根因描述、修复方式、关键影响
- **卡片要求**：所有模块必须包裹在 `.module-card` 容器中，具有左边框、圆角、阴影效果

### `## 执行摘要`
- **格式**：`## 执行摘要` 标题 + `<div class="module-card">` 包裹的 `<section class="verdict">` 或 `<section class="warning">`
- **verdict**：左边框5px绿色，用于成功/解决的结论
- **warning**：左边框5px橙色，用于警告/待处理/风险项
- **内容**：目标达成度、核心产出物、关键决策点
- **卡片要求**：所有模块必须包裹在 `.module-card` 容器中

### `## 执行过程`
- **格式**：`## 执行过程` 标题 + `<div class="module-card">` 包裹的 `badge` 标签标注分类 + 结构化表格
- **badge样式**：圆角胶囊标签，蓝色背景
- **表格列**：步骤 | 操作/测试内容 | 结果 | 结论
- **卡片要求**：所有模块必须包裹在 `.module-card` 容器中

### `## 归档校验`
- **格式**：footer页脚 `p class="footer"`
- **内容**：全量保留声明 + 校验码 + 生成者标记

## 通用扩展模块详细说明

### `## 影响分析`
- **格式**：`## 影响分析` 标题 + `<div class="module-card">` 包裹的 `<section class="warning">` 样式卡片
- **用途**：展示影响范围、影响服务、持续时间等
- **卡片要求**：所有扩展模块也必须包裹在 `.module-card` 容器中

### `## 根因分析`（分析类场景可选）
- **格式**：`## 根因分析` 标题 + `<div class="module-card">` 包裹的 section标题 + h3子标题（直接原因、根本原因、关键教训）
- **修复方式**：使用有序列表 `<ol>` 展示步骤
- **卡片要求**：所有模块必须包裹在 `.module-card` 容器中

### `## 事件时间线`
- **格式**：`## 事件时间线` 标题 + `<div class="module-card">` 包裹的 结构化表格（时间 | 事件）
- **用途**：按时间顺序展示关键事件
- **卡片要求**：所有模块必须包裹在 `.module-card` 容器中

### `## 防止复发建议`
- **格式**：`## 防止复发建议` 标题 + `<div class="module-card">` 包裹的 无序列表 `<ul>`
- **内容**：预防措施、监控建议、巡检脚本、配置变更管理
- **卡片要求**：所有模块必须包裹在 `.module-card` 容器中

### `## 技术要点总结`（分析类场景可选）
- **格式**：`## 技术要点总结` 标题 + `<div class="module-card">` 包裹的 无序列表 `<ul>` + 加粗键值对
- **内容**：故障类型、检测方式、修复难度、影响范围
- **卡片要求**：所有模块必须包裹在 `.module-card` 容器中

### `## 工具轨迹`
- **格式**：`## 工具轨迹` 标题 + `<div class="module-card">` 包裹的 结构化表格 + `pre` 代码块
- **表格列**：步骤 | 工具名称 | 参数快照 | 返回摘要 | 结论
- **卡片要求**：所有模块必须包裹在 `.module-card` 容器中

### `## 工具调用问题`
- **格式**：`## 工具调用问题` 标题 + `<div class="module-card">` 包裹的 结构化表格（不脱敏）
- **表格列**：时间 | 工具名 | 问题类型 | 参数快照 | 错误摘要 | 处理方式 | 影响范围
- **卡片要求**：所有模块必须包裹在 `.module-card` 容器中

### `## 潜在优化`
- **格式**：`## 潜在优化` 标题 + `<div class="module-card">` 包裹的 `div class="grid"` + `div class="card"` 并列卡片
- **内容**：位置/问题/方案/难度1-3/推荐1-5
- **卡片要求**：所有模块必须包裹在 `.module-card` 容器中

## 模块卡片容器规范

> 所有报告模块（包括固定基础模块、场景可选模块、通用扩展模块）必须包裹在 `.module-card` 容器中呼嗯~

### HTML结构

```html
<div class="module-card">
  <h2>模块标题</h2>
  <div class="module-content">
    <!-- 模块内容 -->
  </div>
</div>
```

### CSS样式

```css
/* 模块卡片容器 */
.module-card {
  background: var(--panel);
  border: 1px solid var(--line);
  border-left: 5px solid var(--good);
  border-radius: 8px;
  padding: 20px 24px;
  margin: 24px 0;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
}

/* 警告类卡片 */
.module-card.warning {
  border-left-color: var(--warn);
}

/* 信息类卡片 */
.module-card.info {
  border-left-color: var(--info);
}

/* 标题样式 - 增大字号 */
.module-card h2 {
  font-size: 1.5rem;
  font-weight: 700;
  color: var(--text);
  margin: 0 0 16px 0;
  padding-bottom: 12px;
  border-bottom: 2px solid var(--line);
}

.module-card h3 {
  font-size: 1.2rem;
  font-weight: 600;
  color: var(--text);
  margin: 16px 0 12px 0;
}

/* 内容区域 */
.module-content {
  font-size: 0.95rem;
  color: var(--text);
  line-height: 1.8;
}

.module-content p {
  margin-bottom: 12px;
}

.module-content ul,
.module-content ol {
  margin: 12px 0 12px 24px;
}

.module-content li {
  margin-bottom: 8px;
}

/* 代码块 */
.module-content code {
  background: var(--code-bg);
  color: var(--code-text);
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 0.85rem;
  font-family: "Fira Code", "Consolas", monospace;
}
```

### 卡片左边框颜色规范

| 模块类型 | 左边框颜色 | 样式类 |
|---------|-----------|--------|
| 固定基础模块（默认） | 绿色 `var(--good)` | 无 |
| 警告/风险项 | 橙色 `var(--warn)` | `.warning` |
| 信息/提示 | 蓝色 `var(--info)` | `.info` |
| 失败/异常 | 红色 `#c00` | `.error` |

## badge标签使用规范

```html
<span class="badge">入口</span>
<span class="badge">后端</span>
<span class="badge">网络层</span>
```

**使用场景**：标识架构组件角色、网络层级、任务类型等

## 状态标识规范

| 标识 | 样式类 | 颜色 | 含义 |
|------|--------|------|------|
| ✅ `success` | `.status-success` | `var(--good)` 绿色 | 成功/已解决 |
| ⚠️ `warning` | `.status-warning` | `var(--warn)` 橙色 | 警告/待处理/风险 |
| ❌ `error` | `.status-error` | `#c00` 红色 | 失败/异常 |
| ℹ️ `info` | `.status-info` | `var(--info)` 蓝色 | 信息/提示 |

## 数据保留规范

- **脱敏范围**：无
- **保留策略**：全量保留所有原始数据，严禁任何形式的数据脱敏或隐藏
- **保留字段**：IP地址、域名、URL、证书哈希、文件路径、API密钥、配置参数、用户ID、会话ID、端口号、路由等全部保留

## 数据保留示例

```html
<!-- ❌ 脱敏（禁止） -->
<span class="redacted">192.168.1.100</span>

<!-- ✅ 全量保留（推荐） -->
IP: 192.168.1.100
```

## 附录A：任务元数据模块完整HTML+CSS代码

> 每次生成报告时，必须使用以下完整代码作为"任务元数据"模块的基础模板呼嗯~

### HTML模板

```html
<div class="module-card">
  <h2>## 任务元数据</h2>
  <div class="module-content">
    ---

    <div class="meta-table-container">
      <table class="meta-table">
        <thead>
          <tr>
            <th class="meta-field">字段</th>
            <th class="meta-value">值</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td class="meta-field">指令来源</td>
            <td class="meta-value">主人（风见魅铃）</td>
          </tr>
          <tr>
            <td class="meta-field">触发时间</td>
            <td class="meta-value">2026-06-08 18:06（中国标准时间 Asia/Shanghai）</td>
          </tr>
          <tr>
            <td class="meta-field">工作区</td>
            <td class="meta-value">E:\Codes\AI Related\MCP Server\v10（试验场）</td>
          </tr>
          <tr>
            <td class="meta-field">任务类型</td>
            <td class="meta-value">归档执行</td>
          </tr>
          <tr>
            <td class="meta-field">状态</td>
            <td class="meta-value status-success">✅ 已解决</td>
          </tr>
          <tr>
            <td class="meta-field">摘要</td>
            <td class="meta-value">注册中心统一重构方案落地执行</td>
          </tr>
          <!-- 以下行为扩展行，按需追加 -->
          <tr>
            <td class="meta-field">耗时统计</td>
            <td class="meta-value">约10分钟</td>
          </tr>
          <tr>
            <td class="meta-field">工具调用频次</td>
            <td class="meta-value">47次</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</div>
```

### CSS样式

```css
/* 任务元数据模块 */
.meta-table-container {
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 16px;
  margin: 16px 0;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
}

.meta-table {
  width: 100%;
  border-collapse: collapse;
}

.meta-table th {
  background: #f0f4f8;
  color: var(--text);
  font-weight: 600;
  padding: 10px 14px;
  text-align: left;
  border-bottom: 2px solid var(--line);
  font-size: 0.85rem;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.meta-table td {
  padding: 10px 14px;
  border-bottom: 1px solid var(--line);
  font-size: 0.9rem;
  color: var(--text);
  vertical-align: top;
}

.meta-table td:last-child {
  color: var(--muted);
}

.meta-table tbody tr:last-child td {
  border-bottom: none;
}

.meta-table tbody tr:hover {
  background: #fafbfc;
}

/* 字段列加粗 */
.meta-field {
  font-weight: 500;
  color: var(--text) !important;
  white-space: nowrap;
  width: 120px;
}

/* 值列自适应宽度 */
.meta-value {
  word-wrap: break-word;
}
```
