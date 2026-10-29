# Release status / 交付状态

当前公开版本：**v0.2.1**（2026-10-29 发布，C ABI 1.1）。历史版本 [v0.1.0](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0)。
当前本地源码与发布版一致时以 Git 提交核对；领先时以本页登记。发布流程见 [发布清单](RELEASE_CHECKLIST.md)，逐项验证证据见 [verification 目录](verification/)（按主题命名，含过程记录）。

## v0.2.1 验证状态快照

v0.2.1 为兼容优化版（无 ABI 破坏）：R1 帧预算 96→128MiB、R2a/R2b Digest 缓存与
订阅节拍修复、R6 JPEG 直解 RGBA（720p 下游判据达标 GAP 0.79ms，1080p 复测进行
中）、M2c GPU 搬运审计计数。Rust 回归 387 项通过；R7（MJPEG 流源）与 GPU 零拷贝
主线属未发布扩展，不随本版。R6-1080p 保留 1.71ms 已知差距（2026-10-29 裁决：
纯 Rust zune-jpeg 内核取向，不引入 C libjpeg-turbo 依赖；720p 判据达标 GAP 0.79ms）。构建指纹见包内 build-record.json 与 SHA256SUMS.txt。

## v0.2.0 验证状态快照

| 面 | 状态 | 依据 |
| --- | --- | --- |
| Rust 回归 | 384 项通过 / 20 项环境门槛忽略 | workspace 全量（fmt/clippy 零告警） |
| ABI 与生成物 | 冻结 11 工件；C 头/Rust/Python/C# 生成链零漂移 | generate-abi/-rust-abi/-python-abi --check |
| 来源完整性 | 239 核心文件指纹核查通过 | check-boundary |
| 包级验证 | 四语言示例（C/C++17/C#/Python）+ 攒批六场景在干净包端到端通过 | [打包复验](verification/sdk-r5-clean-bundle-20261028.md) |
| 硬件解码 | H.264/H.265 360p～4K（含 Main10）D3D11VA 实测全过；断流恢复/在途关闭矩阵通过 | [矩阵](verification/sdk-r2-h24-matrix-20261027.md)、[故障矩阵](verification/sdk-r2-h23-fault-matrix-20261027.md) |
| 资源对照 | 单/双/四路 MET-2 对照：CPU RGBA 回读合同下硬解不省 CPU（如实负结论，1/2/4 路一致） | [单路](verification/sdk-r2-current-s1-20261027.md)、[双路](verification/sdk-r2-h24-dual-20261027.md)、[四路](verification/sdk-r2-h24-quad-20261028.md) |
| 自适应攒批 | 合成单测 + 真实 ORT 六场景通过（两项调度死锁已修复）；小模型无显著吞吐差异（如实保留） | [实测](verification/sdk-r5-batching-live-20261028.md)、[对照](verification/sdk-r5-scheduler-compare-20261028.md) |
| 模型档案 | Pose 真实图对拍通过；Detect/Classify/Segment/OBB 无本地权重未验 | [档案收口](verification/sdk-r4-ownership-closeout-20261027.md) |
| 推理后端 | CPU 全链验证；CUDA/OpenVINO 分层诊断齐备但无 EP 实测 | [分层收口](verification/sdk-h5-layered-closeout-20261027.md) |

## 未验边界（不阻塞使用，选用前自查）

- 四类模型真实权重端到端；CUDA/OpenVINO 执行后端；现场相机码流
- 多路（>4）解码、跨进程崩溃恢复、固定时长长跑（2026-10-27 用户裁决移出强制门槛）
- GPU 原生输出（非 CPU RGBA 回读合同）下的硬解收益

## 历史候选与制品

- 公开制品：v0.1.0 / v0.2.0 GitHub Release（校验以 Release 附带 SHA256SUMS 为准）
- 本地开发候选：`.local/releases/candidates/`（含 r4clean/r5clean 过程候选，非公开下载依据）

## 历史验证流水归档说明

2026-10-26～28 优化期间的逐批验证播报已并入上方快照与 [CHANGELOG](../CHANGELOG.md)；原始过程记录保留于 [verification 目录](verification/)（工作透明，不作为发布验收替代）。R1～R5 OpenSpec 工作流全部收口，入口见 [OpenSpec](../openspec/README.md)。
