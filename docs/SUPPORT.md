# Support boundaries / 支持边界

> 2026-10-27用户裁决：完整硬件矩阵及固定4h/24h长跑已移出强制验收，不再作为R1～R5收口阻塞，也不改记为通过。保留已有原始证据；功能、失败回退、资源上限、所有权/取消/关闭定向回归及适用量化继续必验。硬件按实际设备和已声明范围记录实测/未测，不要求穷尽全部设备组合。历史“矩阵/长跑仍待”仅描述旧范围，不恢复已撤销门槛。

OwnedPixels protects frame/shared/RGB owners through synchronous calls and default-copy async submission; it is a language convenience layer, not a new native retain mode. Legacy raw views, external array writes, result raw views, real camera-frame hardware paths and five-family accuracy retain their separate caller obligations/validation. / 新门面不消除全部低层误用或替代硬件验收。

Local Python/C# notification buffers are bounded to 256 hints per model; overflow drops oldest and increments language-only loss counters. / 通知不是可靠事件账本，裸像素视图拥有者、五类模型/硬件及持续运行验收仍按R4/H2/H5待办处理。

Local unreleased auxiliary Rust H264Hub: 16 current sources per instance; stopping work cannot be replaced until exit. Handle `DenyReason::TooManySources` in exhaustive matches. / 与C ABI Context额度分开，不是全进程线程保证，见[记录](verification/sdk-r3-rust-hub-20261027.md)。

[Documentation / 文档入口](README.md)

Interface presence, tested behavior and release commitments are separate. No support SLA is promised. See [current local evidence](RELEASE_STATUS.md).

接口存在、实测通过与交付承诺分开，不承诺支持时限。

Local C ABI 1.1 r10 exposes a Media worker budget per Context (default/ceiling 64, tightening only, zero disables new SDK workers). Counts cover the eight C ABI worker kinds, excluding caller threads and unmanaged driver threads. Direct Rust SessionPool configuration remains a separate audit path. Python/C# helpers and Windows x64 loopback checks are [recorded](verification/sdk-r3-media-workers-20261027.md); no published-platform or soak guarantee is added. / 历史峰值不随收紧清零，不能拿旧峰值与新上限误判超额。

Local R2 introduces explicit Rust hardware primitives ([contract](design/native-hardware.md)); this does not upgrade the published Windows CPU scope or claim C ABI/RTSP automatic hardware selection is complete. / 本地硬解原语不改变公开CPU制品支持承诺，不代表C ABI/RTSP自动硬解已完成。

The local direct C decoder now selects hardware and admits CPU recovery ([verification](verification/sdk-r2-direct-auto-20261026.md)); RTSP integration, rich device diagnostics and production acceptance remain open. / 直接C入口已有本地实测，仍不扩大旧公开制品支持范围。

| Area / 方面 | Initial scope / 首发范围 | Limits / 限制 |
| --- | --- | --- |
| Platform / 平台 | Windows x64 CPU | Linux/macOS/mobile and other architectures separately verified / 其他平台架构另验 |
| Build / 构建 | Rust sources and two DLL targets / Rust 源码及双 DLL 目标 | Local Windows Release candidate verified within recorded scope / 本地 Windows Release 已限定范围验证 |
| Languages / 语言 | Recorded C/C++17/C#/Python calls / 已记录实调 | Java not delivered / 不交付 Java |
| Media / 媒体 | ISAPI, RTSP H.264/H.265/MJPEG, standalone H.264/H.265 | Camera/FPS/recovery depend on workload and configuration / 相机、帧率、恢复依负载配置 |
| Direct decoder / 独立解码 | Complete Annex B AU / 完整 Annex B AU | Caller assembles fragments/converts packetization / 调用方装配分片与转换封装 |
| Vision / 推理 | Five fixed profile families and supported RAW tensor paths / 五类限定档案及支持的 RAW 张量 | Exact dtype/shape/export contracts; not all YOLO / 非全版本 YOLO |
| Acceleration / 加速 | CPU/OpenVINO/CUDA selection APIs | Local candidate validates CPU, not GPU/hardware decoding / 当前候选仅验 CPU |
| Shared frames / 共享帧 | Optional, trusted local processes / 可信本机进程可选能力 | No cross-machine bus, hostile-tenant isolation or universal zero-copy guarantee / 非跨机总线、租户隔离或全链路零拷贝保证 |
| Browser playback / 浏览器 | Host supplies transport/player / 宿主提供传输与播放器 | Native packets do not make a complete browser player / 原生包不等于播放器 |
| Business / 业务 | Host owns rules, alarms, PLC, persistence and UI | Not SDK capabilities / 不属于 SDK |

## Versions / 版本

Version 0.1.0 is a developer preview, not a production support commitment. C ABI 1.0 and legacy symbols remain. Before 1.0, source/wrapper changes may occur; match actual DLLs, headers, bindings and capability queries, not package numbers alone. ABI changes require contract/generator/test review.

0.1.0 为开发者预览版，不代表生产支持承诺。保留 C ABI 1.0 和兼容符号，1.0 前源码/封装可能变化；按实际 DLL 匹配头、封装、能力，不只看包版本。ABI 变化须审合同、生成器与测试。

## Resource and safety limits / 资源及安全边界

Finite local tests do not establish universal FPS, camera counts, lossless queues, end-to-end latency or 24×7 reliability. Configure budgets, observe diagnostics and validate your workload. Model confidence does not certify absence of danger; safety-rated stopping needs an independently designed safety system.

有限测试不保证通用 FPS、路数、无损队列、端到端延迟或 7×24 稳定性。设置预算、观测诊断并按负载验收。模型置信度不证明无危险，安全级停机须独立安全系统。

This is a capability statement, not an additional license restriction. / 本文为能力说明，不增加许可限制。

Local unpublished r7: [RTSP hardware verification](verification/sdk-r2-rtsp-hardware-20261026.md) covers shared policy, actual D3D11VA and CPU/MJPEG controls. Hardware availability is separate from speed; reader isolation, full hardware/pressure/soak acceptance and H5 remain open. Published v0.1.0 is unchanged. / 本地候选与旧公开CPU制品区分，H2整体未关闭。

Local H5 core: bounded Rust model selection and explicit CUDA device discovery are implemented; CPU fallback/strict and native failure diagnostics tested. Explicit C inference device probes/cached reports are also available locally; Local r9 now adds ordered path/memory C selection and Python/C# helpers with per-model/per-attempt reports; accelerated inference and recovery acceptance remain open. See [core verification](verification/sdk-h5-selection-core-20261026.md). Hardware-vs-CPU RGBA latency alone is not a defect; CPU offload and overall budgets require measurement. / H5核心阶段已保存，未发布；不等于整包完成。

Local ordered-selection validation / 本地有序选择验证: [H5 r9](verification/sdk-h5-ordered-selection-20261026.md). Unpublished; no universal GPU performance claim.
