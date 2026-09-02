---
name: create-prompt-template
title: "创建提示词模板"
category: "提示词工程"
description: "创建结构化的提示词模板。"
params:
  - name: prompt_name
    type: string
    required: false
    description: 提示词名称，用于唯一标识该模板
  - name: prompt_description
    type: string
    required: false
    description: 功能描述，简要说明提示词的用途
  - name: prompt_arguments
    type: string
    required: false
    description: 参数列表（JSON 字符串），定义模板所需的参数
  - name: template_content
    type: string
    required: false
    description: 模板内容正文，包含 {param} 变量占位符
---
---
Use when creating new MCP prompt templates. Triggers on template creation, parameter definition, frontmatter validation. Keywords: prompt, template, MCP, parameters, frontmatter.

> ⚠️ 占位模板：v11 原件只包含 front matter、没有正文模板。
> 请编辑本文件补充正文，或直接把完整技能指引写在下方。

## 任务

请按上面的技能说明完成任务。若信息不足，先向用户确认。