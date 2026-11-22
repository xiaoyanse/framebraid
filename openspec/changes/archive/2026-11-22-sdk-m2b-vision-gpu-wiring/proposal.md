# Proposal: SDK M2b — Vision 侧 GPU 帧接线（CUDA 互操作 + GPU 预处理）

## Summary

打通 M2a GPU 帧到推理引擎的最后一环：Vision 侧消费 `am_gpu_frame_t`（D3D11 纹理指针主交付物），
经 CUDA 互操作注册纹理并在 GPU 上完成预处理（NV12→模型输入张量），经 ORT IOBinding 直达
CUDA EP，全程零 CPU 回读。CPU 像素输入路径完全保留（回退与兼容）。

## Motivation

- M2（全程显存管线）的 Vision 侧步骤（docs/ROADMAP-PLAN.md 主线 2）；M2a 已收口（7/7，
  实测修订合同：纹理指针为主交付物，共享句柄平台可选）
- M1 预研已验证两环：cudaGraphicsD3D11RegisterResource 互操作、ORT IOBinding GPU 张量
  （.local/evidence/sdk-gpu-m1-cuda-20261028.md）
- 零回读端到端是 v0.3.0 主线验收（CPU 卸载从"未证"变"已证"）

## What Changes

- ABI（兼容新增）：`amv_infer`/模型输入接受 GPU 帧句柄（frame_kind 协商；am_camera 与
  am_vision 同进程时纹理指针直传）、GPU 帧能力位（AMV_CAP_GPU_FRAME_INPUT）、
  输入搬运审计计数（per-frame copies/bytes，M2c 验收主指标）
- am-vision / vision-ffi：CUDA 互操作纹理注册（cudaGraphicsD3D11RegisterResource，
  map→CUDA 数组→kernel 采样）、GPU 预处理（NV12→RGB/RGBX resize，letterbox/pad 按模型
  输入形状）、IOBinding 绑定（输入直驻显存、输出按现有 result 路径）
- 回退矩阵：GPU 帧不可用（SOFTWARE_ONLY/无 GPU 帧订阅/引擎无 CUDA）→ CPU 像素输入不变；
  互操作注册失败单帧回退并计数（不 fail 整个模型）
- 设计文档：dual-dll-abi.md 新增 Vision 侧 GPU 帧输入章节；gpu-pipeline.md L4 落地记录

## Impact

- vision 域（am-vision/vision-ffi/vision-types），不触碰媒体域；E1（OpenVINO 原生）落定后
  按 GPU 帧输入接口适配原生引擎（本 change 的输入抽象预留引擎适配位，不提前实现）
- 预处理 kernel 是新代码（CUDA C 编译期特性门控，无 NVRTC 运行时依赖）
- 门禁等级 L3（推理主链路）：关键路径单测 + 真实 GPU 端到端 + 搬运审计数字 + 深度对拍
  （GPU 路径 vs CPU 路径检测结果一致性）
