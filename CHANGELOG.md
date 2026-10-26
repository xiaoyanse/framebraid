# Changelog / 变更记录

## Unreleased / 未发布

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

## 0.1.0 — 2026-10-26 / 开发者预览版

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
