# Spec Delta

## Purpose

为 FrameBraid 的低等待、公平调度与显式组批提供可重复、可观测且受资源与兼容边界约束的验收依据，区分已有实现、改造效果与未验证范围。

## ADDED Requirements

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

