# GPU 主线 M1 CUDA 推理验收（2026-10-28）

负责人 2026-10-28 批准第二批（GPU 主线）后 M1 的 CUDA 侧验收记录。SDK 为本仓 Release 构建（am_vision，提交 `47060b4` 后工作区，含 ABI 1.1）；运行库目录 `.local/runtimes/ort-gpu-131-final`（见下节构成与指纹）。模型 `.local/tests/r5-batch/dynamic.onnx`（合成张量，动态批量轴）；本机 NVIDIA GeForce RTX 5060 Laptop GPU（驱动 617.42，CUDA driver 13.4）。合成数据，无现场设备。

## 运行库目录构成（组装过程如实记录）

本机既有 GPU 包（050-Depth 拷贝）组件目录残缺且依赖错配（CUDA 13 构建配 CUDA 12 组件），不可直接使用。以官方形态重组：

- onnxruntime-gpu 1.31.1 官方 wheel（cp313/win_amd64）完整解包
- CUDA 13.4 组件 wheel 补齐：nvidia-cublas 13.4.1.3、nvidia-cuda-nvrtc 13.4.92、nvidia-cuda-runtime 13.4.92、nvidia-cudnn-cu13 9.27.0.42
- 延迟加载备选项：cufft64_11（nvidia-cufft-cu12 11.4.1.4）、cudart64_12（源目录传递）
- 全部 DLL 与 onnxruntime.dll **同目录**（pip 官方文档标准形态；跨目录混用会因加载顺序出现 provider 注册失败或初始化错误，已实测两类故障形态）

排障中实证的两个接口事实（对宿主有参考价值）：① SDK 运行库版本协商严格——提供低于构建绑定版本（本构建 ≥1.31）的 ORT 目录返回 `vision.runtimeLoadFailed`（1028），不允许降级混用；② 单独 `LoadLibrary(providers_cuda)` 的诊断方式不可靠（缺主库上下文时会报初始化例程失败），以 SDK 探测或官方入口验证为准。

## 验收结果（SDK 显式分层选择路径）

| 检查 | 结果 |
| --- | --- |
| `amv_probe_inference_hardware` | CUDA 驱动存在、初始化成功、枚举 1 设备（RTX 5060，UUID 核对）、耗时 46ms |
| 允许列表选择（CUDA→CPU，允许回退） | `load_selected` 成功，选中 CUDA 候选 |
| 批量真实推理 | 4 项 AUTO/NATIVE 提交全部接受并完成交付 |
| `amv_query_backends` 矩阵（CUDA 行） | hw_detected=1、deps_present=1、provider_loadable=1、**verified=1**（该后端真实推理成功，H5 语义） |

## 边界

- 单模型单次批量验收，非吞吐/延迟结论（MET-2 对照见 M1 收尾另记）；固定槽位 device 0，多 GPU ordinal 未测。
- OpenVINO 侧（M1 另一半）：实测确认 ORT 官方 GPU 包 providers 仅 TensorRT/CUDA/CPU，**不含 OpenVINO EP**——OpenVINO 验收需 Intel 渠道构建，另行获取验证后记录。
- CUDA 13 组件版本锁定 13.4 系（cublas 13.8 与 13.4 混搭实测初始化失败）；升级组件需重验。
