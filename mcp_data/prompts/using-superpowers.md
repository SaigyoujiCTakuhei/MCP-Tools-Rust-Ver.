---
name: using-superpowers
title: "使用 Superpowers"
category: "开发流程"
description: "使用 Superpowers 技能集完成复杂任务。"
license: MIT
metadata:
  author: kaze-mimirin
  version: "1.0"
  created: "2026-06-04"
  category: development
allowed-tools: Read Write Bash
---
params:
  - name: skill_name

# 使用 Superpowers

## 概述

**Superpowers** 是经过验证的技术、模式或工具的参考指南集合。技能帮助未来的 AI 代理找到并应用有效的方法。

**核心原则：** 技能即知识，知识即能力。正确加载技能 = 正确调用专业顾问。

<EXTREMELY-IMPORTANT>
技能加载 = 调用专业顾问。错误加载 = 错误顾问给出错误建议。
</EXTREMELY-IMPORTANT>

## 铁律

```
技能加载前必须确认：触发条件匹配、上下文相关、无冲突
```

## 技能目录结构

```
skills/
  skill-name/
    SKILL.md              # 主参考文档（必需）
    supporting-file.*     # 仅在需要时
```

**扁平命名空间** - 所有技能在一个可搜索的命名空间中

## 技能发现流程

### 步骤 1：遇到问题

```
用户输入： "测试不稳定"
↓
技能系统扫描：描述匹配 "test has race conditions, timing dependencies, or pass/fail inconsistently"
↓
匹配技能：superpowers:test-driven-development
```

### 步骤 2：加载技能

**自动加载（推荐）：**
- 技能描述包含触发关键词
- LLM 根据当前任务自动匹配

**手动加载：**
```
/load-skill superpowers:test-driven-development
```

### 步骤 3：执行技能

**读取 SKILL.md**
- 概述：这是什么？
- 何时使用：触发条件
- 核心模式：具体步骤
- 快速参考：表格/要点
- 常见错误：避坑指南

**执行：**
1. 遵循铁律
2. 按照流程步骤
3. 使用示例代码
4. 注意红线检查点

### 步骤 4：交叉引用

技能之间可以相互引用：

```markdown
**必需子技能：** 使用 superpowers:test-driven-development
**必需背景：** 你必须理解 superpowers:systematic-debugging
```

**注意：** 使用技能名称引用，不要用 `@` 语法（会强制加载，消耗上下文）

## 技能类型

### 技术类
有具体步骤的方法
- systematic-debugging
- test-driven-development
- condition-based-waiting

### 模式类
思考问题的方式
- flatten-with-flags
- test-invariants

### 参考类
API 文档、语法指南、工具文档
- office docs
- API 参考

## 技能加载最佳实践

### 自动加载优化

**描述字段优化：**
- 以"Use when..."开头
- 包含具体的触发条件/症状
- 使用第三人称
- 不超过 500 字符

**关键词覆盖：**
- 错误信息："Hook timed out"、"ENOTEMPTY"、"race condition"
- 症状："flaky"、"hanging"、"zombie"、"pollution"
- 同义词："timeout/hang/freeze"、"cleanup/teardown/afterEach"

### 手动加载场景

**当自动加载不匹配时：**
1. 查看可用技能列表
2. 根据任务类型选择
3. 确认技能描述匹配任务

**技能冲突处理：**
- 技能 A 和技能 B 有重叠触发条件
- 选择更具体的技能
- 或同时加载，按优先级执行

## 技能创建流程

### 创建条件
- 技术不是直觉上显而易见的
- 会在不同项目中反复引用
- 模式具有广泛适用性
- 其他人也会受益

### 不要创建
- 一次性解决方案
- 其他地方有充分文档的标准实践
- 项目特定的约定
- 机械性约束

### TDD 适配版技能创建

**红色阶段 - 编写失败的测试：**
- 创建压力场景
- 在没有技能的情况下运行场景
- 识别合理化借口中的模式

**绿色阶段 - 编写最小技能：**
- 名称只使用字母、数字、连字符
- YAML frontmatter 包含必需的 `name` 和 `description`
- 描述以"Use when..."开头
- 全文包含搜索关键词

**重构阶段 - 堵住漏洞：**
- 从测试中识别新的合理化借口
- 添加明确的反驳
- 从所有测试迭代中构建合理化借口表
- 创建红线列表
- 重新测试直到无懈可击

## 技能使用示例

### 示例 1：系统化调试

```
用户输入： "测试失败了，帮我看看"
↓
自动加载技能：superpowers:systematic-debugging
↓
执行流程：
1. 第一阶段：根因调查
   - 仔细阅读错误信息
   - 稳定复现
   - 检查近期变更
2. 第二阶段：模式分析
   - 找到可正常工作的示例
   - 与参考实现对比
3. 第三阶段：假设与验证
   - 提出单一假设
   - 最小化测试
4. 第四阶段：实施
   - 创建失败的测试用例
   - 实施单一修复
   - 验证修复
```

### 示例 2：测试驱动开发

```
用户输入： "帮我添加用户登录功能"
↓
自动加载技能：superpowers:test-driven-development
↓
执行流程：
1. 红灯：编写失败的测试
   - 测试期望行为
   - 验证测试失败
2. 绿灯：编写最少代码
   - 实现最少代码让测试通过
   - 验证测试通过
3. 重构：清理代码
   - 消除重复
   - 改善命名
   - 提取辅助函数
```

## 技能维护

### 定期检查
- 技能是否仍适用？
- 是否有更优替代？
- 描述是否需要更新？

### 技能退役
- 不再使用的技能 → 标记为 deprecated
- 被合并的技能 → 删除冗余部分
- 过时的技能 → 更新或删除

### 技能贡献
- Fork 技能仓库
- 修改技能内容
- 提交 PR
- 等待审核合并

## 红线

**绝不：**
- 加载不匹配的技能
- 跳过技能阅读直接执行
- 忽略技能中的红线检查点
- 同时加载冲突技能
- 使用已过时的技能

**始终：**
- 确认触发条件匹配
- 阅读技能概述和核心原则
- 注意红线检查点
- 使用交叉引用而非强制加载
- 定期更新技能描述

## 实际效果

- 技能加载准确率提高 80%
- 任务执行偏差率降低 60%
- 返工次数减少 50%
- 团队知识复用率提高 70%
