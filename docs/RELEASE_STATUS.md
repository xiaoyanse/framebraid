# Release status / 交付状态

本地Rust源码候选R3 1.1r：H264Hub源/工作占位及原Core退订修复、6项定向和301项全工作区验证见[记录](verification/sdk-r3-rust-hub-20261027.md)。Rust穷举匹配须适配TooManySources；本阶段验证Rust源码路径，不替换既有DLL或发布制品，不关闭整包硬件/长跑。

R3 1.1q本地候选固定域登记索引/隔离区，最终DLL布局/132导出、400生命周期和媒体/CPU推理压力已验；固定启动存储、排序索引局部代价及未验范围见[记录](verification/sdk-r3-domain-registry-20261027.md)。不替换发布制品。

本地未发布R3 1.1p：Context服务/关闭钩子固定容量，满额/失败不发布、通知无堆快照，机制计数与固定内存代价见[记录](verification/sdk-r3-context-services-20261027.md)。C ABI及已发布制品不变；域注册、完整矩阵/长跑仍待。

[Documentation / 文档入口](README.md) · [Release checklist / 发布清单](RELEASE_CHECKLIST.md)

Local ABI 1.1 r10 adds `amc_set/get_worker_budget`, WORKER_BUDGET capability and Python/C# helpers. Media Context SDK workers now share a 64-slot ceiling and report reserved/live counts and historic peak; real quota/isolation/Closing checks and three five-worker RTSP cycles are [verified](verification/sdk-r3-media-workers-20261027.md). / 未发布，原生驱动未知线程、Rust直接会话、完整硬件及4h/24h仍待，不能用工作槽取代OS线程数。

Local Media capture backing storage now includes request/coalescing/cadence/subscription index capacity and Flight bodies, with overlap reservation and retained idle credits. Cache errors publish a terminal result and retire the coalesced Flight. [Validation](verification/sdk-r3-capture-tables-20261027.md) covers actual low-budget admission/execution distinctions, failed-cache retries, closed-handle reuse, 53 unit tests and loopback Digest/cancellation/consumer isolation. / 未发布，不代表全部元数据、线程、硬件或长跑已验。

Local R3 inference request storage now reserves exact backing capacity and growth overlap against control bytes; rejected growth neither publishes IDs nor replaces pending requests. Idle metadata does not hold Context closing. [Validation](verification/sdk-r3-record-storage-20261027.md): 21 shared-feature/20 no-default unit tests and actual CPU batch/notify/pressure checks passed, with raw latency variations retained. / 本地未发布补请求表预算，不代表R3全路径、硬件或长跑已通过。

Updated 2026-10-26. Version 0.1.0 developer preview; Windows x64 CPU.

更新于 2026-10-26，0.1.0 开发者预览版，首发验证 Windows x64 CPU。

Local directory consolidation and host source pinning are recorded in [verification](verification/local-consolidation-20261026.md). This maintenance work does not rebuild or replace public release assets.

本地目录与宿主版本维护见上述记录；本轮没有替换公开版本。

Local bounded pixel execution is integrated ([record](verification/sdk-r3-pixel-worker-20261026.md)): native pixel calls leave the compressed reader thread, workers consume existing decoder permits, and queued/in-flight epochs are checked before publication. Controlled barriers and local codec regressions do not establish arbitrary driver-hang or soak acceptance; [Codec/size coverage](verification/sdk-r2-codec-sampling-20261026.md) now includes HEVC/MJPEG sampling and three-codec spatial changes, separately from hardware/fault/soak acceptance; the subsequent [cadence stage](verification/sdk-r2-cadence-20261026.md) restores the measured H.264 software sampling rate while full matrices remain open. / 本地隔离及采样阶段已验，未升级AM-RT或发布，不等于完整恢复/长跑通过。

Local RTSP parameter reconfiguration now reuses the selected device/decoder permit, with CPU/D3D11VA H.264/H.265 component and actual loopback DLL validation ([record](verification/sdk-r2-rtsp-reconfigure-20261026.md)). Live sampling delivery, read/decode isolation, accelerated inference and full recovery/soak acceptance remain open. / 参数重建设备复用接线已验；未发布、未升级AM-RT，不代表硬件整包完成。

Local R1 measurement tools and CPU/synthetic baselines are complete; see [R1 verification](verification/sdk-r1-complete-20261026.md). This is not R2–R5, hardware, real-model or production acceptance and does not replace public artifacts.

本地R1观测与合成基线已完成，原生硬件能力、其他优化包及长跑仍未完成；未发布、不升级支持范围。

R5 priority queue retirement is locally verified ([record](verification/sdk-r5-priority-20261026.md)); hardware selection, adaptive batching and production validation remain open. No public assets were replaced.

R5第1组排队清理本地验证完成，保留微基准告警与限制；不等于整包R5或硬件能力完成。

Local R2/H2 explicit Rust hardware foundation is verified on synthetic H.264/HEVC Main ([record](verification/sdk-r2-hardware-foundation-20261026.md)); C ABI/RTSP automatic selection, global readback budgeting and production validation remain open. Published assets unchanged.

R2/H2.0已实测原生设备与显式硬解基础路径；自动选择/回退、跨语言诊断及H5推理选择尚未完成，不扩大公开CPU支持范围。

R2/H2.0b adds a locally verified Rust initialization selector ([record](verification/sdk-r2-hardware-selection-20261026.md)); later H2.3a/b integrate direct/RTSP selection and recovery; complete pressure acceptance remains open. / Rust共用初始化选择与CPU回退已验，整包硬件能力仍未完成。

R2 H2.0c/1.1 readback reservations and conversion caches are locally verified ([record](verification/sdk-r2-readback-cache-20261026.md)); both pixel-copy entries use the budget adapter, but automatic native hardware activation/recovery and production/soak validation remain open. / 回读额度覆盖缓存寿命、转换缓存已验证，不改变公开CPU制品和未验事项。

Local R2 direct tensor preparation is verified for allocation reduction, byte/transform equivalence and CPU Pose/retained-input regression ([record](verification/sdk-r2-prepare-direct-20261026.md)); published assets and support remain unchanged. MET-2 S1/S2/S3 baselines are available, while live 30-FPS sampling delivery, accelerated inference, recovery and soak acceptance remain open. / 本地直写张量已验证，原生真实采样不足和其他阶段继续，不扩大公开支持。

## Public version / 公开版本

Local C ABI 1.1 probe/report extension is verified through C/Python/C# and ABI 1.0 initialization ([record](verification/sdk-r2-abi11-hardware-20261026.md)). Published v0.1.0 remains ABI 1.0; match DLLs and bindings by tag/build. / 本地扩展未发布，不以初始化探测替代真实码流验收，RTSP策略r7已接线，H5及完整硬件验收仍开放。

Local sampling-before-conversion is measured on real H.264 decode/production fanout with a virtual 30 FPS clock ([record](verification/sdk-r2-sampling-20261026.md)). Live RTSP timing, HEVC/MJPEG and dimension-change validation remain open; no public artifacts were replaced. / 采样前移已有局部量化证据，不作为完整1.2或RTSP硬解验收。

Local H2.3a direct C decoder automatic hardware selection and CPU recovery are verified on synthetic H.264/HEVC Main/Main10 plus unsupported HEVC Rext and injected readback failures ([record](verification/sdk-r2-direct-auto-20261026.md)). RTSP basic integration is now verified in r7; complete device diagnostics and long-run matrices remain open. / 本地直接入口验证通过，未替换公开制品，不宣称整包H2完成。

Local H2.0d device reuse is verified ([record](verification/sdk-r2-device-reuse-20261026.md)): independent contexts share an initialized device without inheriting execution verification. Construction-only measurements are not decode-throughput claims. / 参数重建设备复用已实测，底层设备复用不代替DLL/RTSP整链验收。

[Repository](https://github.com/xiaoyanse/framebraid) · [v0.1.0 downloads](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0)

The public source is a clean initial snapshot; prior local history is retained
privately. Release asset manifests bind their exact source commit, toolchain,
features and checksums. Use the release SHA256SUMS, not the historical candidate
hashes below, to verify downloaded files. Original code uses Apache-2.0; third-party
rights and model licensing remain separate.

首次公开采用精简后的初始快照，本地旧历史保留。公开制品以各自清单记录源码提交、
工具链、编译特性和指纹；下载校验使用 Release 中的 SHA256SUMS，下方是历史候选哈希。

## Local unreleased runtime identity fix / 本地未发布版本诊断修复

Runtime version text now comes from Cargo; target and source/build notes reflect compilation
inputs. C ABI 1.0 layouts/exports remain unchanged. This local fix does not replace the public
v0.1.0 assets or broaden runtime/platform acceptance. See the
[scoped verification record](verification/runtime-version-20261026.md).

已完成本地运行时版本诊断修复及定向验证；未发布、不覆盖 v0.1.0 制品，不代表重跑整包验收。
源码包无自身 Git 时明确未知，具体二进制仍按制品校验和识别。

## Historical candidate identity / 历史候选身份

- Source/build commit: `8f7fa3a444c07426a0402f931e3d390f680a5ce7`.
- Version label: `0.1.0-local`; ABI 1.0 r5; Windows x64 Release with both shared-frame features.
- Source ZIP SHA-256: `8d11d7c0b3a5ec221fdfa46b58c58ab64374b28342ce8a1c22b8151adfefda92`.
- Windows ZIP SHA-256: `16914abdf27af4133e5ce03a3d64b9348bd06aa54c90670f8cd34271931ad98a`.
- Media DLL SHA-256: `0c8442058aac35554b502d0d3bf4dc331b876f0c4f2604ba6ddc05d4d143f128`.
- Vision DLL SHA-256: `97c6705adf060a3808d3d05f0f4a5b9e2559dd4cf415b91fddcfaf396e743a75`.

Later documentation commits do not change this candidate's identity. Runtime tests
used the preceding candidate; final DLL bytes are identical. After correcting
packaged documentation links, final inventories and links were checked again.

后续文档提交不改变该候选身份。运行测试来自前一候选，最终两 DLL 与实测字节完全一致；
修正打包文档链接后，重新核验最终制品清单与链接。

## Verified locally / 本地验证

| Check / 检查 | Result / 结果 |
| --- | --- |
| Release Rust library tests | 177 passed, 0 failed, 3 ignored, 8 filtered |
| Packaged DLL groups | 11 passed: lifecycle, shared frames, native diagnostics, SDK loading diagnostics, capture, probe, decoding, notifications/statistics, language bindings, reload cycles, RTSP transport |
| Language calls | C/C++17 with `/W4 /WX`, C# and Python; CPU inference, ownership and shared frames |
| Media fixtures | Local Digest HTTP and synthetic H.264/H.265/MJPEG RTSP; no customer camera |
| Reload observation | 30 rounds, 5 warm-up; handle-count range 0; post-warm-up private-memory net change −241,664 bytes |
| Final Windows contents | 352 inventoried files, size/hash checked; 169 relative documentation/image links checked |
| Packaging regressions | 4 passed, including broken-link detection |
| Dependency materials | Matching FFmpeg source/build archives, native notices and originals for 148 resolved Cargo packages included |

Environment: Windows x64 10.0.26300.0, Intel Core Ultra 9 285H, Rust 1.97.1 MSVC,
controlled minimal FFmpeg and ONNX Runtime CPU 1.24.1. Builds reused cache;
duration is not a clean-build benchmark. Public summaries and candidate fingerprints
are retained. The original detailed private log directory is no longer at its recorded
location after cleanup; recovery of those raw logs is unconfirmed.

限定环境实测，不据此承诺固定 FPS、长期无泄漏或现场验收。
当前保留摘要及制品指纹；原详细私有日志目录清理后不在原位置，原始日志恢复尚未确认。

## Limits and remaining gates / 限制与待办

- Five-family postprocessing has unit coverage; all five frozen real-weight models
  were **not rerun end-to-end in this candidate pass**. CPU inference used a generated
  test graph. Accelerator, hardware-decoder, other OS and 24-hour/field acceptance
  remain separate.
- Source ZIP excludes runtime binaries/large external archives. The Windows candidate
  includes exact runtime materials. See [third-party materials](../sdk/third-party/README.md).
- Current history was screened for private keys, tokens and private login email;
  synthetic localhost credentials are intentional fixtures.
- Superseded preparation/extraction reports and logo exploration assets are archived
  internally, with byte-verified originals. Public release guidance describes the
  current candidate. Generator inputs, ABI compatibility history, extraction
  fingerprints and original licenses remain intact.
- Owner authorized public source and binary distribution on 2026-10-26. FrameBraid
  is the public SDK maintenance source; host dependency migration remains separate.
- GitHub private vulnerability reporting is enabled. First delivery is a developer
  preview; future publication still requires authorization.

五类真实权重、加速器、硬解、跨平台及长期/现场验收不混同本轮结果。
过程资料已归档，公开入口已精简；本次已获发布授权，宿主迁移及未来版本发布另行安排。

Local unpublished r7: [RTSP hardware verification](verification/sdk-r2-rtsp-hardware-20261026.md) covers shared policy, actual D3D11VA and CPU/MJPEG controls. Hardware availability is separate from speed; reader isolation, full hardware/pressure/soak acceptance and H5 remain open. Published v0.1.0 is unchanged. / 本地候选与旧公开CPU制品区分，H2整体未关闭。

Local H5 core: bounded Rust model selection and explicit CUDA device discovery are implemented; CPU fallback/strict and native failure diagnostics tested. Explicit C inference device probes/cached reports are also available locally; C model-selection extensions, accelerated inference and recovery acceptance remain open. See [core verification](verification/sdk-h5-selection-core-20261026.md). Hardware-vs-CPU RGBA latency alone is not a defect; CPU offload and overall budgets require measurement. / H5核心阶段已保存，未发布；不等于整包完成。

H5.0b本地探测报告与原生/语言回归详见[验证](verification/sdk-h5-inference-report-20261026.md)；新模型C策略、加速执行与恢复仍未完成。

2026-10-26 MET-2文档收口：[统一指标](PERFORMANCE_ACCEPTANCE.md)、[历史评价复核](verification/sdk-metrics-review-20261026.md)。旧R1 CPU/合成基线及阶段功能证据保留，新增量化前置尚未实施；不把改评价口径当新测试通过或解除全部性能警戒。当前不继续代码开发，公开制品与支持范围不变。

Local unpublished ABI1.1 r9 adds ordered C model selection, bounded reports and Python/C# helpers. CPU/negative-EP controls and retained-result ownership are checked in [verification](verification/sdk-h5-ordered-selection-20261026.md); accelerated execution, full fault recovery and soak remain open. Existing release assets are unchanged. / 本地阶段功能验证与性能/生产验收分开。

Local H5 resource admission fixes explicitly allowed CPU fallback under tight thread budgets, with native-failure/no-replay and explicit-rebuild checks. [Verification](verification/sdk-h5-resource-admission-20261027.md). C ABI and published artifacts remain unchanged; accelerated/fault/soak acceptance is separate.

Local R3 byte-admission isolation separates realtime/offline and Context ledgers without increasing the shared ceiling. Controlled slow-release cases improve healthy acceptance from 0/9 to 9/9 with identical outputs; overload and existing batch/close controls pass. [Verification](verification/sdk-r3-budget-isolation-20261027.md). Full-path budget audit, fault combinations and soak remain open; no new release or platform claim.

Local Rust H264Hub fanout now enforces payload/item bounds and keyframe-safe overload recovery, rejects late publication after close and avoids lost close wakeups. Thirty-two H.264 targeted tests pass with explicit FFmpeg, including local file transport and EOF generation changes. [Verification](verification/sdk-r3-rust-fanout-20261027.md). This does not reaccept C RTSP, hardware, whole-process RSS or soak behavior.

Local R3 common recovery and draining checks pass: 20 actual loopback C-source faults (10 disconnect, 10 authentication), 100 source start/close cycles plus warmup, bounded retry waits, no late callbacks or named SDK media/inference workers after drain, and real CPU expiry/retained-result/cancellation controls. [Verification](verification/sdk-r3-c-recovery-20261027.md). No production-code/API change in this stage; hardware faults, full pressure coverage and soak remain open.

Local R3 controlled isolation checks pass for same-source stalled subscriptions and callbacks, unfetched/externally held results, slow inference notifications and live shared-slot holding. Healthy consumers continue within scenario budgets; exhausted results fail explicitly and live borrows are never overwritten. [Verification](verification/sdk-r3-pressure-isolation-20261027.md). No production implementation/API changes or performance speedup claim; full-budget audit, hardware and soak remain open.

Local R3 fixes retained ISAPI interval caches: retiring the last same-interval subscriber releases its cache without stopping healthy consumers, and late submissions cannot revive retired keys or overwrite newer frames. Under the same 32 MiB budget, three runs complete 20 interval changes instead of six, with one live cache instead of eight historical entries and healthy single capture restored. [Verification](verification/sdk-r3-capture-cache-20261027.md). ABI/external-image lifetime unchanged; not an RSS or throughput claim, no release or hardware/soak acceptance.

Local R3 connects standalone-decoder admission counters and charges subscription metadata, fixed reference slots and scaling presets to the control budget until final object release. Real item/byte/domain overload and reset checks, 16 subscription tests and a short loopback isolation regression pass. [Verification](verification/sdk-r3-budget-audit-20261027.md). No ABI change or speedup claim; remaining pre-copy/notification accounting and legacy object-scope gaps are explicitly open.

Local R3 reserves SDK RTSP configuration/framing workspace before allocation, retains Reader/packet credits through errors and ownership transfer, and applies probe budgets. Under a 64-byte frame budget, old HTTP/RTSP probes returned OK; the candidate returns Pending, with no HTTP request issued. Complete notification entries are charged by actual type size (48 bytes on this host). Targeted native loopback, pressure, CPU inference and ownership checks pass; [verification](verification/sdk-r3-reader-reservations-20261027.md). Additive Rust APIs only; C ABI and published artifacts unchanged. Full metadata audit, hardware and soak remain open.

Local R3 fixes inference-notification backing storage: prepay fixed slots, keep idle capacity charged, count live entries separately without duplicate bytes, and free storage after unregister/stop drains callbacks. One-control-item registration now rejects without creating a notifier; 100 actual CPU registration/inference/unregister cycles and existing pressure/barrier checks pass. [Verification](verification/sdk-r3-notification-storage-20261027.md). C ABI/published assets unchanged; Media event storage, full audit and soak remain open.

Local R3 now prepays fixed Media source-event storage, preserves drop-old hints without replacing busy callbacks, and releases backing capacity after callback drain. Low-budget rejection, 100 registrations reusing one thread, five targeted tests and three actual loopback pressure runs pass; [verification](verification/sdk-r3-media-event-storage-20261027.md). C ABI/published assets unchanged. Full-path closure, hardware matrices and soak remain open.

Local R3 reserves standalone-decoder configuration/cache/serialization bytes before allocation and shares immutable configuration on NULL reset. Real C oversized create/reset now reject without changing old generations/frames; H.264/H.265 B-frame, queue, local hardware-recovery and RTSP pressure controls pass. Additive Rust NAL visitor removes list allocations only; no C ABI/public artifact change. [Verification](verification/sdk-r3-decoder-retained-20261027.md). Full-budget audit, hardware matrices and soak remain open.

Local R3 now prepays standalone decoder input-slot/timing-table storage and keeps idle slots charged until destruction. Real C undersized create/reset rejection, retained frames and continued decoding, 43 default tests and local native recovery pass. The 64 KiB control ceiling remains; simultaneous independent maxima are not guaranteed. [Validation](verification/sdk-r3-decoder-metadata-20261027.md). No ABI/public release change; remaining metadata paths and soak remain open.

Local R3 fixes RTSP container capacity beyond reserved credits: exact pixel-job slots and eviction before insertion into the 128-entry receipt table. Two host-local backing buffers total 3200 instead of 6144 bytes, without changing the final timing window or budgets. 45 default tests, a native sampling/resize check and real loopback pressure pass. [Validation](verification/sdk-r3-rtsp-storage-20261027.md). Full audit, codec/hardware matrices and soak remain open; no ABI/public release change.

Local R3 prepays continuous-capture reference slots and 120-entry FPS history, fixes registration rollback, and keeps idle storage charged. Real 512-byte start now rejects without a subscription or HTTP request; 48 default tests and Digest/slow-callback/cache regressions pass. [Verification](verification/sdk-r3-capture-storage-20261027.md). The [budget matrix](verification/sdk-r3-budget-matrix-20261027.md) retains explicit BP10/BP12 gaps; no full-audit, hardware/soak or release acceptance.

Local R3 accounts fixed executor Job/worker-handle slots and retained Context-index capacity against the existing global ceiling, including growth overlap before ID commit. Bounded stack snapshots/direct folds replace temporary Vec lists. 18 unit tests and CPU isolation/batch/cancellation/notification/pressure controls pass. Initial latency spikes and bounded neighboring comparisons remain in the [record](verification/sdk-r3-executor-storage-20261027.md); no overall speedup claim. BP10 record storage, BP12 Media threads, hardware and soak remain open; no publication.
