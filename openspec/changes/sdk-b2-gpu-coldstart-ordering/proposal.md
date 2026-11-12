# Proposal: SDK B2 — GPU 冷启动顺序修复 + 批 3 等槽归因（两小项一批）

## Summary

两个 B1 随臂登记项的同批处理：①GpuPipeline 初始化顺序修复——
`establish_context`（cuInit+primary 激活）前置到 PTX kernel JIT 之前，
消除"业务线程冷启动 GPU 帧路径必败"的接入方踩坑点；②批 3 等槽反常
归因（调查型任务，产出=归因结论+微修复或登记）。

## 项 1：GpuPipeline 冷启动顺序（2026-11-12 决策人批立项）

**缺陷**（`crates/vision/src/gpu/pipeline.rs:57-70`）：`GpuPipeline::new()`
中 L60 `KernelModule::load_nv12_letterbox`（PTX JIT，内部 cuModuleLoadData）
先于 L61 `driver.establish_context()`（cuInit+primary 激活）——JIT 依赖
**首次触碰线程恰好已有 current CUDA 上下文**。非初始化线程（业务线程）
冷启动首触 GPU 帧路径必现 `vision.gpuUnavailable|CUDA驱动 模块JIT加载 失败(3)`
（探针 `POC/.../gpu-path-probe.rs` 稳定复现；B1.6 GPU 臂改造冒烟首轮
6008 项全拒实证）。仓内既有测试/e2e 均主线程首触所以从未暴露——接入方
线程模型不受控，P1 级踩坑候选。

**修法**：L60/L61 对调（establish_context 前置）+错误路径复核（两步各自
错误语义不变：cudart/nvcuda 加载失败→JIT 失败→上下文失败的原错误串
分派保持）+冷启动定向测试（新起线程首触 GPU 帧路径，断言不再报
JIT 失败）+GPU 门全量回归。

**门禁 L1-L2**：顺序调整无 ABI/合同/行为语义变化（成功路径结果一致，
仅失败场景消除）；定向测试+GPU 门回归=L2 要件。

## 项 2：批 3 等槽/窗口轮转交互归因（调查）

B1.6 CPU 批 3 臂反常：双 Model 批 3 全凑满（timeout=0、N=3.00）却每批
周期 355ms（批 4 为 131ms），吞吐 20.1 远低于预期。怀疑 exec_stage 等槽
队列与攒批窗口轮转的交互（等槽批占住聚合键/窗口重开时机）。产出：
归因结论（实现级取证）+微修复（若为小缺陷）或如实登记（若为语义固有）。
不阻塞任何在途项；调查不改变 B1 已验收结论（批 3 形态本就不推荐）。

## Impact

- 范围：crates/vision（pipeline.rs 顺序）+定向测试；项 2 或涉
  vision-ffi exec_stage（视归因结论）。零 ABI/FFI/生成物。
- 风险：极低（对调两行+测试）；项 2 若触发修复按其性质另行定级。
