# Proposal: SDK E2 — DirectML 推理后端枚举扩展

## Summary

在现有 CPU/OpenVINO/CUDA 三值后端枚举中新增 DirectML（第四推理路径），使 Windows 上任意 GPU（Intel Arc 核显 / AMD / NVIDIA）均可经 DirectML EP 加速推理。运行库形态已实测可载（`onnxruntime-directml` 1.24.4 wheel，自带 DirectML.dll，API 线兼容 SDK 的 1.24 请求）。

## Motivation

- 排期主线 1 首批（docs/ROADMAP-PLAN.md）：用户确认 OpenVINO/ORT/DirectML 三路径并存
- OpenVINO EP 路线因 Intel 发布侧阻塞转入原生引擎（E1，2~3 周）；DirectML 枚举扩展量级仅 1 周，先拿快速战果
- Intel Arc 140T 核显是宿主目标硬件之一，DirectML 是其在无 OpenVINO 原生引擎时的可用 GPU 路径

## What Changes

- ABI：`am_backend_e` 枚举新增 `AM_BACKEND_DIRECTML = 3`（兼容新增，值追加不重排）
- 后端矩阵/探测/选择/回退语义覆盖 DirectML 行
- 生成器同步 C 头/Rust ABI/Python/C# 绑定
- 合同文档 dual-dll-abi.md 对应章节更新（枚举值、能力位、DirectML 限定：设备由 DirectML 运行时枚举，无 device_id 语义）

## Impact

- 兼容新增：旧调用零影响；ABI 次版本不升（枚举值追加属既有枚举域内扩展，按 §1.1 惯例）
- 涉及：sdk-abi 常量/生成器模型、vision 后端选择/矩阵、Python/C# 绑定、文档
- 验证：合同→生成器→实现→单测→真实运行库（ortdml-rt）实测→验证记录
