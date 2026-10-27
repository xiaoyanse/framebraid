# Spec Delta

## Purpose

为 FrameBraid 的转换、采样与张量热路径提供可重复、可观测且受资源与兼容边界约束的验收依据，区分已有实现、改造效果与未验证范围。

## ADDED Requirements

### Requirement: Observable native hardware decode selection

SDK MUST 为直接解码和流式像素输出提供策略约束下的硬件探测与自动选择，区分编译支持、设备发现、依赖、初始化和目标码流实际解码；返回实际后端、设备及原因，而非仅返回硬解偏好。Windows H.264/H.265 的 D3D11VA 为本轮路径，DXVA2 为可用时的兼容候选。

#### Scenario: Hardware present but target decoding unavailable

- **WHEN** 存在显卡但依赖、初始化、编码/profile/位深或资源条件不满足
- **THEN** 明确报告失败层次并遵循 SOFTWARE_ONLY/PREFER_HW 合同回退，未测不报成功，纯压缩直传不创建解码器

#### Scenario: No compatible adapter is discovered

- **WHEN** 显式硬件选择已获准检查运行库但没有可用硬件适配器
- **THEN** 报告各候选后端的编译支持和目标解码器配置；未执行的检查为未知，不能将设备缺失解释为运行库未编译，也不能将配置存在解释为实际硬解成功

### Requirement: Bounded hardware decode recovery

探测与恢复 SHALL 受时间、并发、内存和代次约束，瞬态负结果可重试，活帧在实际读取结束前不能释放或覆写。

#### Scenario: Device fails during output

- **WHEN** 硬解运行中失效或重新配置
- **THEN** 隔离旧输出代次、按关键帧恢复并报告实际回退，重试不循环放大资源；浏览器 WebCodecs 状态不冒充本 SDK 验收

#### Scenario: Recover before the first published picture

- **WHEN** 硬件失败且只接受过一张画面、未发布任何输出，保留的启动随机访问包仍拥有输入字节额度
- **THEN** 先通过CPU首张输出及规范化暂存预算准入，最多重试该包一次，保留PTS/received并递增代次；有其他已接受画面或任何已发布输出时不重放历史包，等待新随机访问点；显式reset才重新探测硬件

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

### Requirement: Shared source hardware policy and pixel isolation

RTSP源硬件策略 SHALL 属于共享连接；单拥有者首次像素订阅前可改，同值幂等。共享或首次成功像素订阅后改变 MUST 返回modeConflict，包括尚未安装回调的push订阅；失败的订阅不得冻结。只读快照 MUST 不启动硬件探测。

#### Scenario: Pixel decoder fails while compressed consumers remain

- **WHEN** 目标码流使硬件解码失败，另有压缩消费者
- **THEN** 清像素队列并按下一真实随机访问点有界CPU恢复，保持连接/压缩队列/源代次/压缩配置代次；报告单独decode_generation诊断值、原生原因和实际执行；慢驱动隔离和服务延迟另按R3量化，不能以不重连代替低延迟证明

#### Scenario: Source owner closes while another remains

- **WHEN** 同连接一个source句柄关闭但其他拥有者继续
- **THEN** 已关闭句柄快照返回closed、空选择/实际后端和非活动状态，不泄露其他拥有者后续活动；新独占源可重新设置策略

### Requirement: Reuse without changing output

SDK MUST 在支持的像素与张量路径减少可避免的上下文创建、分配或复制，保留原像素/张量与坐标映射合同。

#### Scenario: Stable conversion shape

- **WHEN** 同会话连续处理相同格式尺寸
- **THEN** 转换上下文不随帧数逐次重建，格式改变及时重建，帧持有与关闭安全保持

#### Scenario: Reconfigure an initialized hardware decoder

- **WHEN** 参数集变更需要新codec上下文但所选设备仍可用
- **THEN** 取得已有设备的独立引用而非重新探测或初始化设备；新上下文真实执行状态重新验证，构造失败不破坏原上下文或待输出帧，调用方仍管理关键帧和源代次

### Requirement: Sample before pixel materialization

流式 SDK SHALL 在无到期像素消费者时避免无用颜色转换，同时维护视频参考链和控制通知。

#### Scenario: Mixed sampling consumers

- **WHEN** 30 FPS 输入被不同 FPS 消费者订阅
- **THEN** 交付率按各订阅配置验证，减少转换不等于任意跳过压缩参考帧解码

#### Scenario: Frame arrives just before a sampling deadline

- **WHEN** 像素worker内新解码帧距可服务订阅到期点不超过其一采样周期及50ms上限
- **THEN** 可提前物化并在该worker有界等待，到期后按实际单调时间重新检查严格最短间隔；失败/关闭/新配置不提前消费机会或发布旧输出，压缩读取不等待，不以允许突发放宽max_fps

#### Scenario: Sampled consumers lag behind newer input

- **WHEN** 全部有效像素订阅均配置非零上限，且内部存在更新待处理包
- **THEN** SDK可略过旧画面的像素物化，但保留参考帧解码及维度通知；任何0上限消费者存在时不执行此额外合并，队列/在途帧继续受已有预算及代次检查约束

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


### Requirement: Decode offload and mechanism specific goals

硬解评价 MUST 以MET-2的CPU卸载或固定宿主预算合格路数为主目标，颜色转换/缓冲/张量优化 SHALL 明确机制及资源目标；各自保护帧龄、有效交付、精度/所有权和预算，不要求跨后端单帧必定更快。

#### Scenario: Hardware to CPU pixel transfer is slower

- **WHEN** 同输出要求的硬解RGBA耗时高于CPU而CPU时间尚未测量
- **THEN** 保留耗时事实并标资源取舍待量化，不判硬解缺陷、不宣称CPU卸载收益，也不将硬件帧回读标作GPU零拷贝

#### Scenario: Fewer conversions with the same sampled outputs

- **WHEN** 虚拟时钟下采样前移减少转换且采样输出保持
- **THEN** 可确认限定机制与局部耗时结果，真实网络有效FPS、CPU占用及多路承载分别验证，不把转换次数比例外推成整机收益

### Requirement: Scenario-scoped acceptance without exhaustive duration gates

当前优化验收 SHALL 按实际声明范围验证功能、失败诊断/回退、资源边界、所有权与关闭安全；SHALL NOT 把完整硬件矩阵或固定4h/24h长跑作为强制完成条件。移除门槛不等于相应未运行测试通过，不扩大支持范围。

#### Scenario: Required targeted checks pass without an exhaustive hardware run

- **WHEN** 本包相关功能/故障/资源/接口定向验证已按范围完成，但没有全硬件组合或固定时长长跑结果
- **THEN** 不得仅因缺这两项阻塞本包收口；仍如实列实际设备/模型和测试范围，其他未完成实现或必验项照常保持开放
