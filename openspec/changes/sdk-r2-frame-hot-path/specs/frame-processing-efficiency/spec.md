# Spec Delta

## Purpose

为 FrameBraid 的转换、采样与张量热路径提供可重复、可观测且受资源与兼容边界约束的验收依据，区分已有实现、改造效果与未验证范围。

## ADDED Requirements

### Requirement: Observable native hardware decode selection

SDK MUST 为直接解码和流式像素输出提供策略约束下的硬件探测与自动选择，区分编译支持、设备发现、依赖、初始化和目标码流实际解码；返回实际后端、设备及原因，而非仅返回硬解偏好。Windows H.264/H.265 的 D3D11VA 为本轮路径，DXVA2 为可用时的兼容候选。

#### Scenario: Hardware present but target decoding unavailable

- **WHEN** 存在显卡但依赖、初始化、编码/profile/位深或资源条件不满足
- **THEN** 明确报告失败层次并遵循 SOFTWARE_ONLY/PREFER_HW 合同回退，未测不报成功，纯压缩直传不创建解码器

### Requirement: Bounded hardware decode recovery

探测与恢复 SHALL 受时间、并发、内存和代次约束，瞬态负结果可重试，活帧在实际读取结束前不能释放或覆写。

#### Scenario: Device fails during output

- **WHEN** 硬解运行中失效或重新配置
- **THEN** 隔离旧输出代次、按关键帧恢复并报告实际回退，重试不循环放大资源；浏览器 WebCodecs 状态不冒充本 SDK 验收

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

