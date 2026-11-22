## ADDED Requirements

### Requirement: Intel 核显路径实测完备

SDK 在 Intel 核显平台（无独显）SHALL 提供完整可用路径：硬解（核显 D3D11VA）、推理（DirectML 核显 / OpenVINO 原生 CPU）、媒体软解全链；支持矩阵按实测如实标注。

#### Scenario: 核显硬解
- **WHEN** Intel 核显机器上以 PreferHardware 解码 H.264/H.265
- **THEN** 实际后端为核显 D3D11VA，帧交付与故障恢复语义与 NVIDIA 路径一致

#### Scenario: 无 GPU 推理回退
- **WHEN** GPU 推理路径不可用（无独显/驱动缺失）
- **THEN** 回退 CPU/OpenVINO 原生全链可用，回退可观测（selection 报告）
