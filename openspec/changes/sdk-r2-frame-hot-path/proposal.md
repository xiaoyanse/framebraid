# Proposal

## Why

已有可验证热点：逐帧转换上下文、无到期消费者也物化像素、临时浮点数组再复制张量。优先减少确定的开销。

## What Changes

- 转换、采样与张量热路径，具体任务及量化条件见 tasks.md。
- 复用既有预算、所有权和 ABI；新增公共能力先同步合同和生成器，保留原冻结历史。
- 原始性能证据只放 .local，公开摘要不包含凭据、设备地址和客户图片。

## Capabilities

### New Capabilities

- `frame-processing-efficiency`：转换、采样与张量热路径的可验证行为与性能边界。

### Modified Capabilities

无已有主规格；不修改宿主业务或既有公开 ABI 的兼容语义。

## Impact

仅影响 FrameBraid 核心、受影响测试、基准及文档。R1 基线先成立；不等待 R3 全套长跑才着手局部优化，必要所有权回归本包完成。 不修改 AM-RT，不自动发布或增加平台承诺。
