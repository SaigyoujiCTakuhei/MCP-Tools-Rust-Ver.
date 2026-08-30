---
name: executing-plans
description: "Use when executing multi-step implementation plans created by writing-plans. Triggers on plan execution, task implementation, checkpoint verification. Keywords: plan, execute, tasks, checkpoint, implementation."
params:
  - name: plan_path_or_content
    type: string
    required: false
    description: 计划文件路径或计划内容（支持直接粘贴计划文本）
  - name: review_depth
    type: string
    required: false
    description: 审查深度（light/standard/deep），决定审查的详细程度
  - name: checkpoint_interval
    type: integer
    required: false
    description: 检查点间隔（每完成 N 个任务暂停审查一次）
---
---

# 执行计划

## 概述

加载计划，批判性审查，执行所有任务，完成后报告。

**核心原则：** 先审查，再执行，每步验证。

<EXTREMELY-IMPORTANT>
开始时宣布："我正在使用 executing-plans 技能来实现此计划。"
</EXTREMELY-IMPORTANT>

## 铁律

```
先批判性审查计划，再执行任务
```

## 参数说明

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| plan_path_or_content | string | 否 | 计划文件路径或计划内容（支持直接粘贴计划文本） |
| review_depth | string | 否 | 审查深度（light/standard/deep），决定审查的详细程度 |
| checkpoint_interval | integer | 否 | 检查点间隔（每完成 N 个任务暂停审查一次） |

## 何时使用

- 有书面实现计划需要执行
- 需要在单独会话中执行
- 设有审查检查点

**配合技能：**
- **superpowers:using-git-worktrees** - 开始前建立隔离工作区
- **superpowers:writing-plans** - 创建此技能要执行的计划
- **superpowers:finishing-a-development-branch** - 所有任务完成后收尾

## 流程

### 步骤 1：加载并审查计划

1. 读取计划文件
2. 批判性审查——识别计划中的任何问题或疑虑
3. 如果有疑虑：在开始之前向你的人类伙伴提出
4. 如果没有疑虑：创建 TodoWrite 并继续

**审查时重点检查：**
- 步骤之间是否有依赖遗漏？（A 依赖 B，但 B 排在 A 之后）
- 验证条件是否明确？（"确认可用"不算，"运行 `npm test` 全部通过"才算）
- 是否有隐含的环境假设？（Node 版本、数据库连接、API Key）

### 步骤 2：执行任务

对于每个任务：

1. **标记为进行中** — 更新 TodoWrite
2. **理解目标** — 重读任务描述，明确完成标准
3. **执行实现** — 严格按照计划步骤执行
4. **运行验证** — 按要求运行测试或检查
5. **提交变更** — 每完成一个任务提交一次，commit message 引用任务编号
6. **标记为已完成** — 更新 TodoWrite

**批量审查检查点：**
- 每完成 3 个任务后，暂停回顾：整体方向还对吗？有没有偏离计划？
- 如果发现前面的实现有问题，先修复再继续，不要带着问题往下走

### 步骤 3：处理常见异常

**测试失败：**
1. 读错误信息，定位失败原因
2. 区分：是实现 bug？还是测试本身有问题？还是计划描述有误？
3. 实现 bug → 修复并重跑
4. 测试有问题 → 修复测试，向伙伴说明
5. 计划有误 → 停下来，向伙伴报告并建议修正

**依赖缺失：**
- 停止执行
- 向伙伴报告
- 建议插入缺失的配置步骤

**指令不清：**
- 不要猜测意图，不要"合理推断"
- 列出你的理解和困惑，让伙伴澄清
- 等待回复后再继续

### 步骤 4：完成开发

所有任务完成并验证后：
- 宣布："我正在使用 finishing-a-development-branch 技能来完成此工作。"
- **必需子技能：** 使用 superpowers:finishing-a-development-branch

## 何时停下来求助

**在以下情况立即停止执行：**
- 遇到阻塞（缺少依赖、测试失败、指令不清）
- 计划有严重缺陷导致无法开始
- 你不理解某条指令
- 验证反复失败（同一测试失败 2 次以上）

**不确定时就问，不要猜测。**

## 红线

**绝不：**
- 未经用户明确同意就在 main/master 分支上开始实现
- 跳过验证
- 猜测意图而不询问
- 带着未修复的问题继续推进

**始终：**
- 先批判性审查计划
- 严格按照计划步骤执行
- 每个任务单独提交，commit message 引用任务编号
- 遇到阻塞时停下来，不要猜测

## 常见错误

| 错误 | 修正 |
|------|------|
| 跳过审查直接执行 | 先批判性审查计划，再执行任务 |
| 不验证中间结果 | 每步完成后运行验证 |
| 带着未修复的问题继续 | 发现问题先修复再继续 |
| 不更新 TodoWrite | 每步都更新进度标记 |
