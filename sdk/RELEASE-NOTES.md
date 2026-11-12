# FrameBraid Windows candidate / Windows 候选说明

## v0.6.5 (2026-11-13) — hotfix for v0.6.4

Hotfix release, **C ABI unchanged at 1.3** — swap the DLLs. Two fixes:

- **Single-frame non-batched regression (all v0.6.4 DLLs — upgrade advised)**: a length-consistency check introduced by the v0.6.4 execution-slot entry ran before the zero-share bypass; every single-frame non-batched submission (one-shot full-charge path) was misreported as `sdk.internalError` (28). Root-caused via a v0.6.3 worktree control run in the same environment (driver/ORT drift ruled out); bypass reordered ahead of the check; the four real-weights gates pass again (solo and across processes). Honest boundary: the same-process 13-gate serial run still trips the last gate at ctx initialization — pre-existing test-isolation debt, tracked separately.
- **GPU pipeline cold-start ordering**: context establishment now precedes PTX kernel JIT — a cold business thread's first GPU-frame touch no longer fails with a spurious JIT error (probe-reproducible before, regression-tested now; GPU gate family green).

Why verification missed it in v0.6.4: the B1 conservation family exercises only batched paths; single-frame coverage lives solely in ignore-gated real-ORT suites that the release gate did not run — the release checklist now lists ignore gates explicitly.

Verification: workspace suites green; am-vision 112 + am-vision-ffi 69 unit tests; all 13 ignore gates green solo/across processes; GPU event log clean. Evidence `.local/evidence/sdk-b1-20261112/` + P1 attribution record.

/ v0.6.4 热修版：①单帧非攒批回归（v0.6.4 制品全受影响，建议升级）——执行槽入口长度检查排在零份额旁路前，单帧一次性全额入账路径误报 internalError(28)；v0.6.3 worktree 同环境对照定案，旁路重排后 4 真实权重门全绿（单跑/跨进程）。②GPU 管线冷启动顺序：上下文建立前置到 PTX JIT 前，业务线程冷启动首触不再伪 JIT 失败（回归测试覆盖）。验证与盲区说明同上英文段。

## v0.6.4 (2026-11-12)

Optimization release, **C ABI unchanged at 1.3** — swap the DLLs to pick up the changes.

- **Multi-model ctx budget (sdk-b1): staggered execution slot + estimate refinement** — multi-model deployments share one 64MiB preprocessing budget envelope; CPU-pixel submissions whose execution shares (24B/px) stack across models were 74-75% admission-rejected at batch 3/4. A ctx-level execution slot now serializes execution-phase share accounting across models (a waiting batch holds only its conserved parked share; GPU-direct batches with zero exec share bypass the slot). The encoded-path transient source plane leaves the exec charge (serial prepare, zero co-existence, decoder-side hard cap retained). Guardrail semantics unchanged (envelope total / three layers / full-cap rejection / conservation) — verified by the extended batch-E conservation family plus a new dual-model staggered e2e gate.
- **Quantified (dev-machine GPU, 8 lanes 25fps, ORT 1.31/CUDA 13.4.92, driver 621.14)**: CPU-pixel dual-model batch 3/4 unlock (rejects 75%→0; throughput 20.1/56.5 — staggering buys availability, not speed; CPU-channel optimum stays batch 2 at 97.7); **GPU-frame dual-model batch 4 = 194.8 items/s, zero rejects — 97.4% of the 8-lane 200 demand, effectively full-rate**, superseding the CPU-scoped "8 lanes unreachable" assessment. Single-model regression anchor 56.3 vs 57.9 (2.8%, within noise). Per-channel multi-model guidance added to INTEGRATION (GPU-direct batch 4 primary / CPU-pixel batch 2 secondary).
- Honest boundaries: the batch-3 anomaly (100% fill yet slow cycle) is attributed-unresolved and registered for follow-up; a GpuPipeline cold-start ordering fragility (PTX JIT before context establishment on first-touch threads) was found by the harness and registered as a follow-up candidate — integrators hitting it can warm up from the initializing thread.

Verification: workspace 9 suites green across two runs (69 unit tests + 4 real-ORT e2e gates in am-vision-ffi); boundary gate 287 with chained source-revision registration; ABI freeze check green (no ABI artifacts touched); GPU event log clean across all quantification arms. Evidence `.local/evidence/sdk-b1-20261112/`.

/ 优化版，C ABI 不变（1.3），换 DLL 即得。多 Model ctx 预算分配：执行错峰槽（CPU 像素批 3/4 从 74-75% 准入拒绝解锁为零拒绝；GPU 直传零份额批旁路不串行化）+编码路径瞬态源面出账；护栏语义全部保持（批 E 守恒族扩展+双 Model e2e 门验证）。量化（开发机 GPU，8 路 25fps，ORT 1.31/CUDA 13.4.92 驱动 621.14）：CPU 像素双 Model 批 3/4 解锁（20.1/56.5——错峰买可用性不买速度，CPU 通道最优仍批 2=97.7）；**GPU 帧直传双 Model 批 4=194.8 items/s 零拒绝（8 路 200 需求 97.4% 实质满额）**；单 Model 回归锚 2.8% 噪声内；事件日志全臂零新增。诚实边界：批 3 反常归因登记后续；GpuPipeline 冷启动顺序脆弱性登记立项候选。验证：workspace 9 套件两轮全绿；boundary 287 链式登记；freeze 检查过；证据 `.local/evidence/sdk-b1-20261112/`。实现 `5388a78`。

## v0.6.3 (2026-11-12)

Documentation guidance release, **C ABI unchanged at 1.3 and binaries functionally identical to v0.6.2** (changes are documentation, header comments and version metadata — no code-semantics change). Existing v0.6.2 consumers need no upgrade for correctness; the guidance below is configuration advice.

- **Batching throughput-first configuration guidance (batch I)** — on CUDA backends, batch-shape stability beats per-batch waiting: alternating deadline batches (N<max) and fill batches force cuDNN replanning. Measured: 6 lanes × batch 4 × wait=40ms runs ~45% below all-fill throughput and can even drop below per-item execution; `max_wait_ms` should be the *fill timescale* (time for a full batch to arrive), not the frame period (8 lanes @25fps × batch 4: 40ms already yields 100% fill; non-divisible/under-load shapes needed 120ms for 99.9% fill). `max_wait_ms=0` is fire-on-submit: single-frame submission shapes equal per-item execution (semantics settled by code forensics: zero-width window, zero parked items, no deadlock). Shaped batches also win p50/p99 frame age under saturation.
- **Retest on driver 621.14 with ORT 1.31.1/CUDA 13.4.92** — both anchors confirmed (after the 616.56 crash chain was replaced): 8-lane wait=40 → 57.9 items/s with 1868 batches, zero deadline batches, meanN=4.00; 6-lane non-divisible trap → 34.8 items/s with the same deadline/fill alternation at meanN=3.0. The trap shape reproduces across drivers while its penalty magnitude is driver-sensitive — the fill-timescale guidance is the recommended mitigation for non-divisible lane counts.
- Honest boundary: study numbers are ORT 1.26/CUDA 12.9.79 (per-DLL ProductName; the earlier "12.8" label was an inference from the official package baseline — corrected here); throughput figures are saturated-queue steady-state, not single-session ceilings.

Verification: workspace 508 tests zero failures (code semantics identical to v0.6.2); fmt/clippy/bindings/doc-coverage/zero-semantics/boundary/freeze-abi all green — this release also clears a gate debt from v0.6.0 (zero-value-semantics annotations for the cascade/track/frame-encode structures, comment-synced through the generator chain; four cascade/tracker API names spelled out in INTEGRATION); probe + both retest arms green with zero new GPU-related event-log entries and no memory-regression signatures (full reports in the internal evidence directory).

/ 文档指导版，C ABI 不变（1.3）、二进制与 v0.6.2 功能一致（变更=文档+头文件注释+版本元数据，无代码语义变化）。v0.6.2 既有用户无正确性升级需要，以下为配置建议。本版顺带清偿 v0.6.0 起门禁债（级联/跟踪/帧导出结构体零值语义标注 20 处经生成链同步；INTEGRATION 四个 API 名展开全名）。攒批吞吐优先配置指导（批 I）：CUDA 后端下批形状稳定比单批等待更重要——截止批与凑满批交替会让 cuDNN 反复重规划（6 路×批 4×wait=40ms 比全凑满低约 45%、甚至低于逐项）；max_wait_ms 取"凑满一批的到达时间尺度"而非帧周期（8 路 25fps×批 4 实测 40ms 即 100% 凑满，非整倍形态需 120ms 恢复 99.9% 凑满）；wait=0 即入即发（单帧形态=逐项，零挂起零死等，代码取证定案）；定形批 p50/p99 帧龄同步占优。621.14 驱动+ORT 1.31/CUDA 13 复测双锚点确认（616.56 事故链更换后）：8 路 w40=57.9 items/s（1868 批零截止）；6 路陷阱交替形态复现（34.8，定形的 60%）——陷阱形态跨驱动复现而幅度驱动敏感，非整除路数按凑满时间尺度指导规避。诚实边界：研究数字为 ORT 1.26/CUDA 12.9.79 口径（按包内 DLL ProductName 订正，旧记 12.8 为官方包基准推断）；吞吐为欠载排队稳态口径非单 session 上限。验证：workspace 508 零失败（代码与 v0.6.2 一致）；fmt/clippy/bindings/boundary/freeze-abi 全绿；探针+复测两臂全绿、事件日志零新增、无内存回归迹象（完整报告见内部证据目录）。

## v0.6.2 (2026-11-08)

Fix + compatible-addition release, **C ABI unchanged at 1.3** — no new symbols or structures; swap the DLLs to pick up the changes.

- **Profile registry evolution: batch-dynamic open-vocabulary entries** — new `detect-yolo-world-batch-v1` (images batch-dynamic + 3-D text `[batch,num_prompts,512]` aligned to the image batch axis + output `[batch,rows,8400]`) and `text-encoder-clip-dynseq-v1` (dynamic sequence length) connect real YOLO-World-class weights to image batching; `decode_affinity` now takes the runtime anchors dimension (fixed declarations verified identical, semantics unchanged); CPU/GPU vocab batch injection unified byte-identical. Real-weights e2e green (GPU direct vs CPU cross-domain Pearson 0.927; batched vs per-frame ≤0.4%). Honest boundary: detection semantics quality is model-side (no tokenizer in-repo; pipeline correctness asserted).
- **Staged preprocessing accrual (batching availability unlock)** — parking-phase admission now accounts actual residency (~3.1MB/item vs 17.8MB full-cycle estimate at 1080p→640) with execution-phase top-up; caps and rejection semantics unchanged. Pooled 8-lane re-run: window fills to configured 4.0 (was 3.0 with 30270 backpressures → now 0), throughput +14%; tail latency shifts from throttled-smooth to queued — pick per SLA. Deployment note: CUDA batching steady-state RSS scales with batch shape (world-class dual-input pooled ≈2.9-3.0GB; ~+200-300MB per batch unit N) — reserve accordingly.
- **JPEG export encoding core speedup** — `amc_frame_encode` JPEG base swapped from image-crate JpegEncoder to `jpeg-encoder` 0.7.1 (pure Rust, AVX2 runtime-detected, scalar fallback; 4:4:4 pinned to match the old base's actual output — image-crate's documented 4:2:2 is wrong, verified via SOF0 byte streams). Pure-encode 1.75-2.1x (1080p q95 50.9→26.6ms; 960x540 up to 2.10x); PSNR slightly better across the board; guard test pins quality-scale parity against the reference encoder. End-to-end gain depends on the NV12→RGB conversion share (unchanged, in the same seam).
- Governance: ABI freeze baseline recomputed (v0.6.0's ABI-1.3 artifacts had drifted unrecorded — six artifacts fully attributed); release checklist now gates on `freeze-abi --check`.

Verification: workspace 508 tests zero failures; hardware gates green (real-weights e2e, FFI batch e2e 3/3, device-side assembly, vocab GPU parity, m2b-repro stability); fingerprint gate 285; ABI frozen 11 unchanged.

/ 兼容新增+修复版，C ABI 不变（1.3），换 DLL 即得。档案注册表演进（world 级真实权重的图像攒批路径接通+真实权重 e2e+CPU/GPU 词汇注入批语义统一）；攒批入账分期计费（窗口可正常凑满、背压归零+部署内存规划注记）；JPEG 导出编码底座提速 1.75-2.1x（纯 Rust+SIMD，4:4:4 对齐旧底座实际行为并守门）；ABI 冻结基线重算与发版清单堵漏。验证：workspace 508 零失败；真机门全绿；指纹闸门 285。

## v0.6.1 (2026-11-07)

Fix + compatible-addition release, **C ABI unchanged at 1.3** (`AM_ABI_VERSION` 65539). No new C symbols or structures — existing consumers only need to swap the DLLs to pick up the fixes and expanded coverage.

- **Tensor batching regression fix (important)** — a 2026-10-29 refactor made every N>1 `run_batch` on dynamic-batch-axis models (`native_batch=1`, the hard precondition for adaptive batching) fail with `vision.batchUnsupported`, silently breaking R5 tensor batching for 9 days; caught when the batch-contract gate was re-run and restored to input-first-axis comparison. **Consumers on v0.6.0 using adaptive batching with N>1 dynamic-axis batches should upgrade.**
- **Open-vocabulary on the GPU direct-transfer path** — `run_gpu` now injects the vocab embedding snapshot into the IoBinding second input (CPU-memory binding, explicit `synchronize_inputs` before run); affinity decoding works on the GPU path; unbound dual-input profiles reject with the same `vision.vocabNotBound` code as CPU; single-input models unaffected.
- **Image-item adaptive batching** — image inputs (incl. GPU frames) now batch on par with tensor items (contract §3.5 image-item clause). CPU images and GPU frames batch in separate windows (different assembly domains); GPU frames preprocess synchronously at submission (§3.3a: texture borrow closes when the call returns — async GPU-frame submissions therefore carry ~ms-scale preprocessing inside the call); GPU physical batches assemble device-side with zero CPU readback; per-item decode params (confidence/max_det) take effect per item. Honest boundary: the shipped profile registry has no batchable PROFILE image model yet — machinery is in place and verified (world.onnx device-side assembly vs single runs; FFI-window RAW image batching e2e), real-model e2e awaits profile-registry evolution.
- **H264 session NotReady loop hotfix (P0, integrator production evidence)** — the unready-session startup limit now counts from the last subscription interest; sessions stay alive through slow RTSP re-establishment / 453 long backoff instead of being killed into a rebuild loop. Ships with a Rust-only diagnostic ring (`H264Hub::diag_snapshot`/`session_states`).

Verification: workspace 499 tests zero failures; new FFI/vision ignore gates (batch e2e ×2, GPU batch assembly, vocab GPU parity) green on real hardware; m2b-repro 8-iteration stability zero crashes; clippy clean on touched crates; fingerprint gate 284 green. Pooled 8-lane comparison recorded as a structural negative (single pooled session serializes into one CUDA stream — see ROADMAP); tail-latency-sensitive scenarios still benefit (P99 114ms vs 348ms).

/ 修复+兼容新增版，C ABI 不变（1.3），无新符号/结构——换 DLL 即得。重点：张量攒批回归修复（10-29 起 N>1 动态轴合批被错误拒绝，v0.6.0 攒批用户建议升级）；开放词汇 GPU 直传路径支持；图像项自适应攒批（CPU/GPU 分窗、接受段预处理、设备侧组装、逐项参数——真实档案模型路径待注册表演进）；H264 会话 NotReady 死循环热修（按最后订阅意向续命+诊断环形缓冲）。验证：workspace 499 零失败；新增真机门全绿；m2b-repro 8 循环零崩；指纹闸门 284 绿。池化 8 路对照为结构性负结论（单池化会话单 CUDA 流串行，见 ROADMAP），尾延迟敏感场景仍有正收益（P99 114ms vs 348ms）。

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
