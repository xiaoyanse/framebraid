# Release status / 交付状态

当前公开版本：**v0.3.2**（2026-11-03 发布，C ABI 1.1，[Release](https://github.com/xiaoyanse/framebraid/releases/tag/v0.3.2)；二进制构建提交 5181b39，tag 提交 76a3084 仅文档变更）。历史版本 [v0.3.1](https://github.com/xiaoyanse/framebraid/releases/tag/v0.3.1)、[v0.1.0](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0)。
当前本地源码与发布版一致时以 Git 提交核对；领先时以本页登记。发布流程见 [发布清单](RELEASE_CHECKLIST.md)，逐项验证证据见 [verification 目录](verification/)（按主题命名，含过程记录）。

## v0.3.2 验证状态快照

接入方反馈批（2026-11-03，已发布）。本批 P0-1（E1 OpenVINO 原生引擎 2026.x）：
根因为运行库发现只认未文档化的 `AM_SDK_OPENVINO_DIR` 环境变量、未接 ABI 合同的
runtime_dirs 通道——非 2026.x 符号断裂（绑定 28 符号在 2025.3.0/2026.1.0/2026.2.1
三线的 openvino_c.dll 导出表全存在，元素类型枚举/结构布局/变参签名逐项一致；2026 线
仅新增 2 个日志回调符号）。修复后单一绑定覆盖两线，**三线真机实测**（装载+编译+推理+
对拍 yolo26n-pose，matched max_rel 6.468e-4 / 3.150e-4 / 3.375e-4，容差 5e-3）+
回退矩阵三档 + runtime_dirs 接线用例（不设环境变量）全过；失败详情经 amv_get_last_error
携带检测版本与期望线。运行库存放：`.local/deps/ov2026|ov2025`（wheel 解包，仅本地验证用）。

批 D（2026-11-03）：错误详情架构——动态文本驻留下沉 sdk-support（`diagnostics::intern`
内容去重进程保活 + `Detail::dynamic`），键纪律修订为稳定键与动态详情并存；FFmpeg 版本
不匹配失败详情带具体库名/检测主版本/期望主版本（经驻留下发）。C#/Python 绑定参数对齐
（init 线程参数、intra_threads、confidence、capture deadline、PULL 队列参数、stream 主/子
码流、abi_version/magic 生成常量）。本地验证：三 crate 单测/clippy 绿 + 工作区 check 绿 +
check-boundary 过闸（270 核心源码，指纹链式登记）+ C# 全绑定 csc 语法编译通过 + Python
py_compile/生成器 --check 通过。真机 FFmpeg 版本不匹配注入复现未跑（需伪造主版本 DLL，
登记为待验，不阻塞：构造路径为纯格式化字符串，映射单测已锁定）。

## v0.3.1 验证状态快照

审查缺陷全量修复版（无 ABI 破坏，前缀兼容增强）：issue #2-#8 七项缺陷（尺寸校验防越界、
CUDA 缓冲 RAII、MP4 finish 错误上抛、RTMP 连接预算与可唤醒退避、编码 PTS/DTS 时基贯通、
旧版结构前缀协商、C# 绑定补齐）+ 全仓模式审计同修 10 项 + 扩展审计高危 2 项（攒批三锁环
挂死、DETACH 排空假成功）。GPU 帧句柄族导出在闭环落地前统一 notSupported（安全加固）。
合同语义六条增补见 dual-dll-abi.md 2026-11-01 段。Rust 回归 402 项通过；真机 FFmpeg 套件
全绿且失败清单与改动前基线逐项一致；源码修订按仓规则登记 sdk-source-revisions.json。
审计报告与待修台账：.local/audit/（F7 丢包、F5 GPU 借用窗口等后续项不随本版）。

## v0.3.0 验证状态快照

GPU 零拷贝主线全通（M2a/M2b/M2c/M3：每帧 2 次 GPU 内拷贝/0 CPU 回读，真机检测级对拍）；
五推理引擎（E1 OpenVINO 原生对拍 matched max_rel 3.15e-4+回退矩阵三档）；M5 热切换 8 线程
零失败；S5 编码推流 MP4 可播放验证（RTMP 真机待环境如实登记）；R 系列 7 项闭环；V1 消费方
验证体系（基线门禁+场景库+画像过堂）。Rust 回归 408 项通过。FFmpeg 运行库自本版起为 GPL
构建（libx264，用户 2026-10-29 裁决；x264 许可文本随包分发）。

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
