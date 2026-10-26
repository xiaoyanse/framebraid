# Proposal

## Why

当前公开性能资料未建立本轮构建身份下的全链路数值，缺少优化前对照。先形成小而有界的测量工具，避免凭感觉改造。

## What Changes

- 2026-10-26 指标复核采用[MET-2](../../../docs/PERFORMANCE_ACCEPTANCE.md)，本包指标组M-OBS，场景S0～S6中下一包实际需要的场景；观测正确/低开销、CPU/GPU及计数口径、A/A重复性和目标卡；既有合成基线保留，不外推生产SLA。 本轮只修订文档，保留既有实现及原始证据。

- 最小可复现观测与基线，具体任务及量化条件见 tasks.md。
- 复用既有预算、所有权和 ABI；新增公共能力先同步合同和生成器，保留原冻结历史。
- 原始性能证据只放 .local，公开摘要不包含凭据、设备地址和客户图片。

## Capabilities

### New Capabilities

- `performance-evidence`：最小可复现观测与基线的可验证行为与性能边界。

### Modified Capabilities

无已有主规格；不修改宿主业务或既有公开 ABI 的兼容语义。

## Impact

仅影响 FrameBraid 核心、受影响测试、基准及文档。无前置阶段；R5 优先项和 R2 必须复用本包原始基线。 不修改 AM-RT，不自动发布或增加平台承诺。
