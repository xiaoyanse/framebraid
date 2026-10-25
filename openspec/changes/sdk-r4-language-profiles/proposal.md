# Proposal

## Why

低层 ABI 已有但常见流程、借用视图和模型诊断需要统一，降低调用方所有权与接入负担。

## What Changes

- 跨语言易用性与五类档案，具体任务及量化条件见 tasks.md。
- 复用既有预算、所有权和 ABI；新增公共能力先同步合同和生成器，保留原冻结历史。
- 原始性能证据只放 .local，公开摘要不包含凭据、设备地址和客户图片。

## Capabilities

### New Capabilities

- `language-model-integration`：跨语言易用性与五类档案的可验证行为与性能边界。

### Modified Capabilities

无已有主规格；不修改宿主业务或既有公开 ABI 的兼容语义。

## Impact

仅影响 FrameBraid 核心、受影响测试、基准及文档。R2/R3 所有权和错误稳定；可前置只读审计，正式封装不与核心契约并行漂移。 不修改 AM-RT，不自动发布或增加平台承诺。
