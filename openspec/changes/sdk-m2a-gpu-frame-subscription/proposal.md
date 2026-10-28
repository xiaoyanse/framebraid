# Proposal: SDK M2a — 媒体侧 GPU 帧订阅与显存帧池

## Summary

为 Media SDK 增加解码 GPU 帧的零回读输出路径：硬解（D3D11VA/DXVA2）产生的 GPU 纹理经共享句柄以不透明 GPU 帧句柄形式供跨 DLL 消费，配套显存帧池（有界、计费、租约）。CPU 像素订阅路径完全保留（回退与兼容）。

## Motivation

- M2（全程显存管线）的媒体侧第一步（docs/ROADMAP-PLAN.md 主线 2）；前置三环调研已全通（.local/evidence/gpu-m2-preresearch-20261028/hw-frames-path.md）
- S1~四路实测负结论根源 = `av_hwframe_transfer_data` 回读（codec.rs:460）+ CPU 转换吃掉硬解收益；旁路该点才能让 CPU 卸载可证

## What Changes

- ABI（兼容新增）：GPU 帧视图/池导出（不透明句柄 + 元数据 + 租约语义）、像素订阅新增 GPU 帧模式、显存字节预算上限与计费
- am-camera：硬解帧分流（GPU 帧路径旁路 readback）
- camera-ffi：GPU 帧订阅注册/拉取、GPU 帧池（复用共享池骨架按资源类目分账）
- 合同：跨 DLL GPU 帧句柄传递规则（同进程、共享 D3D11 设备或共享 NT 句柄；宿主不可解析内里）

## Impact

- 媒体域（am-camera/camera-ffi），与 E1/E2（vision 域）零冲突可并行
- 验收含 M2c 搬运审计挂钩点（本批预留计数接口，M2b 完成后全线贯通）
