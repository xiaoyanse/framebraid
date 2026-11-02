# FrameBraid Windows candidate / Windows 候选说明

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
