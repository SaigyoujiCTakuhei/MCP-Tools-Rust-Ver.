---
name: zh-commit-conventions
description: "Use when following Chinese commit conventions. Triggers on commit message creation, changelog updates, version tagging. Keywords: commit, conventions, Chinese, changelog, version."
params:
  - name: change_type
    type: string
    required: false
    description: 变更类型（feat/fix/docs/style/refactor/perf/test/chore/ci/revert）
  - name: scope
    type: string
    required: false
    description: 影响模块
  - name: description
    type: string
    required: false
    description: 变更描述
  - name: reason_and_solution
    type: string
    required: false
    description: 变更原因和方案
  - name: breaking_change
    type: string
    required: false
    description: 不兼容变更标记
  - name: issue
    type: string
    required: false
    description: 关联 Issue

license: MIT
metadata:
  author: kaze-mimirin
  version: "1.0"
  created: "2026-06-04"
  category: development
allowed-tools: Read Write Bash
---
Use when following Chinese commit conventions. Triggers on commit message creation, changelog updates, version tagging. Keywords: commit, conventions, Chinese, changelog, version.

> ⚠️ 占位模板：v11 原件只包含 front matter、没有正文模板。
> 请编辑本文件补充正文，或直接把完整技能指引写在下方。

## 任务

请按上面的技能说明完成任务。若信息不足，先向用户确认。