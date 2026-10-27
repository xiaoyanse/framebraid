# H5.1/H5.3/H5.4 分层状态与故障路径收口核对（2026-10-27）

本记录为既有实现与既有证据的合同对照收口，不新增运行验收；核对基于 `main` 提交 `0e1069e` 时点，引用记录均已在此前提交。本机现实：Vision 运行库为 CPU-only ORT（`.local/releases/candidates/v0.1.0/windows-x64/runtime/onnxruntime-cpu`），无 CUDA/OpenVINO EP；显示适配器 Intel Arc 140T + RTX 5060 Laptop（不等于推理可用）。

## H5.1 五层状态分层核对

| 层 | CPU | CUDA | OpenVINO |
| --- | --- | --- | --- |
| 设备发现 | 逻辑并行度报告（[H5.0b](sdk-h5-inference-report-20261026.md)） | 系统驱动显式枚举（不依赖 nvidia-smi），本机实测 RTX 5060 一轮 48ms、最多 16 ordinal、进程单轮准入（[H5.0a](sdk-h5-selection-core-20261026.md)） | 文件级可用性探测：核心 DLL 旁提供器伴随文件与 OpenVINO 运行库齐全才可选（`vision::backend`） |
| 依赖 | 运行库显式目录加载，缺失有诊断 | 本机 CPU-only ORT 无 CUDA EP——**缺失**，负向实测（严格拒绝/允许回退 CPU，[H5.0d](sdk-h5-ordered-selection-20261026.md)） | 同 CUDA：本机缺 EP，按可尝试列表进入后由加载失败路径处理 |
| provider 加载 | 已验证（大量 CPU 推理记录） | 不可用路径实测：允许列表无 CPU 且严格时拒绝；含 CPU 时回退加载 | 同属 CPU-only 负向控制范围；线程/预算准入失败不隐式绕开（[H5.2](sdk-h5-resource-admission-20261027.md)） |
| 目标模型会话 | 会话池受线程预算，坏模型拒绝（H5.0d） | 无 EP 时不建 CUDA 会话；会话/Run 分层快照 | 同 CUDA 侧负向；"OpenVINO 仍按当前 CPU 合同，不暗增 GPU 支持"（合同 §1.1） |
| 真实推理 | CPU 完整实测（Identity/Pose 多记录，含 MET-2 三轮基线 [S3](sdk-met2-s3-20261026.md)） | **未测**（驱动发现≠ORT执行，如实标记；不把设备存在当 provider 成功） | **未测** |

禁止项核对：不依赖 nvidia-smi ✓（显式驱动 API 枚举）；不将有显卡等同可用 ✓（发现/依赖/执行三层分离，`executed_mask` 只在原生 Run 成功置位）；缺失/失败/未测三态分开 ✓（probe 报告 stopped/elapsed/soft_budget_exceeded 与未知快照 note 明确区分）。

**结论：H5.1 收口。** 分层状态在本机可实测范围内全部落地并区分实测/缺失/未测；CUDA/OpenVINO 真实推理按用户裁决的硬件范围规则如实登记未验，不冒充。

## H5.3 故障与重试路径核对

| 要求 | 证据 |
| --- | --- |
| 缺依赖 | CPU-only ORT 下 CUDA 请求严格拒绝、允许回退 CPU（H5.0d）；CUDA 驱动缺/失败在 probe 报告区分 stopped/Failed（[Rust 查询迁移](sdk-h5-rust-query-20261027.md)） |
| provider 加载失败 | 全局后端锁不跨原生创建，装载失败保留单加速域，同域重试/CPU 允许、失败不缓存为永久不可用（H5.0a） |
| 模型不兼容 | 坏模型加载拒绝（H5.0d）；Rext 类比同层见 H2.4b |
| 设备忙/资源不足 | 进程单轮探测准入（重叠 tryAgain）；逐候选线程准入拒绝不隐式绕开（[资源准入](sdk-h5-resource-admission-20261027.md)）；输出预算不足明确 `resourceExhausted` 且不抹去已执行事实（[原生观察](sdk-h5-native-observation-20261027.md)） |
| 运行期故障 | 原生 Gather 失败注入：`native_failures=1`、`native_runs=0`，保留错误类别；R3 故障矩阵互补 |
| 显式重试 | 查询不自动重放（连续三次查询计数不变）；调用方显式再次调用得到正确交付与 2/1 计数；探测显式刷新先清旧报告，失败不伪装成旧成功 |
| 不迁移在读输入/不重放不确定请求 | Run 只复制数值诊断，选择仅加载时执行，不迁移活请求（H5.0d 审查、native-observation） |
| 不突破单加速后端共存 | 全局后端锁保守保留单加速域（H5.0a） |

**结论：H5.3 收口。** 本机无加速 EP 下可模拟的故障与重试语义全部有定向证据；真实 GPU 忙/OOM 的硬件级故障保持未验并如实登记，不以软件准入拒绝冒充。

## H5.4 MET-2 取舍核对

CPU 必验已满足：[S3 目标卡](sdk-met2-s3-target-20261026.md) 真实 Pose CPU 三轮资源/异步响应基线。跨后端取舍在本机不可测（无加速 EP），按"其他后端分开登记"要求如实列为未验；自动选择为可解释策略（允许列表/优先级/strict/fallback，查询返回实际 provider/设备/回退原因，H5.2），不宣称自动找到最快设备。与 R3 故障（foundation-integration）及 R4 语言接入（H5.0d/0b 跨语言验证）的衔接记录在案。

**结论：H5.4 收口**（CPU 侧；GPU/OpenVINO 取舍未验边界随硬件范围裁决如实保留）。

## 未验边界

本机 CUDA/OpenVINO 真实推理、跨后端冷启/资源取舍、驱动永久阻塞及 C# 旧矩阵专项未验；不因此降低任何已验结论，也不把分层登记当作加速验收。R5 后续组批（2.1～2.3）与同候选综合验证（3.1/3.2）继续开放。
