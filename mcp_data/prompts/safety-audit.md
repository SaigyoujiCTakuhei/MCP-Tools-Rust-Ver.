---
name: safety-audit
title: "安全审计"
category: "安全"
description: "对项目或操作进行安全审计与风险检查。"
---
# 安全检查指南

## 概述
本指南介绍如何使用进化引擎进行Prompt安全检查。

## Step 1: 检查文本安全性
调用 `check_safety` 检查文本的安全漏洞。
```
调用 check_safety(text="text to check")
```

## Step 2: 验证变体
调用 `validate_variant` 验证变体的完整性。
```
调用 validate_variant(variant_id="variant_xxx")
```

## Step 3: 添加自定义安全模式
调用 `add_safety_pattern` 添加自定义安全模式。
```
调用 add_safety_pattern(category="custom", pattern="regex_pattern")
```

## Step 4: 检查注入攻击
检测Prompt注入攻击模式。
- `ignore (previous|all|above) (instructions?|prompts?)`
- `disregard (previous|all|above)`
- `forget (everything|all|previous)`
- `you are now a`

## Step 5: 检查命令注入
检测命令注入攻击模式。
- `$(...)` 命令替换
- `` `...` `` 反引号命令
- `; (rm|cat|curl|wget|chmod|sudo)` 命令链

## Step 6: 检查路径遍历
检测路径遍历攻击模式。
- `../` 上级目录引用
- `..\` Windows上级目录引用
- `/etc/(passwd|shadow)` 敏感文件访问

## Step 7: 检查资源限制
检查变体的资源使用是否在限制内。
- CPU使用率 < 80%
- 内存使用 < 500MB
- 响应时间 < 5秒
- Token使用 < 4096

## Step 8: 检查策略合规性
检查变体是否符合安全策略。
- forbidden_patterns：禁止的模式列表
- min_length：最小长度限制
- max_length：最大长度限制
- allowed_tokens：允许的Token列表

## Step 9: 修复安全违规
基于检查结果修复安全违规。
1. 替换注入模式为安全版本
2. 限制特殊字符使用
3. 添加转义处理
4. 设置合理的资源限制

## Step 10: 重新验证
修复后重新运行安全检查确保合规。
```
调用 check_safety(text="fixed text")
调用 validate_variant(variant_id="variant_xxx")
```

## 注意事项
- 定期检查安全模式是否需要更新
- 关注新出现的攻击模式
- 平衡安全性与灵活性
- 记录安全审计结果用于趋势分析
