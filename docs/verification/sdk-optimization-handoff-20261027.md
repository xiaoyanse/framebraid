# FrameBraid SDK 优化交接说明（2026-10-27）

## 1. 交接范围和基线

本交接面向后续接手 FrameBraid Media SDK 和 Vision SDK 优化的开发团队。SDK 唯一维护仓库是 `E:/projects/framebraid`；AM-RT 是独立宿主，未由本次交接修改。用户要求停止本团队继续优化，后续交给接手团队。

交接核查时 FrameBraid 位于 `main`，源码基线为 `35046d37c57267d7d62dd350297f1064ac38fb8f`，工作区无未提交修改，仅主工作树登记。本文所属提交从 Git 历史查，不在正文回填自身提交哈希。核查时本地 `origin/main` 跟踪引用为 `9079b0d264b83cfd0e04c4af5c3ed978a09a14c4`，本地 main 比它超前 24 个提交；这只是本机跟踪引用状态，本次未联网刷新，不能据此断言远端实时状态。未推送、打标签或发布。

只读核对 AM-RT 根 `Cargo.toml`：三个 FrameBraid 依赖同为上述 `9079b0d...` 完整提交；FrameBraid 后续本地提交不会自动进入宿主。是否同步远端和升级宿主需按用户授权与两仓维护规则另行处理；不要将不可获取的本地提交填入公共 Git 依赖，也不要恢复 SDK 源码副本。此前宿主中间版接入记录见 AM-RT `docs/verification/verification-sdk-intermediate-20261027.md`，接手时按实际宿主版本重新核查。

## 2. 首先阅读

1. [仓库规则](../../AGENTS.md)、[OpenSpec 入口](../../openspec/README.md)和[优化路线图](../ROADMAP.md)。各 change 的 `tasks.md` 是进度真相源；`ROADMAP.md` 部分历史登记滞后，不能仅凭其“未开工”等旧描述判断当前状态。
2. [MET-2 性能与验收口径](../PERFORMANCE_ACCEPTANCE.md)和[验收范围调整](sdk-acceptance-scope-20261027.md)。每个候选先固定输入、计时边界、主目标和保护指标，保留 A/A 波动和原始数据；不以单帧快慢代替 CPU 卸载或有效交付。
3. [本地目录](../LOCAL_LAYOUT.md)、[源码维护](../SOURCE_MAINTENANCE.md)、ABI 和生成器约定。不要覆盖 `sdk-source-manifest.json` 历史原件或外部 `.gitignore`；核心变更按仓库规则维护来源修订记录。
4. 执行 `git status -sb`、`git worktree list --porcelain` 和 `openspec list --json`，重新核对工作区及当前提交。并行可写任务必须隔离，由集成方审查和真实 merge；仅本地阶段提交不等于推送或发布。

## 3. 当前阶段状态

| 包 | OpenSpec 核查结果 | 实际边界 |
| --- | --- | --- |
| [R1 性能基线](../../openspec/changes/sdk-r1-performance-baseline/tasks.md) | 9/9 登记完成 | 基线/目标卡见 [R1 记录](sdk-r1-complete-20261026.md)；不代表其他包已通过。 |
| [R3 有界恢复](../../openspec/changes/sdk-r3-bounded-recovery/tasks.md) | 35/35 登记完成 | 固定容量、资源预留、失败恢复等以各记录和 [整合记录](sdk-r3-foundation-integration-20261027.md)为准；不把计费预算解释成全进程 RSS 硬上限。 |
| [R2 热路径与 H2 硬解](../../openspec/changes/sdk-r2-frame-hot-path/tasks.md) | 16/23 完成 | 转换上下文、采样前移、张量直写、硬解基础/直接解码/RTSP 已有分项；H2.1～H2.4 和 1.4～1.6 父项仍开放。 |
| [R4 语言与模型档案](../../openspec/changes/sdk-r4-language-profiles/tasks.md) | 6/10 完成 | Python/C# 安全像素和五类结构化复制已有分项；完整调用流程、借用、五类真实模型和干净包仍开放。 |
| [R5 推理调度与 H5](../../openspec/changes/sdk-r5-inference-scheduling/tasks.md) | 11/19 完成 | 优先调度与 H5 探测、选择部分已交付；H5.1/3/4、自动组批及同候选综合验证仍开放。 |

上述数量为 OpenSpec 复选框统计，不是功能完成百分比或交付验收结论。实际实施按 `R3 已收口 → R2/H2 与 H5 → R4 → R5 → 同候选定向综合验证` 的依赖顺序集成；各项准确范围看对应 tasks、specs、design 和验证记录。

## 4. 已完成的主要成果

- **R2**：转换上下文缓存、采样前移、直接写对齐张量和最近邻缩放等已有定向性能/正确性记录；相关证据为 [张量直写](sdk-r2-prepare-direct-20261026.md)、[采样](sdk-r2-cadence-20261026.md)、[缩放](sdk-r2-nearest-scale-20261027.md)。共享帧发布、借用和 Python 显式复制仅有局部基线，见 [记录](sdk-r2-shared-copy-baseline-20261027.md)，不能外推为跨进程或端到端零拷贝收益。
- **H2 媒体硬件**：有限探测、D3D11VA 优先和可用时 DXVA2、CPU 回退已有 Rust/C 接线，直接解码与 RTSP 像素订阅已有局部验证；见 [硬解基础](sdk-r2-hardware-foundation-20261026.md)、[直接解码](sdk-r2-direct-auto-20261026.md)、[RTSP](sdk-r2-rtsp-hardware-20261026.md)及[硬件设计](../design/native-hardware.md)。探测/初始化成功不等于实际码流已硬解，硬解 CPU 回读不等于 GPU 零拷贝。
- **H5 推理硬件**：显式探测、后端实际执行观察、允许列表和 strict/fallback 选择、旧查询迁移已按对应分项记录；见 [有序选择](sdk-h5-ordered-selection-20261026.md)、[资源准入](sdk-h5-resource-admission-20261027.md)、[Rust 查询](sdk-h5-rust-query-20261027.md)。不能凭设备存在声称 CUDA/OpenVINO 已完成真实加速验收。
- **R4**：Python/C# 输出深复制和有界通知已有阶段成果。真实人物图 Pose 有 4 项有效检测：SDK 输出与 Python 独立 ORT 的有效行在限定容差内一致；完整 300 行按索引最大差 520.445，低分候选差异原因未完全确认。详见 [Pose 记录](sdk-r4-pose-real-image-20261027.md)。它不是五类模型、模型精度或所有语言全面验收。
- **共享帧安全**：真实 CPU 推理下两个 Pose 实例持有同一合成帧，单槽池满拒绝覆盖，正常及关闭取消后释放并可再发布；详见 [R2 定向记录](sdk-r2-shared-retained-pose-20261027.md)。该测试不代替跨进程崩溃和全部资源竞争。
- **R5 优先项**：已有任务公平性、取消/替换积压与健康模型排队阻塞的阶段修复和限定测量，见 [优先项记录](sdk-r5-priority-20261026.md)。当前显式批量接口的 AUTO 模式不等于自动聚合多个独立异步请求。

最近阶段提交：`98b6738` Rust 后端显式探测查询；`3e4abac` C# 结构化结果复制合并；`18a0820`、`be95ad4`、`30af6ce` Pose 复制与对拍；`35046d3` 共享帧持有/归还定向验证和硬件文档过期描述修正。详细提交以 `git log` 为准。

## 5. 接手后重点未完成事项

### R2 / H2

- 完成设备、依赖、初始化、实际码流和故障原因的分层合同/实测，以及直接解码和 RTSP 在恢复、关键帧、关闭与资源预算下的同候选验证；已做子项不得直接替代 H2 父项。
- 按相同输出和预算测 CPU 时间、有效交付、帧龄及可获得的 GPU 引擎/显存；不要求硬解 RGBA 每帧快于软件。现有 [S1 单路](sdk-r2-current-s1-20261027.md)未证明 CPU 卸载，不能外推多路或现场收益。
- 补共享帧/跨语言搬运、缩放、缓冲复用量化和 SIMD 决策；只对实测热点实施，保留标量回退和数值/边界对拍。池耗尽、活帧、取消、关闭和归还需覆盖相关路径。

### H5 / R4

- 区分 CPU/OpenVINO/CUDA 的发现、依赖、provider、目标模型会话与真实推理；完成缺依赖、失败、资源竞争、显式重试和运行期故障的范围内定向验证。未测设备明确标记，不能由显卡名称推断可用。
- R4 补齐 C/C++、C#、Python 的真实调用、所有权、错误和关闭；从匹配的干净解压 SDK 包验证。五类模型需各自固定合法权重、导出配置和档案，完成真实推理与参考对拍；缺权重如实登记，不能用单一 Pose 冒充五类。
- 已登记的媒体对象级统计合同存在缺口：`dual-dll-abi.md` §2.11 描述 source/sub/capture/decoder scope，而当前实现主要接受 context；订阅、抓图、解码器还需明确调用方可获得的稳定对象 ID、生命周期和跨 context 语义。先核合同与实现并设计兼容接口，再完成测试；不使用内存地址充当公开 ID。推理统计 scope 不应未经合同确认直接套用媒体四类。具体线索见 [R3 预算审计](sdk-r3-budget-audit-20261027.md) 和 R4 tasks 的 workflow follow-up。

### R5 和最终收口

- 设计并实现显式启用、可关闭、有界的自动组批；限定相同模型/后端/输入形状/前处理，定义等待、截止、取消、逐项回执和公平性，固定 batch 不填伪样本。新增接口先同步合同和生成器；测试不满批、过期、关闭及资源预算。
- 按 MET-2 同预算对照吞吐、逐帧 P95/P99、截止/帧龄和资源，保留失败/无收益结论。最后针对同一个明确身份的候选做跨包功能、故障、预算、所有权、取消和关闭验证，并同步绑定、示例与中英文文档。
- 用户已移除完整硬件矩阵及固定 4h/24h 长跑作为强制门槛；这不等于测试通过，也不取消定向功能、资源和恢复验证。特定实际部署需要另行确定验证范围。

## 6. 证据、环境和清理

- `.local/evidence`、`.local/tests`、`.local/build` 和 `.local/releases` 是本机忽略目录；仅公开精简记录/指纹，不提交权重、运行库、测试照片或私有数据。交接记录中使用的 DLL 各自有指纹，部分是开发构建，不能冒充干净发布候选；重新验收须固定本次源码、制品、依赖和输入。
- 先前构建缓存 `E:/projects/framebraid/.local/build/r4-csharp-pose` 的独立删除调用被自动审批返回 `blocked by policy`，目录按核查结果仍保留；规则要求停止，不能通过更换工具、编码或参数绕过。它是构建缓存，不影响源码；接手者按适用权限和规则处理。
- 不连接现场设备、不访问业务活库。测试使用隔离数据、合成源与随机回环端口；删除只限核实归属、路径、链接及在用状态后的测试产物，保护用户和其他团队成果。
- 接手时若本地跟踪引用/工作区已有变化，先核实际状态，避免覆盖；对外推送、打标签、发布和 AM-RT 升级均需单独取得适用授权。
