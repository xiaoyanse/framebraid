# Changelog / 变更记录

## [0.6.1] - 2026-11-07

- **Image-item adaptive batching (batch C) + dynamic-axis regression fix**: image inputs now participate in adaptive batching on par with tensor items (contract §3.5 image-item clause inserted; no C ABI structure change, generator chain untouched). Aggregation key gains an execution-entry dimension (CPU images and GPU frames never share a window — different assembly domains); `Payload::Native` now carries per-item decode params so confidence/NMS/max_det take effect per item. GPU-frame capture now completes preprocessing synchronously in the acceptance segment (§3.3a as always documented: texture borrow closes when the submission call returns), which both fixes the latent async-lifetime gap of the M2b-era worker-time texture read and makes window parking safe — parked items hold device input buffers, not texture references; async GPU-frame submissions therefore carry ~ms-scale GPU preprocessing inside the call. GPU physical batches assemble device-side (`assemble_frames`: N×[1,3,H,W] device tensors D2D-copied into one [N,3,H,W] binding — CPU concat at ~3ms/sample was measured as a non-starter in R1.4); CPU PROFILE batches merge prepared tensors via `run_batch` and split per item. A pre-existing regression is fixed en route: M2b commit `49d8a10` reshaped `extract_outputs`' batch-axis check from "match the actual input first axis" to "equal the declared batch", which made every N>1 `run_batch` on dynamic-axis models (native_batch=1 — the hard precondition for batching) fail with `vision.batchUnsupported`, silently breaking R5 tensor batching since 2026-10-29 (caught when the batch-contract gate was re-run; the check is restored to input-first-axis comparison, drift-rejection semantics preserved). Honest boundary: the shipped profile registry has no batchable PROFILE image model today — single-input profiles pin batch=1 (batch-ineligible at load) and the dual-input affinity profile's 2-D text/2-D output declaration cannot satisfy batch split+decode simultaneously; PROFILE-path machinery is implemented but its real-model e2e awaits profile-registry evolution (PM decision), while the machinery itself is verified at the vision-crate level (world.onnx device-side batch assembly: diagonal max|Δ| 1.7–6.9 vs off-diagonal 287–503 at output amplitude 873) and RAW image batching is verified end-to-end through the FFI window (4 distinct-color PNGs → one physical batch → per-item identity; tail-single, cancel and window stats asserted). Verification: batch-contract ignore gate green (dynamic 1/3/64, fixed 4, drift rejection); new FFI ignore gates `batch_e2e_*` green; new vision-crate ignore gate `gpu_batch_assembly_matches_single_runs` green; vocab CPU/GPU e2e gates green (capture-at-admission regression); m2b-repro full 8 iterations zero crashes; workspace 499 green / 0 failed (3 new unit tests); clippy clean on touched crates; fmt stable; boundary gate 284; Release DLL rebuilt and smoked (batching window live: 43 batches formed in 4s, 2 lanes at 23.1fps); pooled 8-lane comparison experiment recorded as a structural negative (pooled+batching 31.9fps vs 8 independent sessions 109.6fps — single pooled session serializes into one CUDA stream; tail latency smoother at P99 114ms vs 348ms; logged with follow-up PM decisions). Implementation `631e68d` (regression fix) + `cdd3027` (batch C). / 图像项自适应攒批（批 C）+动态轴回归修复：图像项与张量项同等参与攒批（合同 §3.5 插图像项条款；无 C ABI 结构变更、不动生成链）。聚合键增加执行入口维度（CPU 图像与 GPU 帧不混批）；原生批载荷携带逐项 decode 参数（confidence/NMS/max_det 逐项生效）。GPU 帧 capture 在接受段同步完成预处理（§3.3a 既有承诺落地：纹理借用随提交调用返回闭合），既修复 M2b 期"工作线程时读纹理"的异步生命周期缺口，也使窗口挂起安全——挂起项持设备输入缓冲而非纹理引用（异步 GPU 帧提交因此含毫秒级 GPU 预处理耗时）。GPU 物理批设备侧组装（N×[1,3,H,W] D2D 拼进单一 [N,3,H,W] 绑定；R1.4 实测 CPU 拼接 ~3ms/样本不可接受）；CPU PROFILE 批经 run_batch 合并拆分逐项解码。顺带修复既有回归：M2b 提交 `49d8a10` 把 `extract_outputs` 批轴检查从"与实际输入首轴一致"改成"与声明 batch 相等"，动态轴模型（native_batch=1，攒批硬前提）N>1 的 run_batch 自 2026-10-29 起整批失败（重跑批合同门时捕获；已复原为输入首轴对照并保留漂移拒斥语义）。诚实边界：现发布档案注册表无可批的 PROFILE 图像模型——单输入档案全锁 batch=1，双输入亲和档案的 2-D 文本/2-D 输出与批拆分+解码语义互斥；PROFILE 路径机制已实现，真实模型 e2e 待档案注册表演进（PM 决策）；机制本体在 vision crate 层实证（world.onnx 设备侧组装：对角 max|Δ| 1.7–6.9 vs 非对角 287–503，输出幅值 873），RAW 图像攒批经 FFI 窗口全链验证（四异色 PNG→单一物理批→逐项身份；尾项/取消/窗口统计断言）。验证：批合同 ignore 门绿（动态 1/3/64、固定 4、漂移拒斥）；新增 FFI 门 `batch_e2e_*` 绿；新增 vision 门 `gpu_batch_assembly_matches_single_runs` 绿；vocab CPU/GPU e2e 绿（接受段预处理回归）；m2b-repro full 8 循环零崩；workspace 499 绿/0 失败（净增 3 单测）；clippy 改动 crate 零告警；fmt 稳定；闸门 284 绿；Release DLL 重建+冒烟（攒批窗口真实活跃：4 秒 43 批、2 路 23.1fps）；池化 8 路对照实验以结构性负结论入档（池化+攒批 31.9fps vs 独立会话裸跑 109.6fps——单池化会话串行进单 CUDA 流；尾延迟更平滑 P99 114ms vs 348ms；后续 PM 决策项随 ROADMAP 登记）。实现 `631e68d`（回归修复）+`cdd3027`（批C）。

- **Open-vocabulary on the GPU direct-transfer path (batch B)**: `run_gpu` now injects the vocab embedding snapshot into the IoBinding second input — CPU-memory binding with per-injection H2D handled by ORT and an explicit `synchronize_inputs` before `run_binding` (ORT's bound-input transfer to CUDA EP is queued asynchronously; images is a device tensor with no transfer, vocab is not); `decode_prediction` passes the vocab layout through so affinity decoding works on the GPU path; unbound dual-input profiles reject with `vision.vocabNotBound` (same code as the CPU path, submission-layer check unchanged); single-input models are untouched. Two latent defects fixed en route, both exposed by the first ABI-level full-chain GPU-frame e2e: `Plan::capture` unconditionally sliced the encoded payload and hit a null pointer on GPU-frame submissions (GPU frames carry no CPU payload — M2b-era latent, the ABI chain had never been e2e-exercised with GPU frames), and the `cuda_sys` `memcpy_htod/dtod/dtoh` bindings declared a 3-param signature over the 4-param `cudaMemcpy` (kind read off the stack as garbage; zero prior callers). Verification: new ignore-gated `vocab_e2e_gpu_direct_injection_matches_cpu` (synthetic dual models on CUDA backend, GPU frame via D3D texture vs CPU PNG injection parity, confidence ≤1e-4 / bounds ≤1e-3 — the synthetic affinity head's scores depend only on the vocab embedding so preprocessing-domain differences cancel) 15/15 stable plus serial and parallel pair runs green; SDK-level GPU e2e regression green; workspace 496 green; clippy clean. Implementation `fd88c62`. / 开放词汇 GPU 直传路径支持（批 B）：`run_gpu` 注入 vocab 嵌入快照到 IoBinding 第二输入——CPU 内存绑定，H2D 每次注入由 ORT 执行、`run_binding` 前显式 `synchronize_inputs`（ORT 绑定输入到 CUDA EP 的搬运异步排队；images 是 device 张量无搬运、vocab 不是）；`decode_prediction` 透传词汇布局，GPU 路径亲和解码走通；未绑定双输入档案拒 `vision.vocabNotBound`（与 CPU 路径同码，提交层校验不变）；单输入模型零影响。顺带修复两个被首个 ABI 级 GPU 帧全链 e2e 暴露的既有缺陷：`Plan::capture` 对编码载荷无条件切片，GPU 帧提交（无 CPU 载荷）触发空指针切片（M2b 期遗留——ABI 链此前从未有 GPU 帧 e2e 走过）；`cuda_sys` 的 `memcpy_htod/dtod/dtoh` 三参签名错绑四参 `cudaMemcpy`（kind 读栈垃圾；此前零调用）。验证：新增 ignore 门 `vocab_e2e_gpu_direct_injection_matches_cpu`（合成双模型 CUDA 后端，D3D 纹理 GPU 帧 vs CPU PNG 注入对拍，置信 ≤1e-4/框坐标 ≤1e-3——合成亲和头分数只由 vocab 嵌入决定，预处理域差异被模型设计抵消）15/15 稳定+串行/并行双跑绿；SDK 层 GPU e2e 回归绿；workspace 496 绿；clippy 零告警。实现 `fd88c62`。
- **H264 session NotReady loop hotfix (P0, integrator production evidence)**: the unready-session startup limit now counts from the *last subscription interest* instead of session creation — repeated NotReady-denied retries keep the session alive so slow RTSP re-establishment (or the 30s RTSP-453 long backoff) no longer gets killed at the 60s limit into a rebuild loop ("running for a while, then never ready again until service restart" symptom). No-interest startups still expire (leak-safe); terminal sessions are not extended. Ships with a session-level diagnostic ring (32 events: connect attempt/ok with duration/fail with the raw error string — previously discarded/453 backoff/ready with startup duration/read failures/stop cause split idle vs startup-limit) exposed via Rust-only `H264Hub::diag_snapshot()` / `session_states()` (direct-dependency consumers; no C ABI change — the C ABI surface never exposed the affected hub). The startup limit is test-injectable (`H264Hub::new_with_limits`). Implementation `d6b8fc1`. / H264 会话 NotReady 死循环 P0 热修：未就绪期存活上限改按“最后订阅意向”起算（持续被拒重试续命，RTSP 二次建立慢/453 长退避不再被 60s 上限腰斩成重建死循环）；无意向照超限自清、终态不续命。配套会话诊断环形缓冲（连接成败与耗时、原始错误串——此前被丢弃、453 退避、就绪耗时、停流原因二分）经纯 Rust `diag_snapshot`/`session_states` 暴露（直依赖消费者；C ABI 面不含受影响的 hub，无 ABI 变更）；启动上限测试可注入。实现 `d6b8fc1`。

## [0.6.0] - 2026-11-05

- Rust direct-dependency example (`examples/vision_cascade_tracker.rs`): tracker usage (confirm-hits=1 first-frame emission, ID stability across frames, TTL re-acquisition) and cascade primitives (`generate_rois` / `map_box_to_original`), verified by `cargo run`. Implementation `672c49d`. / Rust 直依赖调用示例：跟踪与级联纯逻辑形态，cargo run 验证过。实现 `672c49d`。

- ROI cascade inference (C1, contract §4.1): `amv_infer_cascade_sync/async` — one call runs full-frame primary detect → ROI generation (static + dynamic rules, cap 16, confidence-desc truncation) → per-ROI crop/letterbox upscale → secondary detect → coordinate mapping back to the original frame. Independent model handles per stage (integrator-verified scenario: small-object score 0.167 → 0.461, boxes 7 → 15); per-ROI independent NMS; single-ROI failure skips without failing the batch; `amv_result_t` tail gains a `cascade` result (secondary boxes + source `hits` + `skipped_count` three-state observability); vocab-bound models may serve as the secondary stage. Integrator confirmation frozen 2026-11-05 (8 points, no objections; observability note landed as `skipped_count`). Implementation `98cee5f`+`a11d1b1`. / ROI 级联推理（C1，合同 §4.1）：一次调用完成全图一级检测→ROI 生成（静态+动态规则可并存，上限 16，超限按一级置信度降序截断）→逐 ROI 裁剪 letterbox 放大→二级检测→坐标回映原图。两级模型独立句柄（下游实证：小目标全图直检 0.167，ROI 放大后 0.461、检出框 7→15）；NMS 逐 ROI 独立；单 ROI 失败跳过不废整批；`amv_result_t` 尾部新增 `cascade` 明细（二级框+来源 `hits`+`skipped_count` 三态可观测）；vocab 词表模型可作二级。消费方确认冻结（2026-11-05，8 点无异议；可观测性备注已落地为 `skipped_count`）。实现 `98cee5f`+`a11d1b1`。

- Cross-frame object tracking (§4.1b): `amv_tracker_create/update/reset/close` — SORT-grade lightweight box association decoupled from inference (update accepts boxes from any source: detect results, cascade secondary boxes, external detectors). Constant-velocity prediction + IoU greedy matching (deterministic ordering), track IDs monotonic and never reused, confirm-hits gate against flicker, TTL-based track recycling, output passes through input box values unsmoothed. Contract: dual-dll-abi.md §4.1b; verification: 13 core unit tests (crossing targets keep identity, moving target tracked via prediction, TTL re-acquisition semantics, capacity limits) + 7 FFI tests; workspace 480 green. Implementation `8809d90`+`54248e4`. / 跨帧目标跟踪（§4.1b）：跟踪器四 API——与推理解耦的轻量框关联（update 接受任意来源框：检测结果、级联二级框、外部检测器）。恒速预测+IoU 贪心匹配（确定性排序）、轨迹 ID 单调不复用、确认帧数防闪烁、TTL 回收、输出喂入值透传不平滑。合同 §4.1b；验证：13 项核心单测（交叉不串/移动跟住/TTL 重现语义/容量上限）+7 项 FFI 测试；工作区 480 绿。实现 `8809d90`+`54248e4`。

- Single-frame image export (§2.11): `amc_frame_encode` — encode any host-held `am_pixel_view_t` (subscription callback / shared pool / decoder output, any source) to JPEG/PNG bytes for evidence/snapshot/debug use. Stateless single call sharing the image-codec base without `amc_encoder` handle lifetime; all pixel-view formats supported (RGBA8/RGB8/BGR8 direct, NV12 via internal BT.601/709/2020 conversion parity with the vision side); capacity-insufficient fills required size and returns `sdk.limitOutOfRange` (no partial output); 32MiB output cap; thread-safe pure function. Verification: 7 core unit tests (PNG lossless roundtrip, JPEG quality ordering, NV12 neutral-gray conversion) + FFI test (capacity retry, opts rejections, odd-dimension NV12); workspace 489 stable across repeat runs. Implementation `77ac14b`+`1812ad8`. / 单帧图像导出（§2.11）：宿主持有的任意 pixel view 编码为 JPEG/PNG 字节（取证/快照/调试）。无状态单次调用共享 image 编码底座、不占编码器句柄；全像素格式（RGBA8/RGB8/BGR8 直接组装，NV12 内部矩阵换算与 vision 侧系数同源）；容量不足填所需大小返回 limitOutOfRange（不返回半份）；32MiB 上限；线程安全纯函数。验证：7 项核心单测（PNG 无损往返/JPEG 质量序/NV12 中性灰换算）+FFI 测试（容量重试/参数拒绝/奇数 NV12）；工作区 489 两遍稳定。实现 `77ac14b`+`1812ad8`。

## [0.5.0] - 2026-11-04

- Open-vocabulary A1+A2 (issue #9 second batch / EXTENSION_CAPABILITIES A1+A2+A4): runtime vocab binding via `amv_vocab_bind` (text tower as `task=text-encoder` profile, tokenizer stays with the caller, atomic re-bind on in-flight requests, failed re-bind keeps the old binding); image-path inference auto-injects the cached embeddings as the second input (`run_ref` zero-copy path; unbound dual-input profiles reject with `vision.vocabNotBound`=1033); affinity-head decode with per-(anchor,prompt) candidates, within-entry cross-prompt NMS (best score kept), per-box hit details (`vocab_hits`: entry/prompt indices, business filtering keeps the matched word), class_name = entry label (A4 display-name separation). Contract: dual-dll-abi.md §4.1; integrator six-point confirmation incl. pad-semantics constraint (pad-only encoders, BERT-line excluded). Verification: workspace 437 green, affinity decode synthetic tests (within-entry merge / cross-entry independence / hit order / short-head & bad-score rejection). Implementation `b43cf69`+`b6a18f0`+`bbae548`+`5fdf695`; integrator six-point confirmation incl. pad constraint. / 开放词汇 A1+A2：运行期词表绑定（文本塔独立档案、tokenization 归调用方、原子换代、失败保持原绑定）；图像入口自动注入嵌入（零拷贝路径，未绑定拒 1033）；亲和头解码——组内跨提示词 NMS 合并保留最高分、逐框命中明细（entry/prompt 索引）、类目名=条目 label（A4）。合同 §4.1；接入方六点确认（含 pad 语义约束）。验证：工作区 437 绿+亲和解码合成测试（组内合并/组间独立/命中序/短头与坏分拒绝）。实现 `b43cf69`+`b6a18f0`+`bbae548`+`5fdf695`；接入方六点确认（含 pad 约束）。
- Local FILE source stage 2: `amc_source_seek` (three modes: keyframe before/after, exact with dual semantics — frame-side decode-drop, encoded-side first packet at/after target), `amc_source_get_duration`, `amc_source_set_rate` (positive finite = paced, INFINITY = as-fast-as-possible default), `amc_source_set_loop`; EOF revival via seek; RESYNC(camera.seek) event is the source of truth for effect. Contract: dual-dll-abi.md §2.4 stage 2 (C ABI 1.2 compatible additions); verification: workspace 429 green + real-machine seek integration (three modes, EOF revival, loop replay, EXACT filter) on multi-GOP fixture. Implementation `7e86777`. / 本地 FILE 源阶段 2：seek 三模式、时长查询、倍速（正有限值=节拍，INFINITY=尽快默认）、循环重放与 EOF 后 seek 复活；RESYNC(camera.seek) 事件为生效事实源。合同见 dual-dll-abi.md §2.4 阶段 2；验证：工作区 429 绿 + 多 GOP 夹具真机 seek 集成（三模式/EOF 复活/循环重放/EXACT 过滤）。实现 `7e86777`（C ABI 1.2 兼容新增）。

- Audit clearance batch (2026-11-01 extended audit): P1 multipart part-header 64KB cap now enforced on the terminated-header branch too (malicious headers carrying their own `

` no longer bypass the cap) with a repro test; P3 Digest challenge-cache comment states actual behavior (per-origin overwrite, no eviction, growth host-controlled); L1/L2 defensive hardening (run_gpu shape rank/dimension guards, negative RTSP dimensions clamped at the source — panic paths narrowed to explicit rejects). Blind-spot regression tests ×9 (7 audit gaps + stats/copy_report parity); all seven contract semantics pass, no real defects found. Implementation `8e05d1d`+`b74b1b2`. / 审计清偿批：P1 multipart 头上限补全终结符分支+复现测试；P3 注释口径改实；L1/L2 防御加固（panic 路径收窄为明确拒绝）。测试盲区回归 9 项全过、未发现真缺陷。实现 `8e05d1d`+`b74b1b2`。

## [0.4.0] - 2026-11-03

- Local FILE source stage 1: file demux, backpressure, EOF and PULL draining (implementation `5734e78`+`02061a1`+`f651c45`+`69a99e0`; EOF integration test green on real FFmpeg). / 本地 FILE 源阶段 1：文件解封装、背压、EOF 与 PULL 排空（实现 `5734e78`+`02061a1`+`f651c45`+`69a99e0`；真机 FFmpeg 下 EOF 集成测试通过）。
- Open-vocabulary first batch (issue #9 / EXTENSION_CAPABILITIES A3+B1, implementation `ebcae0d`): detect-profile decoders gain declared output layout (`transposed` default / `row-major`) and box coordinate form (`cxcywh` default / `xyxy`) — the previously hardcoded single layout no longer blocks non-YOLO detection heads; profile output-shape consistency is validated at load (`vision.profileMismatch` carries detected shape vs declaration). Dual-input profiles (image + text-embedding, YOLO-World head form) are declared via `inputs[]` with per-input name/dtype/shape validation; mismatch details carry input name, dimension, expected and actual values (e.g. `vision.inputShapeMismatch|input=images,dim=1,expected=3,actual=4`). Dimension validation does not catch mismatched embedding spaces (same-dimension foreign text towers produce letterbox-padding false boxes — see EXTENSION_CAPABILITIES.md A3); pairing responsibility stays with the model author, embedding-space health probing is registered as future work. / 开放词汇首批（issue #9 / EXTENSION_CAPABILITIES A3+B1，实现 `ebcae0d`）：检测类档案解码器支持声明输出布局（默认 transposed / row-major）与盒坐标形式（默认 cxcywh / xyxy）——此前单一硬编码布局不再拦截非 YOLO 检测头；档案输出 shape 加载一致性校验（profileMismatch 详情带检测 shape 与声明）。双输入档案（图像+文本嵌入，YOLO-World 检测头形态）经 inputs[] 声明并逐项校验名字/dtype/shape；失配详情带输入名/维度/期望/实际值。维度校验不拦截异源嵌入空间（同维异源文本塔实测产生 letterbox 填充带假框，见 EXTENSION_CAPABILITIES.md A3），配对责任在模型制作者，嵌入空间健康度探测登记为后续能力。

- Institutional batch (feedback-driven audit C1-C8 + v0.3.2 packaging lessons, implementation `95d023a`+`1f4757d`): release checklist gains the fixed artifact set (five attachments per version, no silent shrinkage) and a package-hygiene gate (no `__pycache__`/`.pyc`/IDE files; build-record.json mandatory in the windows zip) plus the vl-SDK downstream notification step; CHANGELOG finalization discipline (every claim bound to a commit hash or verification record; integrator numbers labeled, never presented as in-house measurements); error-detail encoding rules enter the ABI contract (parameter failures carry field/value/range, version failures carry detected/expected, file failures carry path/OS error; stable key + dynamic detail coexist); runtime compatibility single source of truth (SUPPORT.md authoritative, RELEASE_STATUS registers verified runtimes per release); new docs/ENVVARS.md registry (all `AM_RT_*`/`AM_SDK_*` variables, runtime vs test-fixture classified, new variables must register first). / 制度批次（反馈驱动审计 C1-C8 + v0.3.2 打包教训）：发布清单增附件形态固定清单（五件逐项，不得静默缩水）与打包卫生闸门（禁入 `__pycache__`/`.pyc`/IDE 文件，windows zip 必含 build-record.json）及 vl-SDK 下游通知步；CHANGELOG 定版纪律（每条声明绑定提交哈希或验证记录，接入方数字标注出处不得写成本仓实测）；错误详情编码约定进 ABI 合同（参数类带字段/值/区间、版本类带检测/期望、文件类带路径/OS 错误码；稳定键与动态详情并存）；运行库兼容唯一口径（SUPPORT.md 为准，RELEASE_STATUS 每版登记已验版本）；新增 docs/ENVVARS.md 环境变量登记表（全量 `AM_RT_*`/`AM_SDK_*`，运行时与夹具分类，新增变量先过表）。
- Three machine gates join the verification chain (baseline-snapshot pattern, implementation `1f4757d`): `check-bindings.mjs --check` (struct-field × python/csharp exposure matrix, undeclared/stale/invalid reporting), `check-doc-coverage.mjs --check` (181 exported symbols × four docs; 152 existing unmentioned symbols frozen as the 2026-11-03 baseline, new symbols must be documented), `check-zero-semantics.mjs --check` (numeric/bitset fields must carry `zero=default|invalid|explicit` annotations; 15 annotated from contract text, 451 frozen as baseline; annotations pass through to `am_sdk_types.h` comments — comment-level only, no layout change, no ABI bump). `intern_detail` in vision-ffi now delegates to `sdk-support::diagnostics::intern` (single implementation, chain-registered in sdk-source-revisions.json). / 三个机器闸门进入验证链（基线快照模式）：绑定参数覆盖矩阵检查（结构字段 × Python/C# 暴露状态，未声明/过期/非法报告）、文档符号覆盖检查（181 导出符号 × 四文档，152 存量未提及冻结为 2026-11-03 基线，新增符号必须进文档）、零值语义强制检查（数值/位集字段须带 zero=default|invalid|explicit 标注，15 个按合同正文标注、451 冻结基线；标注经注释透传进 am_sdk_types.h——纯注释级、无布局变化、不升 ABI）。vision-ffi 的 intern_detail 委托 sdk-support 单一实现（指纹链式登记）。

## [0.3.2] - 2026-11-03

- Integrator-feedback batch (source: SDK integrator review 2026-10-29 + feedback 2026-11-03): Python bindings now expose `infer_threads` / `total_thread_budget` / `domain_infer_slots` / `feature_flags` on `VisionContext` init and `intra_threads` on model loading (0 = SDK default), removing the hardcoded 2-thread settings — integrator-measured (AIeng, 16-core) full-chain p50 63.5 → 33.1 ms once intra threads are raised to 12, i.e. binding users reach the same level previously only available via direct C API; thread accounting against `total_thread_budget` is documented in sdk/INTEGRATION.md. / 接入方反馈批（SDK 接入评估 20261029 + 团队反馈 20261103）：Python 绑定参数化——init 透传 infer_threads / total_thread_budget / domain_infer_slots / feature_flags，模型加载暴露 intra_threads（0=SDK 默认），移除写死的 2 线程；16 核（接入方 AIeng 实测口径）放开 intra 12 线程后全链 p50 63.5→33.1ms，绑定用户拿到此前只有直调 C API 才能到的档位；与 total_thread_budget 的记账关系见 sdk/INTEGRATION.md。
- OpenVINO 2026.x adaptation for the native engine (E1, feedback P0-1/P1-6): the runtime directory is now discovered from vision `runtime_dirs` (as the ABI contract always stated) with `AM_SDK_OPENVINO_DIR` as an explicit operator override — the env-var-only channel was the actual cause of integrator load failures, not a 2026.x symbol break (211 vs 209 C-API exports differ only by two log-callback symbols; all 28 bound symbols present on both lines, element-type ordinals and struct layouts identical). The loaded runtime version is now detected and carried in failure details (detected buildNumber vs expected version line) instead of a bare `vision.backendStrictUnavailable`, so integrators no longer reverse-engineer version mismatches from the ABI doc; single binding covers the 2025.x/2026.x lines, verified on 2026.2.1 (wheel), 2026.1.0 (vcpkg) and 2025.3.0 (wheel) with load+compile+inference+parity each. / OpenVINO 原生引擎 2026.x 适配（反馈 P0-1/P1-6）：运行库目录按 ABI 合同从 vision runtime_dirs 发现，AM_SDK_OPENVINO_DIR 降为显式操作员覆盖——接入方加载失败的根因是仅认环境变量的实现缺陷而非 2026.x 符号断裂（C API 导出 211 vs 209 仅差 2 个日志回调符号，绑定 28 符号两线全在，元素类型枚举与结构布局一致）；装载读取运行库版本，失败详情携带检测到的版本与期望版本线，不再只报裸 backendStrictUnavailable、无需对照 ABI 文档倒推；单一绑定覆盖 2025.x/2026.x 两线，2026.2.1（wheel）/2026.1.0（vcpkg）/2025.3.0（wheel）三线装载+编译+推理+对拍实测通过。
- Init failure details now name the field, the submitted value and the valid range when `amv_init` rejects out-of-range parameters such as `infer_threads` / `total_thread_budget` (feedback P1-3/P1-6), instead of a bare `sdk.limitOutOfRange`. / init 参数超范围（infer_threads / total_thread_budget 等）的失败详情带字段名、传入值与合法区间（反馈 P1-3/P1-6），不再只报裸 sdk.limitOutOfRange。
- Language binding wrappers for raw tensor inputs default the `AM_TENSOR_CONTIGUOUS` flag instead of leaving callers to hand-fill it. / raw 张量输入的语言绑定封装默认置 `AM_TENSOR_CONTIGUOUS` 连续标志，调用方不再手工拼装该位。
- INTEGRATION.md gains integration sections for the v0.3 capabilities (E1 OpenVINO native engine with `AM_SDK_OPENVINO_DIR` and version-line requirements, M5 `amv_model_swap` slot semantics and DETACH notes, S5 encoder/archive/stream entries with dimension-consistency and RTMP budget behavior), and a new runtime licensing guide (docs/RUNTIME-LICENSING.md) draws the LGPL-minimal vs GPL-encode boundary and the distribution obligations of each (feedback P1-7). / INTEGRATION.md 增补 v0.3 能力接入说明（E1 OpenVINO 原生引擎含 AM_SDK_OPENVINO_DIR 与版本线要求、M5 amv_model_swap 槽位语义与 DETACH 注意事项、S5 encoder/archive/stream 三入口含尺寸一致性与 RTMP 预算行为），并新增运行库许可选择指引（docs/RUNTIME-LICENSING.md）：LGPL minimal 档案与 S5 编码所需 GPL（libx264）构建的边界、适用场景与分发义务（反馈 P1-7）。
- Release artifacts are published in full for each version: the GitHub Release for a published tag now carries the matching binaries and SHA256SUMS (the v0.3.1 gap), per the release-actions section of the release checklist (feedback P2-6). / Release 制品完整形态：已发布 tag 的 GitHub Release 同步携带匹配制品与 SHA256SUMS（补 v0.3.1 缺口），发布动作按发布清单的 Release 页创建与资产上传段执行（反馈 P2-6）。
- Error-detail architecture: dynamic diagnostic text is now interned in sdk-support (`diagnostics::intern` + `Detail::dynamic`, content-deduped with process-lifetime pointers) so runtime-constructed context — field names, detected versions, valid ranges — can flow into `am?_get_last_error` details while `source` stays a stable key (key discipline revised to stable key + dynamic detail); the FFmpeg runtime version-mismatch failure now names the specific DLL, the detected major and the required major (audit A8), delivered through the interned detail. / 错误详情架构：动态诊断文本驻留机制下沉 sdk-support（intern 内容去重、进程保活，`Detail::dynamic` 构造），运行时上下文（字段名/检测版本/合法区间）可进 last_error 详情且 source 保持稳定键（键纪律修订为稳定键与动态详情并存）；FFmpeg 运行库版本不匹配失败详情带具体库名、检测主版本与期望主版本（审计 A8）。
- C# bindings reach parameter parity with Python (audit A1/A5/A6/A7 plus A2/A3/A4): `VisionContext` create exposes `inferThreads` / `totalThreadBudget` / `domainInferSlots` / `featureFlags` (0 = SDK adaptive), `LoadModel` / `LoadModelSelected` / `LoadModelMemorySelected` gain `intraThreads` (0 = SDK default), `confidence` flows through `InferEncoded` / `SubmitEncoded` / `InferPixel` / `SubmitPixel` and `OwnedPixels`, `abi_version` (65537) and the pixel-view magic become generated constants (`AbiConstants`, synced by the generator instead of hand-edits), `Capture` gains `deadlineMs` (0 = SDK default), `Frames` opens the PULL queue parameters `queueMaxItems` / `queueMaxBytes` (PUSH rejected until callback packaging lands) and `OpenSource` gains `stream` ("main"/"sub"); Python adds the same capture/frames/stream parameters and now references `AM_PIXEL_VIEW_MAGIC`. / C# 绑定与 Python 参数对齐（审计 A1/A5/A6/A7 与 A2/A3/A4）：Create 透传 inferThreads/totalThreadBudget/domainInferSlots/featureFlags（0=SDK 自适应），LoadModel/LoadModelSelected/LoadModelMemorySelected 增 intraThreads（0=SDK 默认），confidence 贯通四组推理入口与 OwnedPixels，abi_version（65537）与像素视图 magic 改为生成常量 AbiConstants（随生成器同步），Capture 增 deadlineMs（0=SDK 默认），Frames 开放 PULL 队列参数（PUSH 在回调封装落地前拒绝），OpenSource 增 stream 主/子码流；Python 同步补 capture/frames/stream 参数并引用 AM_PIXEL_VIEW_MAGIC。

## [0.3.1] - 2026-11-01

- Full audit-fix batch for review issues #2-#8 plus 10 additional same-pattern findings from a whole-repo audit (report: .local/audit/issue-pattern-audit-20261101.md): encode/archive/stream dimension validation (no OOB read on mismatched views), CUDA buffer RAII (no leak on second-allocation failure), MP4 finish-error propagation (no false save-success), RTMP connect budget + interruptible backoff (15s total, stop-wake, camera.timeout), encode PTS/DTS/time_base pass-through, am_image_input_t/am_frame_sub_desc_t prefix negotiation (legacy 48/40-byte callers keep working), C# binding regains the gpu field, AMV_CAP_GPU_FRAME_INPUT (bit10) capability bit set per actual CUDA availability, GPU frame handle exports hardened to notSupported until the closed loop lands. / 审查 issue #2-#8 与全仓模式审计另 10 项的全量修复：编码尺寸校验（杜绝越界读）、CUDA 缓冲 RAII（二次分配失败不泄漏）、MP4 结束错误上抛（不再误报保存成功）、RTMP 连接总预算与可唤醒退避（camera.timeout）、编码 PTS/DTS/时基贯通、图像输入与帧订阅结构前缀协商（旧版 48/40 字节调用方继续可用）、C# 绑定补 gpu 字段、GPU 帧输入能力位按实际置位、GPU 帧句柄族导出在闭环落地前统一 notSupported。
- Contract doc updated (dual-dll-abi.md, six semantics) and source revisions registered per repo rules; 402 unit tests + real-FFmpeg suite green, failure list identical to pre-change baseline. / 合同文档六条语义增补、源码修订按仓规则登记；402 单测与真机套件全绿，失败清单与改动前基线逐项一致。

## [0.3.0] - 2026-10-30

- GPU zero-copy mainline complete: D3D11VA texture frames flow to CUDA inference with per-frame copy audit (2 GPU copies, 0 CPU readback), detection-level parity verified on real hardware. / GPU 零拷贝主线全通：硬解纹理帧直达 CUDA 推理，每帧 2 次 GPU 内拷贝 / 0 次 CPU 回读，真机检测级对拍通过。
- Fifth inference engine: OpenVINO native C-API direct engine (AM_BACKEND_OPENVINO_NATIVE=4) with ORT-path untouched and measured fallback matrix. / 第五推理引擎：OpenVINO 原生 C API 直连（与 ORT 路径并存，回退矩阵实测）。
- Model hot swap: amv_model_swap atomic slot rebind; 8-thread sustained inference across swap with zero failures. / 模型热切换：槽位原子重绑，8 线程并发换模零失败。
- Encode & stream: H.264 encoding (libx264, GPL per owner decision), MP4 archive (verified playable), RTMP sessions with reconnect backoff. / 编码推流：H.264 编码（libx264，GPL 经决策）、MP4 存档（可播放验证）、RTMP 推流会话（断线重连）。
- MJPEG stream source: multipart/x-mixed-replace with bounded malicious-stream guards and reconnect. / MJPEG 流源：有界防护与重连。
- Consumer validation system: baseline gate (decode vs OpenCV / cadence / lanes), scenario library (R-series regressions), release-checklist consumer profile. / 消费方验证体系：基线门禁+场景库+画像过堂。
- Latest-overlay fair delivery with reserved queue entry (512 KiB within existing limits); bounded stream-info probe for insufficient header data. / 叠层公平投递（预留 1 项/512 KiB）；头信息不足时有界探测。

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
