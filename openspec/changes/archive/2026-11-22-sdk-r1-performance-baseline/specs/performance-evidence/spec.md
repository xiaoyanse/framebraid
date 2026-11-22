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


### Requirement: Goal specific measurement and acceptance cards

每次性能结论 MUST 声明主目标、保护指标、计时边界、有效交付分母和源码/制品身份；同路径改前/后与跨后端资源取舍 MUST 分开。历史合成预算及5%调查标签不得冒充全SDK SLA或统一验收线。

#### Scenario: Different backends exchange latency for CPU offload

- **WHEN** 硬解回读使单帧耗时增加，但目标是CPU卸载或并发承载
- **THEN** 对照同等有效交付/输出要求下CPU时间、GPU资源、帧龄和预算内合格路数；缺资源测量标证据不足，不仅凭耗时判优劣

### Requirement: Repeatability before performance verdict

收益/回归判定 SHALL 使用测前目标卡、同实现A/A波动及绝对/相对实际影响；旧raw和告警保留，不能为候选事后放宽硬预算或重写旧结果。

#### Scenario: Short benchmark exceeds historical five percent label

- **WHEN** 微基准触发旧5%警戒而波动和实际影响尚未标定
- **THEN** 保存待复核事件，不自动判失败或通过；功能安全、机制变化和性能收益分别记录，后续补适用目标卡与证据
