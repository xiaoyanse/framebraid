# Spec Delta

## Purpose

为 FrameBraid 的低等待、公平调度与显式组批提供可重复、可观测且受资源与兼容边界约束的验收依据，区分已有实现、改造效果与未验证范围。

## ADDED Requirements

### Requirement: Model-aware hardware discovery and automatic selection

SDK MUST 区分硬件可见、依赖存在、provider 可加载、目标模型会话成功与真实推理成功，按调用方允许列表/优先级及 strict/fallback 策略选择 CPU/OpenVINO/CUDA，查询实际 provider/设备及原因。OpenVINO 当前 CPU 边界及进程共存约束保持。

#### Scenario: Visible GPU cannot execute target model

- **WHEN** GPU 可见但依赖、资源或模型支持不满足
- **THEN** 明确报告原因，strict 拒绝且不静默改用 CPU；允许回退才选择允许的候选，未知状态不当作支持证明

### Requirement: Bounded backend probing and recovery

探测 SHALL 显式且有界、查询只读、瞬态失败可刷新；后端故障不自动重放完成或状态不确定请求，不释放仍被原生调用读取的输入。

#### Scenario: Preferred backend exceeds candidate thread allowance

- **WHEN** OpenVINO候选无法在全局线程硬限内准入而调用方允许的后续CPU候选可以
- **THEN** 原生构造前逐候选计费，记录resource_admission失败并仅按允许列表回退；strict或未列CPU仍拒绝，归还失败预留，关闭后允许显式重试，不增预算、不重放请求

#### Scenario: Transient provider failure followed by retry

- **WHEN** 暂时设备忙或内存不足后调用方显式重试
- **THEN** 在预算和运行库共存约束内重验，不受永久失败缓存锁死，记录恢复结果且保持请求唯一终态

#### Scenario: Legacy matrix queried before or during CUDA probe

- **WHEN** 调用方查询旧C后端矩阵而本context尚未探测、探测失败或正在刷新
- **THEN** 查询不初始化 CUDA 驱动，硬件未知通过 note 显示且不能冒充已确定不存在；仅已得到设备数的显式探测快照可作为可见性依据

#### Scenario: Rust caller requests backend state without probe

- **WHEN** Rust 调用方只查询后端状态且未提供显式设备探测结果
- **THEN** 不加载 CUDA 驱动并表示设备未知；显式探测结果经调用方传入后，只有已取得设备数才能报告存在或不存在

### Requirement: Bounded model selection and honest execution evidence

选择 SHALL 仅尝试调用方允许的最多3个backend/device候选，严格模式只尝试首项，未列CPU不得隐式回退。模型/档案错误 MUST 停止选型；后端失败可按策略继续，保留有界阶段诊断。状态锁 MUST 不跨原生构造持有，旧公开可修改字段不得伪造执行成功。

#### Scenario: Accelerator initialization blocks while another model completes

- **WHEN** 一个模型的原生构造尚未返回而另一个模型完成Run
- **THEN** 完成记账和状态读取不等待构造锁；单加速域仍在短锁内先认领，不并发装配不兼容后端

#### Scenario: Hardware path has higher per-frame latency

- **WHEN** 硬件路径包含同步/回读而单帧耗时高于CPU
- **THEN** 记录取舍并量化CPU卸载、有效交付率和并发延迟；不单凭跨后端耗时判缺陷，不据未知CPU/GPU指标宣称收益；同策略优化的警戒继续有效

### Requirement: Runnable work is not blocked by retired requests

执行器 MUST 有界、公平地选择可运行请求，取消/替换终态及时释放等待资源，不让无效积压拖死健康模型。

#### Scenario: Retired requests target a busy model

- **WHEN** 忙模型仍有已取消或被替换请求且另一模型有可运行工作
- **THEN** 在不释放仍被原生调用读取的输入前提下清理无效排队任务，健康工作继续执行，全局线程不超额

### Requirement: Optional compatible batching

自适应组批 SHALL 显式开启且可关闭，仅聚合模型/后端/形状/前处理兼容请求，受批次、等待、截止时间和字节预算限制。

#### Scenario: Fixed single batch or incompatible input

- **WHEN** 模型只支持 batch=1 或待组输入不兼容
- **THEN** 不为凑批增加等待、不填伪样本，按原拒绝/回退合同逐项返回且不重复完成

### Requirement: Explicit backend scheduling

后端选择与恢复 MUST 遵循调用方策略和实际资源/依赖状态，报告实际后端与原因。

#### Scenario: Preferred backend is unavailable

- **WHEN** 优选后端缺依赖或资源竞争且策略允许回退
- **THEN** 按合同回退或明确拒绝，不静默重放已完成请求，不扩大线程预算


### Requirement: Explicit inference probe and bounded cached C report

C调用方 SHALL 可显式触发有软预算的设备发现，并查询有界缓存及累计执行观察；无ORT共享帧context MUST 可独立探测。报告 MUST 区分未探测、失败/未知与真实原生执行，不重新装配运行库。内部缓存 SHALL 计控制额度但不作为外部租约阻塞关闭。

#### Scenario: Repeated query and concurrent shutdown

- **WHEN** 调用方反复读取设备报告，同时context关闭
- **THEN** 查询不调用驱动或改探测完成时间，读与最终释放安全互斥；关闭态返回无设备/观察的空快照，destroy后不再访问

#### Scenario: Insufficient control quota on refresh

- **WHEN** 显式新探测没有足够控制额度
- **THEN** 撤销旧成功缓存并明确resourceExhausted，不扩大默认控制预算，不把旧结果伪装为新成功

### Requirement: Selection and scheduling have distinct success criteria

后端自动选择 MUST 按允许策略、依赖及真实模型执行判断功能可用，调度优化 SHALL 选择准入/排队/过期/公平性或离线有效吞吐作为主目标，保护逐帧截止、资源及唯一终态；GPU更快不作为自动选择的通用验收条件。

#### Scenario: Batch throughput increases but per frame deadlines fail

- **WHEN** 组批均摊耗时下降而逐帧等待超过目标卡预算
- **THEN** 不判实时调度收益通过，保留离线吞吐与等待取舍，不用批次耗时除batch当作逐帧响应

#### Scenario: Model fallback is correct without faster execution

- **WHEN** 优选EP不可用而允许列表中的CPU执行成功
- **THEN** 可记录策略/故障功能通过，CPU/GPU收益与资源竞争另按代表模型验证，不以此宣称推理提速

### Requirement: Ordered model selection through C ABI

SDK SHALL 提供路径/内存有序候选加载和模型/尝试缓存查询，保持旧加载入口行为；限定允许列表、软预算及control额度，报告请求与实际设备，不暗加CPU。

#### Scenario: Concurrent attempts complete out of order

- **WHEN** 同context的较旧选择在较新选择开始后才完成
- **THEN** 旧完成不覆盖新缓存，调用方按各自ID读取，已覆盖返回resultExpired；成功模型拥有独立快照，查询不阻塞原生Run

#### Scenario: CUDA device is not ordinal zero

- **WHEN** 显式选择非零CUDA设备并成功创建模型
- **THEN** 模型信息和保留结果的设备字符串均使用实际设备号，结果关闭模型后仍可读，不引用已释放的模型字符串

### Requirement: Scenario-scoped acceptance without exhaustive duration gates

当前优化验收 SHALL 按实际声明范围验证功能、失败诊断/回退、资源边界、所有权与关闭安全；SHALL NOT 把完整硬件矩阵或固定4h/24h长跑作为强制完成条件。移除门槛不等于相应未运行测试通过，不扩大支持范围。

#### Scenario: Required targeted checks pass without an exhaustive hardware run

- **WHEN** 本包相关功能/故障/资源/接口定向验证已按范围完成，但没有全硬件组合或固定时长长跑结果
- **THEN** 不得仅因缺这两项阻塞本包收口；仍如实列实际设备/模型和测试范围，其他未完成实现或必验项照常保持开放
