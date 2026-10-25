# Proposal

## Why

已有队列和恢复基础仍需贯穿中转暂存、回调及外部持有验证，确保压力下结果新鲜和可预测恢复。

## What Changes

- 全链路有界压力和故障恢复，具体任务及量化条件见 tasks.md。
- 复用既有预算、所有权和 ABI；新增公共能力先同步合同和生成器，保留原冻结历史。
- 原始性能证据只放 .local，公开摘要不包含凭据、设备地址和客户图片。

## Capabilities

### New Capabilities

- `bounded-pipeline-recovery`：全链路有界压力和故障恢复的可验证行为与性能边界。

### Modified Capabilities

无已有主规格；不修改宿主业务或既有公开 ABI 的兼容语义。

## Impact

仅影响 FrameBraid 核心、受影响测试、基准及文档。复用 R1 预算及 R2 候选；关键安全缺口发现即优先修，不将修复推迟到长跑。 不修改 AM-RT，不自动发布或增加平台承诺。
