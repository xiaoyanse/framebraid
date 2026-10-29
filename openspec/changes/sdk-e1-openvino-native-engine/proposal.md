# Proposal: SDK E1 — OpenVINO 原生引擎（多引擎抽象）

## Summary

Vision 推理引入 OpenVINO 原生 C++ API 作为与 ORT 并列的执行引擎：ov::Core 直连
（read_model → compile_model → infer_request），绕开 ORT OpenVINO EP 的 provider
桥接缺陷（本机实测：Intel provider wheel 的 C++ ABI 符号在所有公开运行库缺失，
WinError 127，2026-10-28 符号级证据）。参考 aimm 项目已验证的同路线实现
（OpenVinoBackend.cpp 335 行生产实证）。

## Motivation

- 推理引擎多元化主线（roadmap 主线1）：E2 DirectML 已交付，E1 补 OpenVINO 原生线
- ORT OpenVINO EP 不可用是已证实的死路（符号级根因，非配置问题）；aimm 的原生
  C++ API 路线在本机同环境已跑通（用户指认的参考路线）
- Intel 核显机器（无独显）的推理路径：DirectML 需 DX12、CUDA 需 N 卡，OpenVINO
  CPU/GPU 设备是最广覆盖
- M5（动态换模型）依赖引擎抽象落定——E1 的多引擎抽象是前置

## What Changes

- **多引擎抽象**：vision 侧 session 层引入引擎 trait（ORT 现路径与 OpenVINO
  原生并列），selection 后端矩阵扩展 openvino-native 候选
- **OpenVINO C API FFI**：手写最小绑定（ov_core/ov_model/ov_compiled_model/
  ov_infer_request/ov_tensor，C API 而非 C++ ABI——稳定且 FFI 友好），显式加载
  openvino.dll（runtime_dirs 语义扩展）
- **五档案适配**：pose/detect/classify/segment/obb 的输入输出张量语义映射
- **回退矩阵**：openvino-native 失败回退 ORT CPU（既有多后端 selection 复用）
- ABI：AM_BACKEND 枚举新值（openvino-native=4，兼容新增）+ 运行库目录约定

## Impact

- vision 域（am-vision/vision-ffi），ORT 路径零改动（并存）
- 门禁 L3（推理主链路新引擎）：五档案合成单测 + 真实模型对拍（CPU 设备）+
  回退矩阵实测
- 风险：OpenVINO C API 的版本兼容面（锁定 2025.x 线）；C# / Python 绑定的
  backend 参数枚举同步
