# M1 OpenVINO 验收阻塞深查与替代路径评估（2026-10-28）

承接 [M1 CUDA 记录](sdk-gpu-m1-cuda-20261028.md)的 OpenVINO 阻塞登记，本轮托管完成根因定位与可行动结论。全部工具与中间物在本机 `.local` 与 `/e/temp`，未提交二进制。

## 根因（符号级实证）

Intel `onnxruntime-openvino` 1.24.1 的 `onnxruntime_providers_openvino.dll` 从 `openvino.dll` 导入 **364 个 C++ ABI 符号**（`ov::` 命名空间模板实例与 STL 迭代器，如 `fill_lp_data`、`PartialShape::begin`；C API `ov_*` 导入为 0）。对 PyPI 全部可得的 openvino 版本做符号比对：

| openvino 版本 | 缺失符号数 |
| --- | --- |
| 2025.0 / 2025.1 / 2025.2 / 2025.3 / 2025.4.0 / 2025.4.1 | 199~203（各版本间缺集几乎相同） |
| 2026.0 / 2026.1 / 2026.4.1 | 235 |

缺集跨版本稳定（2025.2 与 2025.4.1 缺集差异为 0），且 provider 构建（2026-02）晚于全部 2025 线 openvino——**这不是版本不匹配，而是 Intel 的 provider wheel 链接的 C++ 符号在任何公开发布的 openvino wheel 中都不存在**（内部 CI 构建差分）。PyPI 的 2024 线已下架、GitHub Release 无 Windows 资产、storage.openvinotoolkit.org 目录为 JS 渲染不可直接枚举、Intel conda 渠道无 onnxruntime-openvino 包——公开渠道全部排除。

## DirectML 替代路径评估（Intel Arc 上的现实选项）

已取 `onnxruntime-directml` 1.24.4（API 线与 SDK 的 1.24 请求兼容，`amv_init` 实测通过，自带 DirectML.dll）。**但 SDK 的 `am_backend_e` 枚举只有 CPU/OpenVINO/CUDA 三个值，无 DirectML**——运行库就位也无法通过现有选择接口启用。接入需要：ABI 枚举新增（合同演进）+ 生成器同步 + 选择/探测/矩阵全链适配。

## 结论与建议（按优先级）

1. **OpenVINO EP 按阻塞关闭本轮尝试**：根因在 Intel 发布侧（公开 wheel 组合不可用），非 SDK 或配置可解；不自行源码编译 onnxruntime（工作量与收益不成比）。
2. **向用户报告两个可行动选项**：(a) 等待 Intel 修复 wheel 配套（跟踪 onnxruntime-openvino 后续版本）；(b) 若 Intel Arc GPU 加速确有近期需求，走 DirectML 枚举扩展（SDK 合同变更，列入 v0.3 候选，工作量合同层小、验证层中）。
3. CUDA 侧已完整验收（3.6~4.2x 加速 + 深度对拍通过），GPU 主线的第一个可用加速路径已落地不受此阻塞影响。
