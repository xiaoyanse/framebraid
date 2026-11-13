# Support boundaries / 支持边界

> 2026-10-27用户裁决：完整硬件矩阵及固定4h/24h长跑已移出强制验收，不再作为R1～R5收口阻塞，也不改记为通过。保留已有原始证据；功能、失败回退、资源上限、所有权/取消/关闭定向回归及适用量化继续必验。硬件按实际设备和已声明范围记录实测/未测，不要求穷尽全部设备组合。历史“矩阵/长跑仍待”仅描述旧范围，不恢复已撤销门槛。

OwnedPixels protects frame/shared/RGB owners through synchronous calls and default-copy async submission; it is a language convenience layer, not a new native retain mode. Legacy raw views, external array writes, result raw views, real camera-frame hardware paths and five-family accuracy retain their separate caller obligations/validation. / 新门面不消除全部低层误用或替代硬件验收。

Local Python/C# notification buffers are bounded to 256 hints per model; overflow drops oldest and increments language-only loss counters. / 通知不是可靠事件账本，裸像素视图拥有者、五类模型/硬件及持续运行验收仍按R4/H2/H5待办处理。

Local unreleased auxiliary Rust H264Hub: 16 current sources per instance; stopping work cannot be replaced until exit. Handle `DenyReason::TooManySources` in exhaustive matches. / 与C ABI Context额度分开，不是全进程线程保证，见[记录](verification/sdk-r3-rust-hub-20261027.md)。

[Documentation / 文档入口](README.md)

Interface presence, tested behavior and release commitments are separate. No support SLA is promised. See [current local evidence](RELEASE_STATUS.md).

接口存在、实测通过与交付承诺分开，不承诺支持时限。

v0.6.0 adds C ABI 1.3 compatible surface (ROI cascade inference `amv_infer_cascade_sync/async` with per-ROI skip observability; cross-frame object tracking `amv_tracker_*` SORT-grade association with confirm/TTL semantics; single-frame JPEG/PNG export `amc_frame_encode`). All three are integrator-confirmed; a Rust direct-dependency example ships in `examples/vision_cascade_tracker.rs`. / v0.6.0 新增 C ABI 1.3 兼容面（ROI 级联推理含逐 ROI 跳过可观测；SORT 级跨帧目标跟踪含确认/TTL 语义；单帧 JPEG/PNG 导出）。三项均经接入方确认；随包附带 Rust 直依赖调用示例（examples/vision_cascade_tracker.rs）。

v0.5.0 adds C ABI 1.2 compatible surface (FILE-source seek/duration/rate/loop; open-vocabulary binding `amv_vocab_bind` with per-box hit details). Text-tokenizer pairing and embedding-space pairing remain caller responsibilities (A3 honesty boundary); vocab end-to-end on real dual-model weights is registered as pending, covered by synthetic tests this release. / v0.5.0 新增 C ABI 1.2 兼容面（FILE 源 seek/时长/倍速/循环；开放词汇绑定与逐框命中明细）。tokenizer 配对与嵌入空间配对责任仍在调用方（A3 诚实边界）；vocab 真机双模型端到端登记待验，本版以合成测试覆盖。

Local C ABI 1.1 r10 exposes a Media worker budget per Context (default/ceiling 64, tightening only, zero disables new SDK workers). Counts cover the eight C ABI worker kinds, excluding caller threads and unmanaged driver threads. Direct Rust SessionPool configuration remains a separate audit path. Python/C# helpers and Windows x64 loopback checks are [recorded](verification/sdk-r3-media-workers-20261027.md); no published-platform or soak guarantee is added. / 历史峰值不随收紧清零，不能拿旧峰值与新上限误判超额。

For the current Windows x64 release, see the [release validation record](RELEASE_STATUS.md#v070-验证状态快照). It distinguishes CPU, D3D11VA hardware decoding, and the measured CUDA GPU-frame path. / 当前 Windows x64 版本的 CPU、D3D11VA 硬解与 CUDA GPU 帧路径验证范围见[交付状态](RELEASE_STATUS.md#v070-验证状态快照)。

| Area / 方面 | Current scope / 当前范围 | Limits / 限制 |
| --- | --- | --- |
| Platform / 平台 | Windows x64 | Linux/macOS/mobile and other architectures separately verified / 其他平台架构另验 |
| Build / 构建 | Rust sources and two DLL targets / Rust 源码及双 DLL 目标 | Local Windows Release candidate verified within recorded scope / 本地 Windows Release 已限定范围验证 |
| Languages / 语言 | Recorded C/C++17/C#/Python calls / 已记录实调 | Java not delivered / 不交付 Java |
| Media / 媒体 | ISAPI, RTSP H.264/H.265/MJPEG, standalone H.264/H.265 | Camera/FPS/recovery depend on workload and configuration / 相机、帧率、恢复依负载配置 |
| Local FILE / 本地文件 | MP4 H.264/H.265 initial scope, shipped since v0.4.0 / 首版口径 MP4 H.264/H.265，v0.4.0 起随包 | Seek (three modes), duration, rate and loop shipped in v0.5.0 (real-FFmpeg integration green) / seek 三档、时长、倍速与循环已随 v0.5.0 交付（真机 FFmpeg 集成全绿） |
| Direct decoder / 独立解码 | Complete Annex B AU / 完整 Annex B AU | Caller assembles fragments/converts packetization / 调用方装配分片与转换封装 |
| Vision / 推理 | Five fixed profile families and supported RAW tensor paths / 五类限定档案及支持的 RAW 张量 | Exact dtype/shape/export contracts; not all YOLO / 非全版本 YOLO |
| Acceleration / 加速 | CPU/OpenVINO/CUDA selection APIs | Backend, runtime and hardware coverage follows the release validation record / 后端、运行库与设备覆盖以当版验证记录为准 |
| Shared frames / 共享帧 | Optional, trusted local processes / 可信本机进程可选能力 | No cross-machine bus, hostile-tenant isolation or universal zero-copy guarantee / 非跨机总线、租户隔离或全链路零拷贝保证 |
| Browser playback / 浏览器 | Host supplies transport/player / 宿主提供传输与播放器 | Native packets do not make a complete browser player / 原生包不等于播放器 |
| Business / 业务 | Host owns rules, alarms, PLC, persistence and UI | Not SDK capabilities / 不属于 SDK |

## Versions / 版本

运行库兼容矩阵以本文件为唯一口径，代码内版本常量注释须指向本矩阵；`RELEASE_STATUS.md` 每版登记已验运行库版本。

The runtime compatibility matrix in this document is authoritative; code comments on version constants must point here, and `RELEASE_STATUS.md` records the runtime versions verified for each release.

v0.7.0 uses C ABI 1.4. Match DLLs, headers and bindings to the release, then query runtime capabilities. Before SDK 1.0, Rust and language wrapper APIs may evolve. / v0.7.0 使用 C ABI 1.4。按发布版本匹配 DLL、头文件及绑定，再查询运行时能力。SDK 1.0 前 Rust 和语言封装接口可能演进。

## Resource and safety limits / 资源及安全边界

Finite local tests do not establish universal FPS, camera counts, lossless queues, end-to-end latency or 24×7 reliability. Configure budgets, observe diagnostics and validate your workload. Model confidence does not certify absence of danger; safety-rated stopping needs an independently designed safety system.

有限测试不保证通用 FPS、路数、无损队列、端到端延迟或 7×24 稳定性。设置预算、观测诊断并按负载验收。模型置信度不证明无危险，安全级停机须独立安全系统。

This is a capability statement, not an additional license restriction. / 本文为能力说明，不增加许可限制。
