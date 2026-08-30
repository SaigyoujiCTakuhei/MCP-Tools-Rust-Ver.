---
name: evolution-workflow
description: "进化引擎操作步骤指南"
---
# 进化引擎操作步骤指南

## 概述
本指南介绍如何使用进化引擎进行Prompt优化。

## Step 1: 启动进化周期
调用 `start_evolution` 工具启动新的进化周期。
```
调用 start_evolution(trigger_type="manual")
```

## Step 2: 监控状态
调用 `get_evolution_status` 工具检查进化进度。
```
调用 get_evolution_status(cycle_id="cycle_xxx")
```

## Step 3: 评估变体
调用 `evaluate_variant` 工具评估变体的适应度。
```
调用 evaluate_variant(variant_id="variant_xxx", test_results={...})
```

## Step 4: 变异Prompt
调用 `mutate_prompt` 工具对Prompt进行变异。
```
调用 mutate_prompt(prompt="original prompt", mutation_type="paraphrase")
```

## Step 5: 交叉变体
调用 `crossover_variants` 工具交叉两个变体。
```
调用 crossover_variants(parent1_id="id1", parent2_id="id2")
```

## Step 6: 生成A/B测试对
调用 `generate_ab_pair` 工具生成A/B测试对。
```
调用 generate_ab_pair(base_variant_id="base_id")
```

## Step 7: 分析Prompt复杂度
调用 `analyze_prompt` 工具分析Prompt复杂度。
```
调用 analyze_prompt(prompt="prompt to analyze")
```

## Step 8: 解释适应度
调用 `explain_fitness` 工具解释适应度评分。
```
调用 explain_fitness(variant_id="variant_xxx", test_results={...})
```

## Step 9: 注册适应度函数
调用 `register_fitness_function` 注册自定义适应度函数。
```
调用 register_fitness_function(name="custom_func", weight=1.0)
```

## Step 10: 更新适应度权重
调用 `update_fitness_weights` 更新适应度函数权重。
```
调用 update_fitness_weights(weights={"speed": 0.5, "accuracy": 0.3, "efficiency": 0.2})
```

## Step 11: 查看适应度函数列表
调用 `list_fitness_functions` 查看已注册的适应度函数。
```
调用 list_fitness_functions()
```

## Step 12: 验证变体
调用 `validate_variant` 工具验证变体的安全性。
```
调用 validate_variant(variant_id="variant_xxx")
```

## Step 13: 检查安全性
调用 `check_safety` 工具检查文本的安全性。
```
调用 check_safety(text="text to check")
```

## Step 14: 添加安全模式
调用 `add_safety_pattern` 工具添加自定义安全模式。
```
调用 add_safety_pattern(category="custom", pattern="regex_pattern")
```

## Step 15: 记录指标
调用 `record_metrics` 工具记录执行指标。
```
调用 record_metrics(task_id="task_xxx", response_time=1.5, success=True)
```

## Step 16: 获取指标窗口
调用 `get_metrics_window` 工具获取指定时间窗口的指标。
```
调用 get_metrics_window(window_minutes=60)
```

## Step 17: 检查进化触发条件
调用 `check_evolution_trigger` 工具检查是否应触发进化。
```
调用 check_evolution_trigger()
```

## Step 18: 检测异常
调用 `detect_anomalies` 工具检测指标异常。
```
调用 detect_anomalies(sensitivity=2.0)
```

## 注意事项
- 每个进化周期有唯一的cycle_id
- 变体有唯一的variant_id
- 适应度评分范围为[0, 1]
- 收敛时进化自动停止
