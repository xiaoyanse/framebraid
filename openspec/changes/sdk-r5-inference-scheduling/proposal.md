# Proposal

## Why

用户要求前移 R5 可独立优先项。先修现有调度竞争与失效积压，后续在数据支持下增加可关闭组批和后端策略。

## What Changes

- 低等待、公平调度与显式组批，具体任务及量化条件见 tasks.md。
- 复用既有预算、所有权和 ABI；新增公共能力先同步合同和生成器，保留原冻结历史。
- 原始性能证据只放 .local，公开摘要不包含凭据、设备地址和客户图片。

## Capabilities

### New Capabilities

- `inference-scheduling-policy`：低等待、公平调度与显式组批的可验证行为与性能边界。

### Modified Capabilities

无已有主规格；不修改宿主业务或既有公开 ABI 的兼容语义。

## Impact

仅影响 FrameBraid 核心、受影响测试、基准及文档。第 1 组只依赖 R1；第 2～3 组依赖 R1～R4 与真实代表模型证据。 不修改 AM-RT，不自动发布或增加平台承诺。
