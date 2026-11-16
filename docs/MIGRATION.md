# ABI 迁移指南 / ABI migration guide

[文档入口 / Documentation](README.md) · [ABI 合同 / ABI contract](design/dual-dll-abi.md) · [变更记录 / Changelog](../CHANGELOG.md)

本文按 ABI 演进步骤给出迁移说明：每段回答四问——变了什么 / 谁受影响 / 怎么迁 / 不迁会怎样。事实依据为 [CHANGELOG](../CHANGELOG.md) 各版本段与 [ABI 合同](design/dual-dll-abi.md)；发布状态以 [交付状态](RELEASE_STATUS.md) 为准。本文不改变任何合同语义。

This guide gives per-ABI-step migration notes: what changed / who is affected / how to migrate / what happens if you stay. Sources are the per-version CHANGELOG entries and the ABI contract; release status follows RELEASE_STATUS. Nothing here changes contract semantics.

## 版本协商通则 / Version negotiation rules

- `amc_get_abi_version` / `amv_get_abi_version` 返回 `(主版本<<16)|次版本`；主版本不符禁止 init（`sdk.abiUnsupported`）。
- 次版本为兼容新增：旧实现按 §1.1 次版本规则**拒绝更高次版本的 init 请求**；新实现接受更低次版本请求（v0.6.5 Python 绑定以 ABI 1.3 加载 ABI 1.4 双库并完成初始化，前向兼容实证）。
- 新导出仅对报告 ≥ 该次版本的实现可用：调用前先查 `amv_get_abi_version`；旧 DLL 缺新符号由加载期符号解析失败表达。
- 结构演进走尾部追加 + `struct_size` 协商：旧读者按已知前缀解释，不受尾部新字段影响。

## ABI 1.2 → 1.3（v0.6.0，`AM_ABI_VERSION` 65538→65539）

**变了什么 / What changed**：vision 侧新增 ROI 级联推理 `amv_infer_cascade_sync`/`amv_infer_cascade_async`（`amv_result_t` 尾部追加 `cascade` 明细字段）、跨帧目标跟踪四 API `amv_tracker_create`/`amv_tracker_update`/`amv_tracker_reset`/`amv_tracker_close`；camera 侧新增单帧 JPEG/PNG 导出 `amc_frame_encode`。全部为兼容新增（尾部追加/新导出），既有布局、枚举与语义零变更，三项均经接入方确认。

Vision adds ROI cascade inference (`amv_infer_cascade_sync/async`, with a tail-appended `cascade` field on `amv_result_t`), cross-frame tracking (`amv_tracker_*`, four exports), and camera adds single-frame JPEG/PNG export (`amc_frame_encode`). All compatible additions; no existing layout, enum value or semantic changed.

**谁受影响 / Who is affected**：仅使用新能力的调用方。既有调用方零改动——`amv_result_t` 读者按 `struct_size` 协商，旧结构大小照常解释；1.2 时代的 init 请求被 1.3 DLL 正常接受。

Only callers adopting the new APIs. Existing callers need no change: `amv_result_t` readers negotiate via `struct_size`, and init requests from 1.2-era callers are accepted by 1.3 DLLs.

**怎么迁 / How to migrate**：按需选用——级联需两级独立 detect 档案模型句柄（ROI 生成规则静态+动态可并存，上限 16；单 ROI 失败跳过不废整批，`skipped_count` 三态可观测）；跟踪器与推理解耦，update 接受任意来源框；`amc_frame_encode` 为无状态单次调用，容量不足按返回的所需大小重试一次即得。

Adopt as needed: cascade takes two independent detect-profile model handles (static+dynamic ROI rules, cap 16; per-ROI failure skips without failing the batch, observable via `skipped_count`); the tracker is inference-decoupled and accepts boxes from any source; `amc_frame_encode` is stateless — on insufficient capacity, retry once with the reported required size.

**同 ABI 线内的行为演进（无 ABI 变更）/ Behavioral evolution inside the 1.3 line (no ABI change)**：v0.6.4 起多 Model 共享 ctx 64MiB 预处理包络引入执行错峰槽——CPU 像素通道多 Model 批 3/4 由高拒绝变为零拒绝可用，GPU 帧直传双 Model 批 4 实测 194.8 items/s（8 路@25fps 实质满额）；分通道批形状指导值（GPU 直传批 4 / CPU 像素批 2）见 INTEGRATION。该演进不动 ABI，多 Model 部署按文档调参即可。

Since v0.6.4 the multi-model 64MiB ctx envelope uses a staggered execution slot: CPU-pixel multi-model batch 3/4 became zero-reject available, GPU-frame dual-model batch 4 measured 194.8 items/s. This changed no ABI — multi-model deployments retune per the INTEGRATION guidance (GPU-direct batch 4 / CPU-pixel batch 2).

**不迁会怎样 / If you don't migrate**：无影响。不使用级联/跟踪/导出即无感；旧 DLL 与新消费者、新 DLL 与旧消费者在 1.3 兼容规则内互不破坏。

Nothing breaks. Without cascade/tracking/export usage nothing is visible; old-DLL/new-consumer and new-DLL/old-consumer pairs stay compatible under the 1.3 rules.

## ABI 1.3 → 1.4（v0.7.0，`AM_ABI_VERSION` 65539→65540）

**变了什么 / What changed**：推理池编排容器三导出 `amv_pool_create`/`amv_pool_lane_model`/`amv_pool_close` + 新结构 `am_pool_desc_t`（合同 §3.5a/§3.4a）。一次调用加载 k 个分片 Model（shards 2..=8）并预建 lane→shard 连续分组映射；提交路径零新增（B 案裁决）——借出句柄即普通 `amv_model_t`，现有 infer 家族原样使用。本版无尾部追加，既有 `struct_size` 协商不受影响。

The inference pool orchestration container: `amv_pool_create`/`amv_pool_lane_model`/`amv_pool_close` plus `am_pool_desc_t` (contract §3.5a/§3.4a). One call loads k shard models (shards 2..=8) with a prebuilt lane→shard mapping; the submission path gains nothing (B-case ruling) — borrowed handles are ordinary `amv_model_t` used with the existing infer family. No tail append; existing struct_size negotiation unaffected.

**谁受影响 / Who is affected**：多 Model 编排用户（8 路级 GPU 直传双分片形态）与语言封装用户（Python `create_inference_pool` / C# `CreateInferencePool`）。手工多 Model 分组形态继续受支持，不受本版影响。

Multi-model orchestration users (8-lane GPU-direct dual-shard shape) and language-wrapper users. The manual multi-model grouping form remains supported.

**怎么迁 / How to migrate**：① 调用前查 `amv_get_abi_version() >= 1.4`——旧 DLL 按次版本规则**拒绝 1.4 init 语义**，且缺新符号在加载期即失败，不要对旧 DLL 直接调用三导出；② desc 全字段显式（先清零再填 `struct_size`），`shards`/`lanes` 域校验与槽位预检先于逐片 load，任一分片失败原子回滚不留半池；③ 借出句柄**禁止直调 `amv_model_close`**（`sdk.handleInvalid` 拒绝），排空关闭仅经 `amv_pool_close` 两阶段执行（超时返回 CLOSING，轮询至 CLOSED 后 destroy，重复 close 幂等）；④ 批形状按通道指导值经 `batching` 字段统一传（GPU 帧直传批 4 / CPU 像素批 2）。

Check `amv_get_abi_version() >= 1.4` first — old DLLs reject 1.4 initialization semantics per the minor-version rule and fail symbol resolution at load. Fill the desc explicitly (zero then `struct_size`); validation and the slot pre-check precede per-shard load, with atomic rollback on any shard failure. Never call `amv_model_close` on a borrowed handle (`sdk.handleInvalid`); drain only via two-stage `amv_pool_close` (timeout returns CLOSING; poll to CLOSED then destroy; repeat close is idempotent). Pass channel-guided batch shapes through `batching` (GPU-frame batch 4 / CPU-pixel batch 2).

**不迁会怎样 / If you don't migrate**：继续手工分组编排（行为语义与池内分片一致，同一 ctx 包络统一核算），功能无损；只是放弃池的一次调用建组、原子回滚与统一排空。旧绑定对 1.4+ DLL 前向兼容（v0.6.5 绑定实证）。

Keep manual grouping (semantics identical to pool shards under the same ctx envelope) — no functional loss; you only forgo one-call setup, atomic rollback and unified drain. Old bindings load 1.4+ DLLs forward-compatibly (verified with v0.6.5 bindings).

## ABI 1.4 → 1.5（v0.7.1，`AM_ABI_VERSION` 65540→65541；v0.7.2 质量批 ABI 不变）

**变了什么 / What changed**：① **显式可选**的 GPU NV12 级联输入——ROI 级联设备侧执行、源帧一次搬运，经能力位 `AMV_CAP_GPU_CASCADE` 协商；默认仍逐 ROI，二级模型攒批合批需显式 `amv_model_configure_batching` 且 `max_batch_items=4`。② 会话级严格 CUDA FP32 选项：模型后端策略 strict 取 `AM_BACKEND_STRICT_CUDA_FP32`（2），仅 CUDA 有效、禁止后端回退并为该会话关闭 TF32（与 CPU 对拍的低分歧路径；默认 TF32 下低置信度首选框存在分歧）。预算/取消/两阶段关闭语义保持。

① The **opt-in** GPU NV12 cascade input — device-side ROI cascade with one source copy, negotiated via `AMV_CAP_GPU_CASCADE`; default stays per-ROI, batch 4 requires explicit `amv_model_configure_batching`. ② A strict CUDA FP32 session option: `AM_BACKEND_STRICT_CUDA_FP32` (2) in the model backend policy — CUDA-only, forbids backend fallback and disables TF32 for that session (the low-divergence parity path vs CPU; the TF32 default diverges on low-confidence first-choice boxes). Budget/cancel/two-stage-close semantics preserved.

**谁受影响 / Who is affected**：GPU 帧输入 + 级联组合的用户（省源帧多次搬运），以及需要 CPU/CUDA 严格对拍或对低置信度分歧敏感的用户。

Users combining GPU-frame input with cascade (saving repeated source copies), and users needing strict CPU/CUDA parity or sensitive to low-confidence divergence.

**怎么迁 / How to migrate**：全部 opt-in，不迁零改动。采用级联 GPU 输入前先查能力位；遇不支持或预处理队列满，可用同画面的像素输入与 CPU 模型显式重试。对拍场景装载两级 CUDA 模型时用 strict=2 关闭对应会话 TF32；默认 TF32 部署无需变化。注意设备后端缓存与工作区需另留显存余量。

All opt-in — staying put requires no change. Capability-check before submitting GPU frames to cascade; on unsupported or full preprocessing queues, retry explicitly with pixel input and a CPU model. For parity, load both CUDA stages with strict=2 to disable TF32 for that session; default TF32 deployments need no change. Reserve extra device memory for backend caches/workspaces.

**不迁会怎样 / If you don't migrate**：行为与 1.4 一致（逐 ROI、TF32 默认），无兼容风险；仅放弃单次源帧搬运与严格对拍能力。

Behavior stays as in 1.4 (per-ROI, TF32 default) with no compatibility risk; you only forgo the single source copy and strict parity.

## ABI 1.5 → 1.6（v0.8.0，`AM_ABI_VERSION` 65542；合同 §3.5b）

**变了什么 / What changed**：设备定向选择器 `am_device_selector_t`（`AM_ABI_VERSION` 65541→65542 计划值）：`match_utf8` 大小写不敏感子串 + `instance` 实例序号（同子串第 N 个匹配，0 起）定向具体适配器——CUDA 候选匹配 CUDA 设备名，DML/OpenVINO 候选与硬解（`am_decoder_config_t.device`）匹配 DXGI 适配器 Description；匹配域排除软件适配器。selection 尾部追加选择器指针、decoder 配置定向、能力位 `AMV_CAP_DEVICE_SELECTOR`（bit12）。**默认零变化**：selector 为 NULL 或 `match_bytes=0` 时不定向，行为与 1.5 完全一致。

The device-targeted selector `am_device_selector_t` (planned `AM_ABI_VERSION` 65541→65542): case-insensitive substring `match_utf8` plus an `instance` ordinal target a specific adapter — CUDA candidates match CUDA device names; DML/OpenVINO candidates and hardware decode (`am_decoder_config_t.device`) match DXGI adapter Descriptions; software adapters (WARP) are excluded. Selection gains a tail-appended selector pointer, decoder config gains targeting, and `AMV_CAP_DEVICE_SELECTOR` (bit12) declares the capability. **Default is zero-change**: a NULL selector or `match_bytes=0` behaves exactly as 1.5.

**谁受影响 / Who is affected**：多 GPU 机器（独显+核显/同型号多实例）上依赖设备枚举序的部署——枚举序跨机器/跨重启漂移（DXGI index 0 在两次快照中分别为核显与独显的实证），不构成可解释的默认。

Deployments on multi-GPU machines relying on device enumeration order — the order drifts across machines/reboots (observed: DXGI index 0 resolved to the iGPU in one snapshot and the dGPU in another), which is not an explainable default.

**怎么迁 / How to migrate**：先查能力位与 ABI ≥1.6；跨重启稳定的定向键为"厂商名+型号子串"（如 `rtx 5060`/`arc 140t`），同型号多实例配 `instance` 消歧；selector 与 CUDA 既有 `device_id` 并存时 **selector 优先**；零命中或 `instance` 越界返回 `sdk.invalidArgument`（配置错误，不静默回退其他适配器）。定向与后端回退正交——定向核显后硬解失败仍按 PREFER_HW 回退软解。

Capability-check and require ABI ≥1.6; the reboot-stable key is a vendor+model substring (e.g. `rtx 5060`/`arc 140t`), with `instance` disambiguating same-model multiples; when both a selector and a CUDA `device_id` are given, the selector wins; zero hits or an out-of-range `instance` return `sdk.invalidArgument` (a configuration error — never a silent fallback to another adapter). Targeting is orthogonal to backend fallback chains.

**不迁会怎样 / If you don't migrate**：与 1.5 行为一致，跟随默认枚举序；在枚举序漂移的机器上可能落到非预期适配器，需自行以诊断报告核对实际后端设备。

Identical to 1.5 — you follow enumeration order, which may land on an unexpected adapter after drift; verify the actual backend device via diagnostics.

## ABI 1.6 → 1.7（随 v0.8.1 发布，`AM_ABI_VERSION` 65543；合同主文件 §2.10+附录 §2.9）

**变了什么 / What changed**：推流状态导出 `amc_stream_get_status(stream, am_stream_status_t*)`（`AM_ABI_VERSION` 65542→65543）：只读快照结构 `am_stream_status_t`——连接状态（`AM_STREAM_CONNECTED`=0 常态 / `AM_STREAM_DISCONNECTED`）、累计重连尝试、累计推送编码包载荷字节、最近一次实际等待的退避档、最近一次失败稳定码。纯新增导出+新结构，既有签名/布局/错误码零变化；write（含重连）持锁期间并发查询返回 `sdk.tryAgain`（可重试）。

Stream status export `amc_stream_get_status` (`AM_ABI_VERSION` 65542→65543): a new read-only snapshot struct `am_stream_status_t` — connection state (`AM_STREAM_CONNECTED`=0 / `AM_STREAM_DISCONNECTED`), cumulative reconnect attempts, cumulative pushed payload bytes, the last backoff tier waited, and the last failure's stable code. Purely additive (new export + new struct); existing signatures, layouts and error codes are untouched. A query concurrent with an in-flight write/reconnect returns `sdk.tryAgain` (retryable).

**谁受影响 / Who is affected**：需要推流可观测（重连/字节/故障）的 RTMP 宿主；既有 amc_stream 调用方不受影响。

RTMP hosts wanting stream observability (reconnects/bytes/faults); existing amc_stream callers are unaffected.

**怎么迁 / How to migrate**：按需调用 `amc_stream_get_status`（struct_size=sizeof 初始化）；观察值以调用边界为准，`sdk.tryAgain` 时重试；错误码沿用 `amc_status_name` 既有映射。顺带行为修复（非 ABI）：重连预算耗尽路径此前不累计重连计数、单段中断线上暴露的连接失败被归一 Timeout 使重试环单发终止——修复后拒连类失败在 15s 总预算内走完整退避梯，预算越过仍归一 `camera.timeout`。

Call `amc_stream_get_status` as needed (init struct_size=sizeof); retry on `sdk.tryAgain`; error codes use the existing `amc_status_name` mapping. Companion behavior fix (not ABI): budget-exhausted reconnect paths previously skipped attempt accounting, and connection failures surfacing on the 5s single-segment deadline were normalized to Timeout (ending the retry loop after one attempt) — refusals now run the full backoff ladder inside the 15s budget, and budget overrun still normalizes to `camera.timeout`.

**不迁会怎样 / If you don't migrate**：与 1.6 行为完全一致（不调新导出零影响）。

Exactly 1.6 behavior (zero impact without the new export).
