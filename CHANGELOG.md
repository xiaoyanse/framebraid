# Changelog / 变更记录

## Unreleased

- Deliver the latest overlay fairly alongside video, reserving one queue entry and 512 KiB within the existing 36-entry/8 MiB limit. / 最新叠层与视频公平投递，在既有 36 项、8 MiB 上限内预留 1 项及 512 KiB。

- Identify known non-H.264 stream codecs without requiring dimensions; use a temporary bounded stream-info probe when header data is insufficient. / 已知非 H.264 编码无需尺寸即可识别；头信息不足时临时采用有界流信息探测。

## [0.2.1] - 2026-10-29

- Expose GPU staging audit counters via context stats: `am_stats_t` gains tail-appended `gpu_inputs`/`gpu_copies_per_frame` (context-scoped, not per-model), filled from the vision-layer pipeline audit in `amv_get_stats`; layout 232→248 bytes with struct_size negotiation, header/Rust/Python generators synced. / GPU 搬运审计计数经上下文统计暴露：`am_stats_t` 尾部追加 `gpu_inputs`/`gpu_copies_per_frame`（上下文级，非 per-model），`amv_get_stats` 从 vision 层管线审计填充；布局 232→248 字节，struct_size 协商，三生成器同步。

- Fix the continuous-capture beat: the in-flight acquisition fast path returned the shared-cache frame every other beat (duplicate frame-id dropped), halving the GET cadence; the fast path is now single-subscriber-transparent and the scheduler advances on a fixed grid so capture latency no longer defers the next beat. / 连续抓图节拍修复：在途获取内层缓存每逢下一拍命中旧帧（重复 frame_id 被丢弃）致 GET 频率减半；单订阅直取 + 固定栅格调度，抓取耗时不再推迟下一拍（服务器侧间隔实测 ≈ 目标间隔）。

- Serve continuous-capture cache only when a source+cadence has multiple subscribers: single subscribers previously hit the shared-cache window every beat (duplicate frame dropped), doubling the effective GET interval. / 连续抓图共享缓存仅在多订阅时启用：单订阅曾因缓存窗与节拍相位叠而每拍取旧帧，实际抓取间隔翻倍；多订阅共享语义不变。

- Decode JPEG straight to RGBA via zune-jpeg (dropping the DynamicImage RGB-to-RGBA full-image conversion): 720p 6.56->3.89 ms (-41%), 1080p 12.20->9.37 ms (-23%), 720p passes the downstream paired-harness criterion (GAP 0.79ms <= 1ms tolerance). 1080p retains a known 1.71ms gap vs the OpenCV baseline, accepted by decision (2026-10-29): keep the pure-Rust zune-jpeg kernel; no C libjpeg-turbo dependency. / JPEG 直解 RGBA（zune-jpeg）：720p 下游判据达标（GAP 0.79ms）；1080p 已知差距 1.71ms 经裁决接受（纯 Rust 决策，不引入 C 内核依赖）；EXIF 旋转语义不变，C ABI 不变。

- Raise the default camera frame budget from 96 MiB to 128 MiB and tighten the cpu-frame capture scratch reservation from 8 to 7 bytes/pixel (RGB+RGBA peak), so 2160p RGBA frames (~93 MiB transient peak) no longer hit the ceiling. / 默认帧预算 96→128 MiB、抓图转换峰值预留 8→7 B/px，2160p 帧获取不再压线（下游实测反馈）；C ABI 不变，`amc_set_limits` 仍只能收紧。

## [0.2.0] - 2026-10-28

- Add the unpublished adaptive batching compatible extension (R5-2.1): `amv_model_configure_batching`/`amv_model_get_batching` plus `am_batching_options_t`/`am_batching_stats_t`/`AM_BATCHING_ENABLE`, disabled by default and limited to dynamic-batch-axis models in AUTO semantics; windows fill, expire or drain as one physical batch with per-item request identity unchanged. / 合同见 dual-dll-abi §3.5 末；合成数据单测与生成器同步已验，真实模型组批实测待验，源码指纹修订待登记。

- Add C# Detect, Classify, Segment and OBB owned result copies with a bounded mask budget. / [局部验证](docs/verification/sdk-r4-csharp-structured-20261027.md)，真实非空模型对拍待验，C ABI不变。

- Bound file-model preflight reads to the opened size and borrow memory-model inputs during synchronous loading. / [测量与调用边界](docs/verification/sdk-r3-model-read-20261027.md)；不改变C ABI或声称原生内存有界。

- Serialize borrowed decode hardware reports into admitted fixed storage instead of building JSON trees; record fixed foundation body costs. / [验证与取舍](docs/verification/sdk-r3-report-storage-20261027.md)，C ABI和JSON字段不变。

- Make bounded model IO ownership copies fallible, release native sessions before failed-load permits, and borrow input metadata instead of cloning on each Run; use fixed source snapshots for events/statistics. / [量化和验证](docs/verification/sdk-r3-model-metadata-20261027.md)，C ABI不变。

- Remove exhaustive hardware-matrix and mandatory 4h/24h soak gates from current optimization acceptance; retain scoped functional, recovery, resource and lifetime checks. / [用户裁决](docs/verification/sdk-acceptance-scope-20261027.md)，不是将未执行测试记为通过。

- Bound pending Context initialization, source indices and shared-frame bookkeeping; use fixed CameraState/Hub registries and allocation-free subscription snapshots. / [整合验证](docs/verification/sdk-r3-foundation-integration-20261027.md)，C ABI不变。

- Admit shared-source alias backing and growth overlap before handle publication; retain idle capacity accounting and roll back shared ownership on refusal. / [定向及真实DLL验证](docs/verification/sdk-r3-alias-storage-20261027.md)，C ABI不变。

- Add owner-locked Python/C# pixel operations for frames, shared borrows and RGB inputs; explicit copy helpers preserve legacy low-level interfaces. / [验证](docs/verification/sdk-r4-owned-pixels-20261027.md)，C ABI不变，异步默认复制而非全链零拷贝。

- Account probe cache backing storage and complete result ownership before copying; preflight native model IO counts before SDK metadata duplication. / [验证](docs/verification/sdk-r3-probe-storage-20261027.md)，保持结果优先、默认预算和C ABI。

- Bound Python/C# model notifications to 256 hints with language-side drop counters; record successful native inference even when delivery is rejected. / [语言缓冲](docs/verification/r4-language-ownership-20261027.md)与[执行观察](docs/verification/sdk-h5-native-observation-20261027.md)，C ABI不变。

- Reclaim dead shared-source alias registrations even when no events occur; 256 reopen cycles retain at most two weak entries instead of 257. / 无事件共享源别名回收，见[定向验证](docs/verification/sdk-r3-source-aliases-20261027.md)，C ABI不变。

- Bound auxiliary Rust H264Hub to 16 current sources, prevent overlapping restart and stale unsubscribe, and clean up worker unwind. Exhaustive Rust `DenyReason` matches must handle `TooManySources`; C ABI unchanged. / 见[验证](docs/verification/sdk-r3-rust-hub-20261027.md)，停止不等于实际退出。

- Fix domain handle/buffer index storage and quarantine growth; bounded child-close snapshots. / 按原4096项固定域索引，隔离区先淘汰，明确冷启动/排序登记代价；见[验证](docs/verification/sdk-r3-domain-registry-20261027.md)，C ABI不变。

- Bound Context service/close-hook storage and remove temporary close lists; no public C ABI change. / 固定服务索引和关闭钩子，分配计数、固定开销与未验边界见[验证](docs/verification/sdk-r3-context-services-20261027.md)。

- Add compatible ABI 1.1 Media worker-budget setters/getters and capability, per-Context admission for all eight C ABI worker kinds, and live/peak statistics with Python/C# helpers. / 统一媒体线程准入与关闭归还，不误报0、不把工作槽当CPU核数；见[验证](docs/verification/sdk-r3-media-workers-20261027.md)，本地未发布。

- Account retained capture record/coalescing/cache/consumer/subscription index capacity and Flight bodies; complete coalesced failures before retiring their index. / 修复媒体抓图元数据漏计与缓存失败的等待者收口，保留控制项数及外持关闭边界；见[验证](docs/verification/sdk-r3-capture-tables-20261027.md)，C ABI不变。

- Charge retained inference request-table capacity and allocation overlap to Context control bytes before accepting IDs or replacing requests. / 推理请求表实际容量计费，低额度明确拒绝、保留旧请求，常驻存储不阻止关闭；见[验证](docs/verification/sdk-r3-record-storage-20261027.md)，C ABI不变。

- Account executor Job/worker-handle slots and retained Context-index capacity, reserve growth overlap, and use bounded cleanup snapshots/direct statistics folds. / 修复共享执行器常驻存储与临时列表，真实CPU回归及原始慢轮见[记录](docs/verification/sdk-r3-executor-storage-20261027.md)，C ABI不变。

- Prepay continuous-capture reference/FPS-history storage and return consumer capacity on failed registration. / 连续抓图固定存储与失败回收，见[验证](docs/verification/sdk-r3-capture-storage-20261027.md)及[剩余预算矩阵](docs/verification/sdk-r3-budget-matrix-20261027.md)，C ABI不变。

- Keep RTSP pixel-job and receipt-time container capacities within existing reserved storage; evict timing entries before insertion. / 修复36→64槽及128→256槽扩张超账，最终关联窗口不变；见[验证](docs/verification/sdk-r3-rtsp-storage-20261027.md)，无C ABI变更。

- Prepay standalone decoder input slots and timing-table storage, keep idle capacity charged, and preserve old state on reset admission failure. / 独立解码元数据预算与固定容量，默认总额约束和回归见[记录](docs/verification/sdk-r3-decoder-metadata-20261027.md)，C ABI不变。

- Reserve standalone decoder configuration, parameter caches and serialized reconfiguration storage before copying; share immutable configuration on NULL reset. Add compatible allocation-free Rust NAL visitation. / 修复低预算配置绕过和失败更新，保留C ABI及旧帧，见[验证](docs/verification/sdk-r3-decoder-retained-20261027.md)。

- Prepay fixed Media source-event storage and reclaim it after callback drain, preserving bounded drop-old hints without overwriting active callbacks. / 补媒体事件空闲容量与停止回收，详见[验证](docs/verification/sdk-r3-media-event-storage-20261027.md)，C ABI不变。

- Prepay fixed inference notification queue storage, retain idle-slot credits, and release backing buffers after callback drain. Reject undersized registration before starting a notifier. / 推理通知改固定存储预付、活项独立计数和注销/停止回收，不改变C ABI；详见[验证](docs/verification/sdk-r3-notification-storage-20261027.md)。

- Reserve RTSP configuration/framing workspace before SDK buffer allocation and retain credits through Reader errors and packet ownership. Charge complete inference notification entries; apply probe budgets to HTTP and RTSP. / 补配置与转换前准入、错误/载荷持续计费、探测预算及通知实际条目计费；兼容新增Rust预算入口，C ABI不变，见[验证](docs/verification/sdk-r3-reader-reservations-20261027.md)。

- Account standalone decoder input acceptance/backpressure, and reserve Media subscription metadata against the control budget until object disposal. / 补齐真实过载计数和订阅引用槽/预设计费，见[验证](docs/verification/sdk-r3-budget-audit-20261027.md)，无C ABI变化，完整预算审计继续。

- Reclaim ISAPI recent-image caches when the last subscriber at a sampling interval drains. Prevent late cache submission from reviving retired intervals or replacing newer frames. / 修复历史频率缓存挤占域预算，32 MiB同预算切换6→20/20、遗留缓存8→1，见[验证](docs/verification/sdk-r3-capture-cache-20261027.md)，不改变ABI或外部图片寿命。

- Add controlled slow-consumer checks for same-source media subscribers, blocked callbacks, result quotas, inference notifications and shared-frame leases. / 补齐R3局部压力隔离验证与明确配额边界，见[记录](docs/verification/sdk-r3-pressure-isolation-20261027.md)，无生产接口变更，不代替长跑。

- Add reproducible loopback C-source recovery and lifecycle checks: 20 disconnect/authentication faults, 100 real source starts/closes, capped retry waits, callback draining, request expiry and retained-result/native-batch cancellation checks. / 补齐R3恢复与启停实测，见[记录](docs/verification/sdk-r3-c-recovery-20261027.md)，不改SDK生产代码或公开ABI，不将短测当长跑。

- Bound Rust H264Hub queues by both items and payload bytes, cap GOP pre-roll items, discard broken reference chains and prioritize video over stale overlays. Make close irreversible and register async waiting before checking terminal state to prevent lost close notifications. / 修复Rust辅助分发的预算、关键帧恢复和关闭唤醒，见[验证](docs/verification/sdk-r3-rust-fanout-20261027.md)，C ABI与已发布制品不变。

- Isolate realtime/offline preprocessing byte admission by Context and queue class while retaining combined Context and shared-executor ceilings. Slow owner release no longer causes false queue-full failures in healthy classes or Contexts; excessive work still rejects and remains charged until actual release. / 修复推理分类预算串扰，见[验证](docs/verification/sdk-r3-budget-isolation-20261027.md)，未改变C ABI或发布制品。

- Admit ordered backend candidates against their own thread requirements, allowing an explicitly listed CPU fallback when OpenVINO cannot fit the shared budget. Keep legacy loading behavior and thread ceilings unchanged; validate one native failure without replay and explicit model rebuild. / 修复最昂贵候选预留阻断后续CPU，见[验证](docs/verification/sdk-h5-resource-admission-20261027.md)。

- Add ordered model selection to the local ABI 1.1 candidate: path/memory loading, bounded attempt/model diagnostics and Python/C# helpers. Explicit allowlists govern fallback; retained results own their actual-device strings. CPU and missing-EP controls are verified separately from accelerated execution. / H5有序选择接线，见[验证](docs/verification/sdk-h5-ordered-selection-20261026.md)，未发布。

- Extend capped sampling verification to HEVC/MJPEG and three-codec spatial changes; validate observer codec/dimensions before opening sources. No production ABI change or new release support claim. / 补齐采样编码与尺寸覆盖，见[记录](docs/verification/sdk-r2-codec-sampling-20261026.md)。

- Restore capped RTSP pixel delivery using strict integer deadlines, bounded worker-local waiting and sampled-only stale-picture coalescing. Three paired local H.264 runs improve a 30-FPS consumer from roughly 16 to 29.5 FPS, with higher but bounded measured receipt age; full codec/hardware/soak acceptance remains open. / 修复实际采样节拍，保留延迟代价与完整矩阵未验项，见[验证](docs/verification/sdk-r2-cadence-20261026.md)。

- Isolate RTSP native pixel work from compressed packet reading with one lazy worker per source, admitted by the existing decoder limit. Bound and account its queue, resynchronize on overflow, validate epochs/configuration before publishing and drain actual work before close. Controlled blocking/lifecycle checks and local codec regressions are recorded separately from live sampling and soak results. / 有界像素执行隔离，保留真实采样不足，不声称整包耐压完成。

- Reuse the selected device and decoder permit when RTSP codec parameters change, switch only at a new random-access picture, clear execution verification and preserve retained old frames. Local H.264/H.265 CPU/D3D11VA component and loopback DLL checks pass; sampling delivery, slow-driver isolation and full fault/soak coverage remain open. / RTSP参数重建不重复探测/创建设备，公开ABI和发布制品不变。

- Write image preprocessing directly into the final aligned owned tensor, removing one full temporary f32 buffer/copy. Byte/transform reference checks, allocation/paired local timings, actual CPU Pose/NV12 and retained-camera input regressions verified locally; public API/ABI, budgets and released assets unchanged. / 前处理内部直写，640×640每次少4,915,200字节临时分配与复制，局部收益不外推系统性能。

- Add loopback RTSP sampling/mixed-consumer measurements tied to a rebuilt Media DLL. Preserve observed delivery shortfalls and distinguish active queued-frame quota from post-drain leaks; no sampling-contract or performance-support change. / 补实际采样基线，保留30 FPS低交付问题，不将测试工具完成当功能通过。

- Add a frozen real-Pose CPU resource/async-response benchmark and isolated strict/fallback controls for missing accelerator EPs. Preserve actual DLL identities and raw A/A variation; no new selection ABI, accelerator execution or speedup claim. / 补真实模型资源基线与缺EP负控制，未扩大支持范围。

- Add a separate MET-2 paced-GOP decoder measurement tool: process CPU time, bounded per-PID/LUID GPU-engine sampling, in-flight accounting and A/A summaries. Local single-stream H.264 CPU/native controls only; no SDK/ABI change or proven offload gain. / 新增量化工具及真实GOP资源对照，保留旧数据，不改变SDK和公开制品。

- Revise performance acceptance (MET-2): distinguish CPU offload, same-path regression, mechanism cost, useful delivery and recovery; retain raw data/historical alerts and annotate prior verdicts. Documentation/OpenSpec only, no code, thresholds, benchmark runs or public assets changed. / 重整量化目标及历史评价，未新增性能实测。

- Add ABI1.1 r8 explicit inference-device probes and bounded cached JSON reports, with Python/C# helpers. Preserve all existing layouts, initialization and model-selection contracts; no ORT needed for device-only use. Local native C/C#/Python and shutdown/budget checks only; not accelerated inference or release acceptance.


Rust 后端 query 不再隐式探测 CUDA；先显式 probe 再传递快照，行为迁移见[验证](docs/verification/sdk-h5-rust-query-20261027.md)。

C# 新增 Pose 框和关键点安全复制；既有 BorrowView 保持借用，见[验证](docs/verification/sdk-r4-csharp-pose-copy-20261027.md)。

Python 检测、分类、分割和旋转框结果支持有界深拷贝；[合成结构验证](docs/verification/sdk-r4-python-structured-copy-20261027.md)，真实模型对拍仍待。

Python 新增拥有者锁保护的 Pose 框及关键点复制；结果释放后复制值仍有效，见[验证](docs/verification/sdk-r4-python-pose-copy-20261027.md)。

旧 C 后端矩阵改读显式 probe 缓存；调用方需先探测，未知与无设备由 note 区分。[验证](docs/verification/sdk-h5-matrix-cache-20261027.md)

最近邻平面缩放减少重复坐标和通道转换；像素逐字节对拍及限定局部量化见 [验证](docs/verification/sdk-r2-nearest-scale-20261027.md)。

NV12 转换乘法预计算进进程级 RG 查表，1080p 每帧约 8.8→6.8ms（-23%）；定点/SIMD 重写因 256³ 色域存在精确半整数边界被穷举否决，RG 表与冻结 f64 参考逐位一致（256³ 穷举+200 万随机块对拍）。C# 跨语言共享帧搬运量化一并登记。[验证](docs/verification/sdk-r2-crosslang-simd-20261027.md)

硬解报告增加后端编译与编码配置快照；Rust SelectionReport 结构字面量需补字段，C ABI 布局不变。[验证](docs/verification/sdk-r2-backend-support-20261027.md)

R2 硬件自动选择保留编译、适配器和设备初始化失败原因；现有 JSON 的原因分类更精确，调用方须兼容未知分类。详见 [验证](docs/verification/sdk-r2-hardware-reasons-20261027.md)。
- Add bounded Rust inference backend policies, explicit Windows CUDA driver inventory and owned session/Run diagnostics. Preserve legacy C selection behavior, shorten backend-state locks and isolate session EP/thread configuration. CPU native fallback/error and DLL regression verified locally; new C exports, accelerated runs and runtime recovery remain pending. / H5 Rust核心阶段，保留短测性能告警，未发布。

- Connect RTSP pixel subscriptions to the shared bounded hardware selector; add source-owned policy and read-only reports. Isolate pixel recovery from compressed queue/epoch resets, with next-random-access CPU recovery and bounded negative caching. H.264/H.265 hardware and software/MJPEG controls verified locally; synchronous reader isolation, device reuse on stream reconfiguration and pressure/soak acceptance remain open. Hardware RGBA readback is slower than CPU in the recorded workload; no blanket speed claim. / RTSP硬件接线与如实诊断，本地候选未发布。

- Develop C ABI 1.1 explicit decode-hardware probes and caller-owned JSON reports, preserving ABI 1.0 initialization compatibility and all existing layouts. Keep old freeze artifacts, freeze the minimum JSON contract, and add Python/C# convenience methods. Published v0.1.0 assets remain unchanged; RTSP policy and inference auto-selection remain open. / 本地1.1兼容探测扩展，非公开发布。

- Check pixel demand before stream readback/conversion while continuing compressed reference decoding; retain dimension and health handling and defer sampling-time advancement until fanout. Real H.264/virtual-clock measurements reduce native conversions without changing observed output fingerprints. Live RTSP/other codec acceptance remains open. / 像素需求前移，压缩参考链保留，未扩大公开验收范围。

- Execute the existing direct-decoder PREFER_HW policy using the shared native selector; report unknown until real output, reserve CPU recovery bytes, preserve source generations and retained frames, and permit only one unpublished startup picture replay. Explicit reset retries hardware. Validate actual codecs, readback faults and CPU compatibility locally; RTSP and rich device diagnostics remain pending. / 直接DLL自动硬解与有预算恢复，旧公开制品未变。

- Add Rust decoder reconfiguration that retains the selected native device, resets execution verification and preserves the original context on failure; explicit CPU reconstruction supports caller-controlled recovery. Native synthetic validation and construction-only timings are recorded locally; DLL automatic recovery remains pending. / 参数重建复用设备引用，失败不破坏原上下文，不扩大公开硬解支持承诺。

- Account aligned native readback buffers before allocation, retain domain reservations for a bounded single-frame cache, and reuse per-decoder SwsContext without changing pixel algorithms. Validate scoped cleanup, recovery and old-frame independence.
- 硬件回读预算适配与转换缓存已本地验证；上下文创建/释放220→1。保留初期性能告警和交替短测结论，不宣称通用提速或整链硬解完成。

- Add a shared Rust SoftwareOnly/PreferHardware initialization factory with bounded attempts, owned diagnostics and CPU fallback for busy/missing/failed hardware; verify real output separately. DLL/RTSP integration and runtime recovery remain open.
- 共用硬件选择工厂已三轮验证及故障注入，CPU默认路径不变；初始化选择不冒充目标码流与真实执行成功。

- Add explicit bounded Windows adapter/device probes and Rust H.264/H.265 hardware decoding with CPU pixel readback; default CPU and C ABI behavior remain unchanged. Hardware policy integration is still in progress.
- 增加原生设备探测与显式硬解原语，区分初始化与真实硬件帧；修正新FFmpeg构建缺少现代D3D11解码入口的问题。旧发布运行库不改写，自动选择/回退及跨语言诊断尚未完成。

- Retire canceled/replaced queued inference inputs without waiting for their busy model;
  preserve active/native-batch ownership, fairness switches and bounded execution budgets.
- R5优先项修复忙模型无效排队占额，有CPU DLL近邻A/B和生命周期回归；未新增ABI，不宣称整体吞吐提升。

- Added build-bound local performance tools for real CPU inference DLL calls and production
  Rust preprocessing/decode functions, with bounded samples and correctness/identity checks.
- 新增局部与真实 DLL 基准工具及三轮初始记录；R1 尚未整包验收，核心实现、ABI、发布制品与硬件支持声明未变。

- Consolidated local build/test/release/evidence paths under ignored `.local` categories; documented AM-RT pinned-source consumption and upgrade/rollback boundaries. SDK core/ABI and published artifacts unchanged.
- 本地目录分类与宿主固定版本接入已整理；构建/打包工具拒绝分类外输出，不改变公开 SDK 接口。

- Fixed runtime version diagnostics: Cargo package version, actual target, source commit/state,
  build profile and shared-frame feature, without changing C ABI 1.0 layouts or exports.
- Retained original extraction provenance and added an explicit source-revision overlay.

修复运行时固定开发版本文本，新增构建身份回归验证；仅本地候选，未替换 v0.1.0 发布制品。

- Added a staged optimization roadmap distinguishing existing capabilities, improvements
  and optional extensions, with soak and fault-injection acceptance criteria.
- Documentation only; SDK code, ABI, v0.1.0 artifacts and support commitments unchanged.

新增分阶段优化路线与持续运行/故障注入验收要求，仅整理文档，尚未开始实施。

## [0.1.0] - 2026-10-26

Initial public developer preview: source SDK and Windows x64 CPU binaries.

首次公开开发者预览版，提供源码及 Windows x64 CPU 二进制。

### SDK extraction / SDK 提取

- Seven-crate independent Rust workspace for Media SDK and Vision SDK.
- Apache-2.0 original code and preserved third-party notices.
- Existing DLL names and C ABI symbols retained.
- Local extraction tests recorded separately from Release acceptance.

独立七 crate，保留兼容接口和第三方许可，提取验证与 Release 验收分开。

### Open-source preparation / 开源准备

- Verified account/attribution: `xiaoyanse`.
- English/Chinese README and quick-start, support boundaries and documentation entry.
- Contribution/security guidance, issue/PR templates and future release checklist.
- Corrected current source-layout instructions.
- Added user-selected green FB logo/banner and reproducible charts from scoped historical observations, with separate extraction/Release labels.
- Added a locally validated Windows Release candidate and matching dependency materials; see docs/RELEASE_STATUS.md.
- Archived superseded preparation/extraction reports and logo exploration assets outside the public tree; retained generator inputs, ABI history and provenance.
- GitHub repository, v0.1.0 release and private vulnerability reporting.

首发包含双语说明、绿色 FB 视觉素材、匹配依赖材料及受限验证结果；历史过程资料内部归档。
