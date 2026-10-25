# Spec Delta

## Purpose

为 FrameBraid 的转换、采样与张量热路径提供可重复、可观测且受资源与兼容边界约束的验收依据，区分已有实现、改造效果与未验证范围。

## ADDED Requirements

### Requirement: Reuse without changing output

SDK MUST 在支持的像素与张量路径减少可避免的上下文创建、分配或复制，保留原像素/张量与坐标映射合同。

#### Scenario: Stable conversion shape

- **WHEN** 同会话连续处理相同格式尺寸
- **THEN** 转换上下文不随帧数逐次重建，格式改变及时重建，帧持有与关闭安全保持

### Requirement: Sample before pixel materialization

流式 SDK SHALL 在无到期像素消费者时避免无用颜色转换，同时维护视频参考链和控制通知。

#### Scenario: Mixed sampling consumers

- **WHEN** 30 FPS 输入被不同 FPS 消费者订阅
- **THEN** 交付率按各订阅配置验证，减少转换不等于任意跳过压缩参考帧解码

### Requirement: Bounded reusable ownership

复用缓冲 MUST 受数量/字节预算限制，不覆写外部持有帧，耗尽时明确拒绝或有界退化。

#### Scenario: Held frame exhausts pool

- **WHEN** 调用方长期持帧或异步推理未结束
- **THEN** 原始内容不变且资源不无限增长，归还后可恢复

