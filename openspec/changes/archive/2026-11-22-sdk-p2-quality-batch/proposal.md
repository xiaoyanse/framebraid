# Proposal: sdk-p2-quality-batch（v0.7.2 质量收尾批）

## Why

v0.7.0/v0.7.1 连续两版功能上线后遗留四笔质量债：池退出挂死（v0.7.0 承诺的首个修复候选）、
GPU 级联严格对拍失败模式的 0xc0000409 退出异常（v0.7.1 研究线发现）、GPU 级联多会话
冷启动未验证（诚实边界）、am-camera-ffi 真机门 harness 配置散佚（8/19 门阻塞，0.3.x
时代配置未沉淀）。决策人 2026-11-15 批 v0.7.2 质量收尾批 → v0.8 Intel 核显主线。

## What Changes

四项（门禁等级 L2：修复+测试债+验证补充，不动 C ABI 与主链路协议）：

- T1 池退出 detach 挂死修复：DLL_PROCESS_DETACH 路径池分片会话挂死（ExitProcess
  串行 DllMain 机制，pool_soak 以 TerminateProcess 绕过=workaround 非修复）——
  SDK 侧根治（detach 语义安全化），pool_soak workaround 保留但不再必要
- T2 严格对拍失败模式 0xc0000409：gpu_cascade 门 FP32 严格对拍 panic 后测试进程
  STATUS_STACK_BUFFER_OVERRUN 退出（成功模式无此异常）——失败路径 GPU 会话析构核查与修复
- T3 GPU 级联多会话冷启动补验：多对独立 CUDA 会话冷启动峰值+容量（诚实边界补齐），
  GPU 负载错峰执行
- T4 am-camera-ffi 门 harness 债清偿：8 门配置重建（AM_SDK_SEEK/FILE_SOURCE/
  HEVC_REXT/JPEG 样本变量映射+本轮回环 RTSP 服务器搭建）+配置沉淀文档化，19 门全绿口径

## Impact

- crates/vision-ffi（池 detach/会话析构）、crates/vision（gpu pipeline detach 语义）
- .local（T3 实验+T4 harness 配置沉淀）、POC harness（pool_soak 注释更新）
- 不动：ABI 工件（C ABI 1.5 不变）、生成物、sdk/ 交付物文档（CHANGELOG Unreleased 记录）
- 风险：T1/T2 均在退出/析构路径——修复不得引入正常运行期行为变化（既有门全绿为准）
