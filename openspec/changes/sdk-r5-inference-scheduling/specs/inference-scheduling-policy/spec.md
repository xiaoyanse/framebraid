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

#### Scenario: Transient provider failure followed by retry

- **WHEN** 暂时设备忙或内存不足后调用方显式重试
- **THEN** 在预算和运行库共存约束内重验，不受永久失败缓存锁死，记录恢复结果且保持请求唯一终态

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

