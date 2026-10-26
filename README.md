# FrameBraid SDK

![FrameBraid SDK — Media SDK and Vision SDK](assets/brand/framebraid-banner.webp)

[简体中文](README.zh-CN.md) · [Documentation](docs/README.md) · [Getting started](docs/GETTING_STARTED.md) · [Roadmap](docs/ROADMAP.md)

**Reusable camera, video and ONNX inference building blocks for native applications.**

Local R2 work adds Rust hardware probes and direct C decoder automatic selection/recovery ([contract](docs/design/native-hardware.md), [local verification](docs/verification/sdk-r2-direct-auto-20261026.md)). Published assets remain CPU validated; RTSP automatic hardware selection and full device diagnostics remain incomplete.

Local main now develops C ABI 1.1 probe/report extensions ([verification](docs/verification/sdk-r2-abi11-hardware-20261026.md)). Use matching source-built DLLs and bindings; v0.1.0 binaries still use ABI 1.0 and require their tagged bindings. This work is not published.

FrameBraid is a Rust source SDK with a C ABI for C, C++, C# and Python. One repository contains two components:

| Component | Purpose |
| --- | --- |
| **FrameBraid Media SDK** | ISAPI images, RTSP encoded streams, decoded frames, standalone decoding and pixel conversion |
| **FrameBraid Vision SDK** | ONNX execution, bounded asynchronous work, raw tensors and profile-based postprocessing |

**Status: v0.1.0 developer preview.** [Download source and Windows x64 binaries](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0). The initial validated target is **Windows x64 CPU**; Linux/macOS/mobile binaries and GPU acceptance are separate. See [validation scope](docs/RELEASE_STATUS.md).

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
- Explicit runtime directories and caller-injected credentials; no credential persistence.
- Stream probing from explicit endpoints. Rust `CameraEndpoint` generates credential-free ISAPI/main/sub addresses; this helper is not a separate C ABI device-management API.

Check runtime capabilities and contracts. H.265 decoding does not imply browser H.265 playback.

## Vision SDK

- ONNX Runtime integration, image preprocessing and structured results.
- Five **constrained model profiles**: Pose, Detect, Classify, Segment and OBB.
- Raw named tensor inputs/outputs for custom models within supported dtype/shape contracts.
- Synchronous, asynchronous and batch interfaces, status, notifications, acknowledgement and release.
- CPU/OpenVINO/CUDA selection and diagnostics. CPU is validated here; other backends need separate dependencies and acceptance.
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

Choose static Rust consumption or DLL consumption in one process, not both runtime domains. Java wrappers, Linux/macOS/mobile binaries and hardware-decoding acceptance are outside the initial Windows commitment.

## Ownership, concurrency and failures

- Release SDK allocations through the owning SDK, not caller `free`/`delete`.
- Request acknowledgement releases cache responsibility, not acquired result handles.
- Keep callbacks short, retain wrapper references and obey reentrancy restrictions.
- Queues/budgets are bounded: handle rejection, backpressure, stale frames and reconnect generations. No universal FPS, unlimited cameras or lossless delivery is promised.
- Preserve frame identity/timestamps when joining video and results. Display predictions are not new inference results.
- Drain children, callbacks and in-flight work before closing contexts/unloading SDK DLLs. Underlying FFmpeg/ORT may remain process-resident.

See the [integration guide](sdk/INTEGRATION.md) and [ABI contracts](docs/design/dual-dll-abi.md).

## Validation and boundaries

The local Windows x64 **Release candidate** passed **177 Rust tests (3 ignored, 8 filtered)** and **11 packaged-DLL test groups**, including C/C++17/C#/Python calls. Five-family postprocessing has unit coverage; all five real-weight models were not rerun end-to-end in this pass. These results are not field, GPU or long-duration acceptance. See [candidate identity and results](docs/RELEASE_STATUS.md).

The source checkout excludes runtime binaries, weights, credentials, business database and host UI. The separate Windows package includes matching CPU runtimes and their source/license materials. The extraction manifest is historical provenance, not a continuously updated release manifest; 159 core files remain fingerprint-checked.

See [support boundaries](docs/SUPPORT.md), [current local validation](docs/RELEASE_STATUS.md) and [release checklist](docs/RELEASE_CHECKLIST.md). Historical plans do not supersede actual evidence.

## Test observations

![Historical SDK reload stability](assets/benchmarks/reload-stability.png)

| Recorded observation | Result | Scope |
| --- | --- | --- |
| FrameBraid Rust tests | 177 passed; 3 ignored; 8 filtered | Local candidate, Windows x64 Release |
| FrameBraid DLL/script groups | 11 passed | Local Release candidate, CPU and language calls |
| DLL reload cycles | 30; 157 process handles throughout | Historical pre-extraction SDK |
| Post-warm-up process private-memory net growth | 0.69 MiB over 25 cycles | Historical pre-extraction SDK |
| Queue pressure / retained owners | 37 accepted, 14 rejected; 37 retain / 37 release | Historical synthetic CPU test |

Historical figures are not current FrameBraid Release performance; per-run hardware/binary identity is incomplete. No FPS or inference-latency claim is derived. See [data, queue chart and methodology](docs/BENCHMARKS.md).

For current local measurements, use the [reproducible performance tools](docs/PERFORMANCE.md).
The R1 CPU/synthetic baseline is separate from public release benchmarks and hardware acceptance.

## Contribute and report

Read [CONTRIBUTING.md](CONTRIBUTING.md). Do not post credentials, private stream URLs or customer images. Security reports follow [SECURITY.md](SECURITY.md). See [CHANGELOG.md](CHANGELOG.md).

## License

Copyright 2026 [xiaoyanse](https://github.com/xiaoyanse).

Original SDK code uses [Apache-2.0](LICENSE), allowing commercial and closed-source use subject to its terms. FFmpeg, ONNX Runtime, models and other dependencies retain their licenses. See [NOTICE](NOTICE) and [third-party materials](sdk/third-party/README.md).
