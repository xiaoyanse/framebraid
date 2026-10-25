# Spec Delta

## Purpose

为 FrameBraid 的最小可复现观测与基线提供可重复、可观测且受资源与兼容边界约束的验收依据，区分已有实现、改造效果与未验证范围。

## ADDED Requirements

### Requirement: Build-bound reproducible measurements

基准工具 MUST 绑定准确源码/制品、运行库、输入与配置身份，提供分段计时、计数、分位数和有效吞吐口径，未知数据标为未知。

#### Scenario: Compare controlled candidates

- **WHEN** 固定输入和预算重复测量改前与候选
- **THEN** 报告机制计数、局部耗时与端到端分开；不同钟域不得相减，少交帧不得伪装提速

### Requirement: Bounded observation

观测 SHALL 有界，不在采集或推理热路径同步写文件，且提供自身开销验证。

#### Scenario: Slow observation consumer

- **WHEN** 观测停滞或输入超出预算
- **THEN** 处理通道不依赖观测在线，丢弃/拒绝有计数，内存与线程不随采样数无界增长

