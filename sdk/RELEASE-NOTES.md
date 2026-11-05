# FrameBraid Windows candidate / Windows 候选说明

## v0.6.0 (2026-11-05)

Compatible-addition release, **C ABI 1.2 → 1.3** (`AM_ABI_VERSION` 65538 → 65539). Three integrator-confirmed capabilities, no breaking changes — existing consumers keep working via struct_size negotiation.

- **ROI cascade inference (C1)** — `amv_infer_cascade_sync/async`: one call runs full-frame primary detect → ROI generation (static + dynamic rules with class filter and expand ratio, cap 16, confidence-desc truncation) → per-ROI crop/letterbox upscale → secondary detect → coordinate mapping back to the original frame. Independent model handles per stage; per-ROI independent NMS; single-ROI failure skips without failing the batch; `skipped_count` three-state observability (null = not cascaded / empty+0 = no hits / >0 = ROIs not executed). Vocab-bound models may serve as the secondary stage. Grounded in the integrator's measured scenario: small-object score 0.167 → 0.461, boxes 7 → 15.
- **Cross-frame object tracking** — `amv_tracker_create/update/reset/close`: SORT-grade lightweight association decoupled from inference (accepts boxes from any source: detect results, cascade secondary boxes, external detectors). Constant-velocity prediction + deterministic IoU greedy matching; monotonic never-reused track IDs; confirm-hits gate (explicit 1 = emit on first frame); TTL recycling; unsmoothed pass-through output. Parameter bounds are contract-frozen (ttl ≤ 10000, confirm ≤ 1000, max_tracks 1..4096).
- **Single-frame image export** — `amc_frame_encode`: any host-held pixel view (subscription callback / shared pool / decoder output) to JPEG/PNG bytes. Stateless, all pixel formats (NV12 via BT.601/709/2020 conversion parity with the vision side), capacity-insufficient fills required size and returns `sdk.limitOutOfRange` (no partial output), 32MiB cap.
- **Rust direct-dependency example** — `examples/vision_cascade_tracker.rs`: tracker usage (confirm=1 semantics, ID stability across frames and TTL re-acquisition) and cascade primitives (`generate_rois`/`map_box_to_original`), `cargo run` verified.

Verification: workspace 489 tests zero failures across repeat runs; cascade + vocab e2e gates over real ORT (synthetic dual models, hand-computed coordinate assertions) all pass; clippy clean on new code; source-fingerprint gate 282 green. The three capabilities compose: cascade detects small objects → tracker assigns stable IDs → frame export captures evidence.

/ 兼容新增版，C ABI 1.2→1.3。三项接入方确认能力：ROI 级联推理（一次调用完成全图检出→ROI 放大细检→坐标回映，含跳过计数三态可观测）；SORT 级跨帧目标跟踪（与推理解耦、确定性匹配、ID 单调不复用、确认/TTL 语义、输出透传不平滑）；单帧 JPEG/PNG 导出（任意 pixel view、全格式、容量不足报所需不返回半份）；另附 Rust 直依赖调用示例。验证：工作区 489 项测试两遍零失败；级联+词汇双 e2e 真机门（真实 ORT 合成双模型、坐标回映手算断言）全过；新码 clippy 零告警；指纹闸门 282 绿。三项能力可组合：级联检小目标→跟踪稳定 ID→导出留证。

## v0.3.2 (2026-11-03)

Integrator-feedback release, no ABI break. Python bindings expose the init/thread parameters that
were previously hardcoded (`infer_threads`, `total_thread_budget`, `domain_infer_slots`,
`feature_flags` on init; `intra_threads` on model load, 0 = SDK default) — measured on a 16-core
host, raising intra threads to 12 moves full-chain p50 from 63.5 to 33.1 ms, the level previously
reachable only via direct C API. The OpenVINO native engine (E1) detects the loaded runtime
version and reports detected-vs-expected version lines in failure details (feedback P0-1/P1-6).
`amv_init` out-of-range failures now carry the field name, submitted value and valid range instead
of a bare `sdk.limitOutOfRange`. Raw-tensor binding wrappers default the contiguous flag.
INTEGRATION.md gains E1/M5/S5 integration sections, and a new runtime licensing guide
(docs/RUNTIME-LICENSING.md) covers the LGPL-minimal vs GPL-encode (libx264) boundary and
distribution obligations. GitHub Release artifacts are published in full per version (binaries +
SHA256SUMS), closing the v0.3.1 gap.
/ 接入方反馈版，无 ABI 破坏：Python 绑定放开此前写死的 init/线程参数（16 核实测全链 p50
63.5→33.1ms，与直调 C API 同档）；OpenVINO 原生引擎版本检测与明确报错（失败详情带检测到
的版本与期望版本线）；init 超范围失败带字段/传入值/合法区间；raw 张量绑定默认连续标志；
INTEGRATION 增补 E1/M5/S5 接入说明；新增运行库许可选择指引（LGPL minimal 与 GPL 编码构建
边界及分发义务）；Release 制品按版本完整发布（补 v0.3.1 缺口）。

## v0.3.1 (2026-11-02)

Audit-fix release, no ABI break (prefix compatibility strengthened). Fixes review issues #2-#8:
encode/archive/stream dimension validation (no OOB reads on mismatched views), CUDA buffer RAII
(no leak on second-allocation failure), MP4 finish-error propagation (no false save-success),
RTMP connect budget with interruptible backoff (15s total, stop-wake, camera.timeout), encoded
PTS/DTS/time_base pass-through, legacy 48/40-byte struct prefix negotiation (am_image_input_t /
am_frame_sub_desc_t), C# binding regains the gpu field. Also fixes 10 same-pattern findings from
a whole-repo audit and 2 high-severity extended-audit findings (batching three-lock deadlock,
DETACH drain false-success). GPU frame handle exports return notSupported until the closed loop
lands. 402 unit tests + real-FFmpeg suite green, failure list identical to pre-change baseline.
/ 审查修复版，无 ABI 破坏（前缀兼容增强）：编码尺寸校验、CUDA 缓冲 RAII、MP4 结束错误
上抛、RTMP 连接预算与可唤醒退避、编码时间线贯通、旧版结构前缀协商、C# 绑定补齐；含全仓
模式审计同修 10 项与扩展审计高危 2 项（攒批三锁环、DETACH 假成功）。GPU 帧句柄族在闭环
落地前统一 notSupported。402 单测+真机套件全绿，失败清单与基线一致。

## v0.2.0 (2026-10-28)

Compatible-addition release. Media/Vision SDK move to C ABI 1.1 (r9 frozen artifacts): explicit
hardware decode/inference probes with bounded cached reports, RTSP decode-policy entry and
source snapshots; everything from 1.0 keeps layout and semantics. New adaptive batching extension
(`amv_model_configure_batching`/`amv_model_get_batching`, AUTO semantics, dynamic-batch-axis models
only, off by default). Hot-path improvements with bit-exact outputs (NV12 table conversion -23%,
tensor direct write, sampling cadence). Four-language examples revalidated from a clean package;
R1–R5 OpenSpec workstreams closed with verification records in docs/verification.

Known-unverified (registered, not blocking): four of five real model classes lack local weights;
CUDA/OpenVINO execution providers absent on the validation machine; field streams and fixed
long-duration runs remain out of per user decision; GPU decode under the CPU-RGBA readback
contract measured not to save CPU on the test GPUs across 1/2/4 lanes.

## v0.1.0 (2026-10-25)

Version 0.1.0 developer preview, authorized for public distribution. Media SDK and Vision SDK use
C ABI 1.0 r5; legacy DLL and symbol names remain for compatibility.

Windows x64 CPU candidate: Release build, both shared-frame features enabled.
The recorded pass covers 177 Rust tests and 11 packaged-DLL groups with
C/C++17/C#/Python calls, synthetic media fixtures and generated CPU test graphs.
All five real-weight model profiles were not rerun end-to-end in this pass.
GPU, hardware decoding, other platforms and field/long-duration acceptance are separate.

Use the package build record and artifact/runtime manifest for exact identity.
Model weights, environment files, customer data and internal logs are excluded.
Matching runtime licensing/source materials accompany the Windows candidate;
finite validation does not establish field or long-duration guarantees.

0.1.0 开发者预览版，首发面向 Windows x64 CPU。限定结果为 177 项 Rust 单测及 11 组 DLL
验证；不将其视为五类真实权重、GPU、硬解、其他平台、长期或现场验收。
以包内构建记录和制品/运行库清单核对身份，模型自行匹配档案并取得使用许可。
原宿主交付过程另存内部归档，当前验证及来源摘要见仓库 docs/RELEASE_STATUS.md。
