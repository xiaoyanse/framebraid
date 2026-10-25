# Support boundaries / 支持边界

[Documentation / 文档入口](README.md)

Interface presence, tested behavior and release commitments are separate. No support SLA is promised. See [current local evidence](RELEASE_STATUS.md).

接口存在、实测通过与交付承诺分开，不承诺支持时限。

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
