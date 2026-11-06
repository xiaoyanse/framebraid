# Release status / 交付状态

当前公开版本：**v0.6.0**（2026-11-05，C ABI 1.2→**1.3** 兼容新增：ROI 级联推理 C1、跨帧目标跟踪、单帧 JPEG/PNG 导出，三项接入方确认闭环）。历史版本 [v0.4.0](https://github.com/xiaoyanse/framebraid/releases/tag/v0.4.0)、[v0.3.2](https://github.com/xiaoyanse/framebraid/releases/tag/v0.3.2)、[v0.3.1](https://github.com/xiaoyanse/framebraid/releases/tag/v0.3.1)、[v0.1.0](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0)。
当前本地源码与发布版一致时以 Git 提交核对；领先时以本页登记。发布流程见 [发布清单](RELEASE_CHECKLIST.md)，逐项验证证据见 [verification 目录](verification/)（按主题命名，含过程记录）。

## 本地领先（未发布）

- **图像项自适应攒批（批 C，2026-11-07）**：图像输入项与张量项同等参与攒批（合同 §3.5 插图像项条款；聚合键加执行入口维度，CPU 图像与 GPU 帧分窗；原生批载荷携带逐项 decode 参数）；GPU 帧预处理移至接受段同步完成（合同 §3.3a 原承诺落地，纹理借用随提交调用闭合——异步 GPU 帧提交因此含毫秒级 GPU 预处理耗时）；GPU 物理批设备侧组装（D2D 拼接零 CPU 回读）。**顺带修复既有回归**：M2b 提交 `49d8a10` 起 `extract_outputs` 批轴检查误改，动态轴模型（native_batch=1）N>1 的 run_batch 整批失败——R5 张量攒批自 2026-10-29 起已损坏，本批复原并重验批合同门。**验证（真机）**：批合同 ignore 门绿（动态 1/3/64、固定 4、输出首轴漂移拒斥）；新增 FFI 门 `batch_e2e_*` 2/2 绿（四异色 PNG 经真实窗口攒批→单一物理批→逐项身份、尾项不足 2 逐项、取消语义、窗口统计）；新增 vision 门 `gpu_batch_assembly_matches_single_runs` 绿（world.onnx 四帧设备侧组装：对角 max|Δ| 1.7~6.9 vs 非对角 287~503，输出幅值 873）；vocab CPU/GPU e2e 门绿（接受段预处理无回归）；m2b-repro full 8 循环零崩。**登记未验/边界**：①现发布档案注册表无可批的 PROFILE 图像模型——单输入档案全锁 batch=1（装载期即不可批），双输入亲和档案的 2-D 文本/2-D 输出与"输出首轴=批数+逐项行数≥4+词行"互斥，PROFILE 物理批机制（CPU run_batch 合并/GPU 设备组装/逐项档案解码）已实现但真实模型 e2e 待档案注册表演进（PM 决策项，含 world-head 3-D 文本条目）；②逐项 params 的 decode 效果需 PROFILE 解码方可观测（管道已单测，效果断言随①）；③池化 8 路对照实验（multilane-scheduler bin 已就绪，world-single.onnx 夹具已生成）与 workspace 全量/clippy/fmt/边界闸门待构建窗口（夜间编译宵禁）后执行。
- **GPU 直传 vocab 双输入（批 B，2026-11-07）**：GPU 帧直传路径的开放词汇注入与亲和解码走通（嵌入按 CPU 内存绑定 IoBinding 第二输入 + run 前 `synchronize_inputs`；未绑定拒 1033 与 CPU 路径同码；单输入模型零影响）。**验证**：新增 ignore 门 `vocab_e2e_gpu_direct_injection_matches_cpu`（合成双模型 CUDA 后端，D3D 纹理 GPU 帧 vs CPU 注入对拍：置信 ≤1e-4、框坐标 ≤1e-3）**15/15 稳定**+串行/并行双跑绿；SDK 层 GPU e2e 回归绿；workspace **496 零失败**；clippy 零告警。过程中修复两个既有缺陷：`Plan::capture` 对 GPU 帧提交空指针切片（ABI 链首次 GPU 帧 e2e 暴露，M2b 期遗留）、`cuda_sys` 三处 `cudaMemcpy` 绑定三参签名错配（kind 读栈垃圾，此前零调用）。登记未验：vocab 注入的 H2D 搬运成本未单独计时（首版如实付，未做设备侧缓存）；真实 world.onnx 权重未测（验证基于合成亲和头）。

## v0.6.0 验证状态快照

ROI 级联推理 C1（合同 §4.1，接入方 8 点确认冻结 2026-11-05）+ 跨帧目标跟踪（§4.1b，接入方 6 点确认，参数上限入合同）+ 单帧 JPEG/PNG 导出（§2.11，接入方确认归档）+ Rust 直依赖调用示例。**验证**：workspace **489 项测试零失败、两遍稳定**（含 cascade 7 核心+1 FFI、tracker 13 核心+7 FFI、frame-export 7 核心+1 FFI）；真机门——cascade/vocab 双 e2e 合成模型真实 ORT 全断言过（静态 ROI 坐标回映手算值/动态规则 primary_index 溯源/独立阈值/跳过计数三态）；帧导出 PNG 无损往返+JPEG 质量序+NV12 矩阵换算单测过；并行测试 ctx 准入竞争修复后两遍全绿；clippy 新码零告警；check-boundary 282 全绿；C ABI `AM_ABI_VERSION` 65539。下游实证背景：小目标全图直检 0.167→ROI 放大 0.461、检出框 7→15（EXTENSION_CAPABILITIES C1）。

## v0.5.0 验证状态快照

FILE 阶段 2（seek/duration/rate/loop，合同 §2.4 阶段 2 段，接入方两轮确认锁定）+ 开放词汇 A1+A2+A4（`amv_vocab_bind` 词表绑定+亲和解码+命中明细，合同 §4.1，接入方六点确认含 pad 约束）+ 审计清偿批（P1/P3/L1/L2 修复+测试盲区 9 项回归锁，七项合同语义全过未发现真缺陷）。**验证**：workspace 447 项测试零失败（437 常规+10 真机门）；真机门——camera 侧 14+12 passed（含 seek 三模式/EOF 复活/循环重放/EXACT 过滤集成、FILE EOF 全链），1 项 D3D11 并行 flaky 单跑通过（既有已知非回归），8 项回环 RTSP/native 隔离库环境缺与基线一致；**vocab 双模型端到端已验**（合成 Gather 常量表文本塔+Mul 广播亲和头，真实 ORT：bind→注入→组内合并→hits/class_name→re-bind 换词全断言过；过程中修出三个集成缺口——load 动态维/图像入口双输入放行/hits 所有权，见提交 2e7a723）；clippy 零告警；check-boundary 270 全绿（指纹链式更新）；四机器闸门（bindings/doc-coverage/zero-semantics/freeze）过。C ABI `AM_ABI_VERSION` 65538。运行库版本：FFmpeg s5-encode（avcodec-63）。

## v0.4.0 验证状态快照

FILE 阶段 2 seek、时长、倍速和循环正在开发，尚未完成验收。

FILE 本地文件源（P1-8 阶段 1）+ 开放词汇首批（A3+B1）+ v0.3.3 制度批次三合一功能版。**三源真机回归**（新 DLL，`.local/tests/v040-regression/`）：RTSP transport 三 codec（H.264/H.265 D3D11VA verified + MJPEG CPU，16 包/慢订阅者重同步/时间基/RA 真实像素全绿）+ ISAPI 抓图全场景（jpegExact/生命周期/推拉互斥/回调屏障）；FILE 源 EOF 全链集成测试通过（真机 FFmpeg，29 包 PULL→EOF→排空 streamEnded→三段式关闭）。Rust 回归 428 项零失败；check-boundary 过闸（含 vocab 批新增 5 条目）。Framing 的 is_avcc 链自洽判定（P1-8 修复）对三源行为等价已由上述回归实证。运行库版本：FFmpeg s5-encode（avcodec-63）。

## v0.3.2 验证状态快照

本版已验运行库版本：OpenVINO 原生 2025.3.0/2026.1.0/2026.2.1；FFmpeg 以 build-record/验证记录为准。

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
