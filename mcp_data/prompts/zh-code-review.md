---
name: zh-code-review
description: "Use when performing Chinese-style code reviews. Triggers on Chinese codebases, style guide checks, documentation reviews. Keywords: code-review, Chinese, style-guide, documentation, standards."
params:
  - name: file_path
    type: string
    required: false
    description: 被审查文件路径
  - name: code_snippet
    type: string
    required: false
    description: 待审查代码片段
  - name: change_type
    type: string
    required: false
    description: 变更类型（功能/修复/重构）
  - name: priority
    type: string
    required: false
    description: 优先级要求（必须修复/建议修改/仅供参考）

license: MIT
metadata:
  author: kaze-mimirin
  version: "1.0"
  created: "2026-06-04"
  category: development
allowed-tools: Read Write Bash
---
Use when performing Chinese-style code reviews. Triggers on Chinese codebases, style guide checks, documentation reviews. Keywords: code-review, Chinese, style-guide, documentation, standards.

> ⚠️ 占位模板：v11 原件只包含 front matter、没有正文模板。
> 请编辑本文件补充正文，或直接把完整技能指引写在下方。

## 任务

请按上面的技能说明完成任务。若信息不足，先向用户确认。