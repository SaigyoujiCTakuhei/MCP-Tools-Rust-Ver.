---
name: finishing-a-development-branch
title: "完成开发分支"
category: "开发流程"
description: "收尾开发分支：合并、清理与交付前检查。"
---
params:
  - name: current_branch

# 完成开发分支

## 概述

通过提供清晰的选项并执行所选工作流来引导开发工作的收尾。

**核心原则：** 验证测试 → 检测环境 → 展示选项 → 执行选择 → 清理。

<EXTREMELY-IMPORTANT>
验证测试 → 检测环境 → 展示选项 → 执行选择 → 清理
</EXTREMELY-IMPORTANT>

## 铁律

```
先验证测试，再展示选项
```

## 何时使用

- 实现完成
- 所有测试通过
- 需要决定如何集成工作

## 流程

### 步骤 1：验证测试

**在展示选项之前，验证测试通过：**

```bash
# 运行项目的测试套件
npm test / cargo test / pytest / go test ./...
```

**如果测试失败：**

```
测试失败（<N> 个失败）。必须先修复才能继续：

[显示失败信息]

在测试通过之前无法进行合并/PR。
```

停止。不要继续到步骤 2。

**如果测试通过：** 继续步骤 2。

### 步骤 2：检测环境

**在展示选项之前，先确定工作区状态：**

```bash
GIT_DIR=$(cd "$(git rev-parse --git-dir)" 2>/dev/null && pwd -P)
GIT_COMMON=$(cd "$(git rev-parse --git-common-dir)" 2>/dev/null && pwd -P)
```

这决定了展示哪种菜单、以及清理方式：

| 状态 | 菜单 | 清理 |
|
