# E2 DirectML 第四推理后端验收（2026-10-28）

合同先行（dual-dll-abi.md DirectML 段 + types 文档枚举追加 AM_BACKEND_DIRECTML=3），生成器四链同步零漂移，实现覆盖 backend 枚举/键值/选择校验（backend≤3，DirectML device_id 忽略但要求 0）/session EP 注册（ort DirectML feature）/runtime 矩阵第四行（deps=目录或系统 DirectML.dll）。

## 验收结果（真实运行库，Intel Arc 140T 核显）

运行库 `.local/runtimes/ortdml-rt`（onnxruntime-directml 1.24.4 wheel，自带 DirectML.dll；API 线兼容 SDK 1.24 请求）。模型 dynamic.onnx（RAW 张量）：

| 检查 | 结果 |
| --- | --- |
| load_selected（DirectML 首选允许回退） | 成功，选中 DirectML |
| 4 项批量真实推理 | 全部接受并完成 |
| 矩阵第四行 | **deps=1 loadable=1 verified=1** |
| CPU/OpenVINO/CUDA 行 | 未受影响（域扩展无回退） |

单测：backend_key round-trip、selection 校验（backend=3 合法/device_id≠0 拒绝/backend=4 拒绝）通过。workspace 380 项测试全绿，fmt/clippy 零告警，生成器/冻结/边界检查通过。

## 边界

- DirectML 无 device_id 语义（EP 内部选卡），矩阵行 hw=1 需 DirectML.dll 可解析；不承诺具体适配器型号
- 本机 Intel Arc 单卡验收；AMD/NVIDIA 上的 DirectML 路径理论可用未实测
- 与 CUDA 后端互斥（同进程单加速域语义不变）；DirectML 与 CPU 同线加载（单目录包）
- 首版 DirectML EP 不支持 IOBinding GPU 输入（M2b 零回读暂不含 DirectML 路径）
