# FrameBraid SDK

![FrameBraid SDK — Media SDK and Vision SDK](assets/brand/framebraid-banner.webp)

[简体中文](README.zh-CN.md) · [Documentation](docs/README.md) · [Getting started](docs/GETTING_STARTED.md) · [Roadmap](docs/ROADMAP.md)

**Reusable camera, video and ONNX inference building blocks for native applications.**

FrameBraid is a Rust source SDK with a C ABI for C, C++, C# and Python. One repository contains two components:

| Component | Purpose |
| --- | --- |
| **FrameBraid Media SDK** | ISAPI images, RTSP encoded streams, decoded frames, standalone decoding and pixel conversion |
| **FrameBraid Vision SDK** | ONNX execution, bounded asynchronous work, raw tensors and profile-based postprocessing |

**Status: v0.2.0 developer preview.** [Download source and Windows x64 binaries](https://github.com/xiaoyanse/framebraid/releases/tag/v0.2.0). C ABI 1.1 (compatible additions over 1.0); the validated target is **Windows x64 CPU + hardware-decode probing**; Linux/macOS/mobile binaries and GPU inference acceptance are separate. See [validation scope](docs/RELEASE_STATUS.md).

## Where it fits

- Add camera acquisition to a desktop application or an existing Python service.
- Take individual or continuous ISAPI snapshots.
- Receive RTSP packets/frames, or decode H.264/H.265 data already acquired by another system.
- Run compatible ONNX models across languages through one native interface.
- Combine acquisition and inference, optionally sharing frames between trusted local processes.

The host owns device configuration, credential storage, business rules, persistence, alarms and UI. This is not a complete monitoring product, browser player, video editor or safety-certified control system.

## Media SDK

- RTSP H.264/H.265/MJPEG handling; encoded-packet and decoded-frame subscriptions.
- Standalone H.264/H.265 decoding from **complete Annex B access units**. Arbitrary network fragments are not complete-frame inputs.
- ISAPI HTTP Digest snapshots and bounded continuous acquisition. Actual rate depends on the camera and network.
- Pull/callback interfaces, bounded queues, metadata, scaling, pixel conversion and reconnect controls.
- Explicit hardware decode probing and automatic backend selection (D3D11VA preferred, DXVA2 where mapped, bounded CPU fallback); per-context worker budgets with live/peak statistics. Hardware readback is not guaranteed faster than CPU; consult the measured contracts.
- Advanced Rust RTSP callers can use `Reader::open_with_budget` and `read_packet_with_budget` to reserve SDK configuration/framing workspace before allocation. Rust callers can also inspect Annex B NAL units with `am_camera::media::codec::visit_access_units` without allocating a NAL list; commit caller state only after overall success.
- Stream probing from explicit endpoints. Rust `CameraEndpoint` generates credential-free ISAPI/main/sub addresses; this helper is not a separate C ABI device-management API.
- Explicit runtime directories and caller-injected credentials; no credential persistence.

Check runtime capabilities and contracts. H.265 decoding does not imply browser H.265 playback.

## Vision SDK

- ONNX Runtime integration, image preprocessing and structured results.
- Five **constrained model profiles**: Pose, Detect, Classify, Segment and OBB.
- Raw named tensor inputs/outputs for custom models within supported dtype/shape contracts.
- Synchronous, asynchronous and batch interfaces, status, notifications, acknowledgement and release.
- **Adaptive batching** (v0.2.0): explicit, bounded, off by default; aggregates AUTO submissions of compatible items on dynamic-batch-axis models into single physical batches. Fixed-batch-axis models are rejected rather than padded.
- CPU/OpenVINO/CUDA selection with explicit device probing, allowlists and strict/fallback policies, plus layered execution diagnostics. CPU is validated here; other backends need separate dependencies and acceptance.
- Owned pixel inputs across languages: Python `owner.pixels().infer(...)` / C# `owner.Pixels().Submit(...)` keep the owner alive and locked through submission; async inputs use the default copy policy unless retained explicitly.
- Bounded model notification buffers (256 hints, drop-oldest) in Python/C# helpers; retain request IDs to query results after drops.
- Optional shared frames, enabled at build time and explicitly at initialization.

This is not every YOLO version/export. Match the [profile catalog](crates/vision/resources/profiles.json). Model weights and their licenses are separate.

## Build from source

If your downloaded Windows package contains `bin/win-x64`, use the [prebuilt guide](docs/USE_PREBUILT.md) without compiling.

Windows x64: Rust 1.90+, x64 MSVC build tools and Windows SDK. Fetch locked Cargo dependencies on the first build. Node.js/Python are for maintenance tools or Python examples, not prebuilt DLL loading.

Run from the repository root, keeping output outside the checkout:

```powershell
$env:CARGO_TARGET_DIR = (Join-Path $PWD '.local/build/current')
cargo build --workspace --locked
cargo build --release --locked -p am-camera-ffi -p am-vision-ffi --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames
```

The Release command outputs `am_camera.dll`, `am_vision.dll` and import libraries in `.local/build/current/release`. Historical filenames, `amc_*`/`amv_*` symbols and crate names remain for compatibility; they do not require the original host application.

FFmpeg and ONNX Runtime are dynamically loaded. Pass compatible absolute runtime directories at initialization. A build does not prove runtime availability. See [getting started](docs/GETTING_STARTED.md) for a real-image call and dependency troubleshooting.

## Language integration

| Language | Entry point |
| --- | --- |
| Rust | Local dependencies on `crates/am-camera` and `crates/vision`; not published to crates.io |
| C / C++ | [Media header](sdk/include/am_camera.h), [Vision header](sdk/include/am_vision.h), [examples](sdk/examples/README.md) |
| C# | [Bindings](sdk/bindings/csharp), SafeHandle helpers and examples |
| Python | [Bindings](sdk/bindings/python), standard-library `ctypes` and context managers |

Choose static Rust consumption or DLL consumption in one process, not both runtime domains. Java wrappers, Linux/macOS/mobile binaries and GPU-inference acceptance are outside the initial Windows commitment.

## Ownership, concurrency and failures

- Release SDK allocations through the owning SDK, not caller `free`/`delete`.
- Request acknowledgement releases cache responsibility, not acquired result handles.
- Keep callbacks short, retain wrapper references and obey reentrancy restrictions.
- Queues/budgets are bounded: handle rejection, backpressure, stale frames and reconnect generations. No universal FPS, unlimited cameras or lossless delivery is promised.
- Preserve frame identity/timestamps when joining video and results. Display predictions are not new inference results.
- Drain children, callbacks and in-flight work before closing contexts/unloading SDK DLLs. Underlying FFmpeg/ORT may remain process-resident.

See the [integration guide](sdk/INTEGRATION.md) and [ABI contracts](docs/design/dual-dll-abi.md).

## Validation and boundaries

The v0.2.0 Windows x64 **Release candidate** passed **384 workspace Rust tests** (20 environment-gated skipped), generator-chain zero-drift checks, ABI freeze (11 artifacts) and source-fingerprint boundary checks (239 files). Clean-package validation covers C/C++17/C#/Python examples plus the adaptive-batching live suite against the package's own DLLs. Hardware decode matrix (H.264/H.265, 360p–4K incl. Main10) is D3D11VA-verified with software controls; RTSP fault matrix (stream-loss resume, in-flight close) passed. Four of five real model classes were not rerun end-to-end (no local weights); CUDA/OpenVINO execution providers, field streams and long-duration acceptance remain registered as unverified. See the [v0.2.0 validation summary](https://github.com/xiaoyanse/framebraid/releases/tag/v0.2.0) (VALIDATION.md asset) and [status](docs/RELEASE_STATUS.md).

The source checkout excludes runtime binaries, weights, credentials, business database and host UI. The separate Windows package includes matching CPU runtimes and their source/license materials. The extraction manifest is historical provenance, not a continuously updated release manifest; core files remain fingerprint-checked.

See [support boundaries](docs/SUPPORT.md), [current local validation](docs/RELEASE_STATUS.md) and [release checklist](docs/RELEASE_CHECKLIST.md). Historical plans do not supersede actual evidence.

## Test observations

![Historical SDK reload stability](assets/benchmarks/reload-stability.png)

| Recorded observation | Result | Scope |
| --- | --- | --- |
| FrameBraid Rust tests | 384 passed; 20 environment-gated skipped | v0.2.0 candidate, Windows x64 Release |
| DLL/script validation groups | Clean-package four-language + batching live suite | v0.2.0 package, CPU and language calls |
| DLL reload cycles | 30; 157 process handles throughout | Historical pre-extraction SDK |
| Post-warm-up process private-memory net growth | 0.69 MiB over 25 cycles | Historical pre-extraction SDK |
| Queue pressure / retained owners | 37 accepted, 14 rejected; 37 retain / 37 release | Historical synthetic CPU test |

Historical figures are not current FrameBraid Release performance; per-run hardware/binary identity is incomplete. No FPS or inference-latency claim is derived. See [data, queue chart and methodology](docs/BENCHMARKS.md).

For current local measurements, use the [reproducible performance tools](docs/PERFORMANCE.md).
Performance acceptance follows [MET-2](docs/PERFORMANCE_ACCEPTANCE.md): CPU offload, useful delivery, latency and recovery have separate goals, and historical verdicts are annotated in the [review record](docs/verification/sdk-metrics-review-20261026.md).

## Contribute and report

Read [CONTRIBUTING.md](CONTRIBUTING.md). Do not post credentials, private stream URLs or customer images. Security reports follow [SECURITY.md](SECURITY.md). See [CHANGELOG.md](CHANGELOG.md).

## License

Copyright 2026 [xiaoyanse](https://github.com/xiaoyanse).

Original SDK code uses [Apache-2.0](LICENSE), allowing commercial and closed-source use subject to its terms. FFmpeg, ONNX Runtime, models and other dependencies retain their licenses. See [NOTICE](NOTICE) and [third-party materials](sdk/third-party/README.md).
