---
name: variant-comparison
title: "变体对比"
category: "进化引擎"
description: "对多个方案变体进行对比评估。"
---
# 变体对比指南

## 概述
本指南介绍如何使用进化引擎对比不同变体的性能。

## Step 1: 获取变体状态
调用 `get_evolution_status` 获取当前周期的变体信息。
```
调用 get_evolution_status(cycle_id="cycle_xxx")
```

## Step 2: 评估变体
调用 `evaluate_variant` 评估变体的适应度。
```
调用 evaluate_variant(variant_id="variant_xxx", test_results={
    "success_rate": 0.85,
    "avg_response_time": 1.2,
    "error_count": 2
})
```

## Step 3: 解释适应度
调用 `explain_fitness` 获取适应度详细分解。
```
调用 explain_fitness(variant_id="variant_xxx", test_results={
    "success_rate": 0.85,
    "avg_response_time": 1.2
})
```

## Step 4: 分析Prompt复杂度
调用 `analyze_prompt` 分析变体Prompt的复杂度。
```
调用 analyze_prompt(prompt="variant prompt")
```

## Step 5: 对比不同变异策略
对同一Base Prompt使用不同变异策略生成变体，然后对比。
```
# 变体A: paraphrase变异
调用 mutate_prompt(prompt="base", mutation_type="paraphrase")

# 变体B: instruction_add变异
调用 mutate_prompt(prompt="base", mutation_type="instruction_add")

# 变体C: context_expand变异
调用 mutate_prompt(prompt="base", mutation_type="context_expand")
```

## Step 6: A/B测试对比
生成A/B测试对进行受控对比。
```
调用 generate_ab_pair(base_variant_id="base_id")
```

## Step 7: 对比评估结果
对比不同变体的适应度评分。
- 关注 `explain_fitness` 返回的各维度得分
- 对比 `success_rate`（成功率）
- 对比 `avg_response_time`（响应时间）
- 对比 `error_count`（错误数）

## Step 8: 选择最优变体
基于评估结果选择最优变体进行下一步进化。
- 优先选择适应度高的变体
- 考虑多样性指标避免过早收敛
- 关注错误模式分布

## 注意事项
- 确保测试条件一致（相同输入、相同环境）
- 至少运行10次测试以获得可靠统计
- 考虑Prompt的鲁棒性而不仅是平均性能
- 注意变体的多样性避免同质化
