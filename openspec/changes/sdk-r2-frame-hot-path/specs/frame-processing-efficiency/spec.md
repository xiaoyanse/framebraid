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

### Requirement: Explicit native hardware foundation

Rust设备探测入口 MUST 有界、显式且与普通状态查询分离；设备上下文初始化成功不得标记真实码流已验证。显式设备构造 MUST 保持默认CPU入口兼容，设备身份可靠、原生引用成对归还，输出预算不足或回读失败不得丢失待输出帧。

#### Scenario: Runtime compiled with legacy D3D11 only

- **WHEN** FFmpeg可以创建D3D11设备但目标解码器只提供AD_HOC配置
- **THEN** 显式硬解构造拒绝，不误报运行成功；新运行库构建门检查H.264和H.265各自的HW_DEVICE_CTX配置

#### Scenario: Transfer fails before output

- **WHEN** 已接收硬件帧但CPU回读失败或输出预算不足
- **THEN** 保留原待输出帧及时间戳供明确重试或关闭；不把GPU地址解释为CPU像素，最终由同一运行库释放资源

### Requirement: Shared native selection policy

直接解码和RTSP像素路径的初始化选择 SHALL 使用同一有界策略。SoftwareOnly不触发硬件探测；PreferHardware在目标解码器配置核对后按确定顺序选择，探测忙/耗尽或没有匹配设备时返回CPU与拥有的原因报告。初始化选择 MUST 与实际硬件输出分开，不永久缓存暂时失败。

#### Scenario: Independent explicit probe is busy

- **WHEN** 显式probe正在进行原生初始化，另一个调用请求PreferHardware
- **THEN** 共用准入立即报告Busy并使用CPU，不同时放大硬件初始化、不把CPU标为硬件执行

#### Scenario: Codec has no generic hardware configuration

- **WHEN** 运行库能创建设备但目标解码器没有该设备的HW_DEVICE_CTX配置
- **THEN** 不为该组合创建设备，保留缺配置原因，并继续有限候选或CPU回退

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

### Requirement: Hardware readback reservation

SDK SHALL 在CPU回读目标帧分配前核对其对齐与填充后的保守字节数，不能只检查输出RGBA字节。可缓存一张目标帧，但预算许可 MUST 覆盖其完整寿命，先释放缓冲再归还许可；错误或重新配置不得遗留未计费缓存。

#### Scenario: Readback exceeds remaining domain budget

- **WHEN** 目标帧回读所需字节超出资源域剩余额度
- **THEN** 分配前明确拒绝，待输出硬件帧与PTS保留；额度恢复后可重试，丢弃/关闭后回读额度归零

### Requirement: Bounded reusable ownership

复用缓冲 MUST 受数量/字节预算限制，不覆写外部持有帧，耗尽时明确拒绝或有界退化。

#### Scenario: Held frame exhausts pool

- **WHEN** 调用方长期持帧或异步推理未结束
- **THEN** 原始内容不变且资源不无限增长，归还后可恢复

