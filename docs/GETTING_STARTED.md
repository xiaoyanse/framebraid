# Getting started

[中文](GETTING_STARTED.zh-CN.md) · [Documentation](README.md)

Use the Windows x64 source checkout or [v0.1.0 download](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0). A scoped local Release candidate was verified; see [candidate identity and results](RELEASE_STATUS.md). The commands below build your own binaries and do not transfer that acceptance to another build.

## 1. Build

Install Rust 1.90+, MSVC x64 build tools and Windows SDK. From the repository root:

```powershell
$env:CARGO_TARGET_DIR = 'D:/FrameBraid-build'
cargo build --release --locked -p am-camera-ffi -p am-vision-ffi --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames
$SDK_ROOT = Join-Path (Get-Location).Path 'sdk'
$DLL_DIR = Join-Path $env:CARGO_TARGET_DIR 'release'
```

DLLs/import libraries appear in `$DLL_DIR`. Cargo must first obtain the locked dependencies. Building does not connect cameras.

## 2. Provide native dependencies

- Media uses compatible FFmpeg shared libraries. The reviewed Windows ABI expects `avcodec-63`, `avformat-63`, `avutil-61`, `swresample-7` and `swscale-10`.
- Real inference needs compatible ONNX Runtime. Recorded CPU validation uses 1.24.1 with the exact provenance in the third-party profile; arbitrary same-version binaries are not automatically validated.
- Caller, SDK and dependencies must all be x64; a compatible Microsoft Visual C++ Redistributable may be needed.
- SDK DLL lookup and FFmpeg/ORT lookup are separate. Pass absolute runtime directories at initialization. Do not copy system DLLs or depend on a development machine's global PATH.
- Runtimes and weights are not in the source checkout. See [third-party materials](../sdk/third-party/README.md).

Example locations supplied by you:

```powershell
$ORT_DIR = 'C:/FrameBraid-runtime/onnxruntime-cpu'
$FFMPEG_DIR = 'C:/FrameBraid-runtime/ffmpeg'
```

Vision shared-frame-only initialization can omit ORT when compiled and explicitly enabled, but cannot load models/infer. Providing a runtime directory triggers strict initialization.

## 3. Run Pose on your image

Use an authorized model matching `pose-yolo26-e2e-coco17-v1` in the [profile catalog](../crates/vision/resources/profiles.json). Match export version, shapes and preprocessing, not just a filename.

Save as `run_pose.py` in your application's directory:

```python
import sys
from pathlib import Path

sdk, dlls, ort, weights, image = map(Path, sys.argv[1:])
sys.path.insert(0, str(sdk / "bindings/python"))
from safe_sdk import VisionContext

with VisionContext(dlls / "am_vision.dll", [ort]) as vision:
    with vision.load_model(weights, "pose-yolo26-e2e-coco17-v1") as model:
        request_id, result = model.infer_encoded(image.read_bytes())
        with result:
            try:
                print("Result task:", result.view.task)
                # Read result.view.pose while result is alive; copy to retain data.
            finally:
                vision.ack(request_id)
```

Replace application, model and image paths:

```powershell
python 'C:/MyApp/run_pose.py' "$SDK_ROOT" "$DLL_DIR" "$ORT_DIR" 'C:/Models/pose.onnx' 'C:/Images/input.jpg'
```

The wrapper uses standard-library `ctypes`, not the Python onnxruntime package. Keep the entire bindings directory. PNG uses `png=True`. Async calls use `model.infer_encoded(data, asynchronous=True)` followed by `vision.wait(id)`, `fetch(id)`, `ack(id)`; release every fetched result. Timeout does not cancel: keep the request ID and query again or cancel explicitly.

Existing `inference.py/.c/.cpp` and `Inference.cs` examples use an exact `1×3×2×2` FLOAT Identity model, not arbitrary YOLO weights. Test model generators require Python `onnx`; SDK runtime does not.

## 4. Try a camera operation

Use only an authorized device. Inject endpoint/credentials securely through your host. The example reads `AM_SDK_SOURCE_URL`, `AM_SDK_USERNAME`, `AM_SDK_PASSWORD`. Do not embed secrets in URLs, code or reports.

```powershell
python "$SDK_ROOT/examples/camera.py" --dll-dir "$DLL_DIR" --runtime-dir "$FFMPEG_DIR"
# Explicit RTSP decoded-frame acquisition:
python "$SDK_ROOT/examples/camera.py" --dll-dir "$DLL_DIR" --runtime-dir "$FFMPEG_DIR" --rtsp
```

These commands acquire one ISAPI JPEG or one decoded RTSP frame; this is not a continuous player. Continuous capture, encoded subscriptions and direct AU decoding use lower-level interfaces.

## 5. Other languages and lifecycle

C/C++ and C# commands and full shutdown order are in [integration](../sdk/INTEGRATION.md) and [examples](../sdk/examples/README.md).

SDK-owned results/frames need SDK release. Acknowledgement does not replace result release. Close children before contexts; a timeout does not authorize destroying live handles or unloading DLLs with callbacks in flight.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| DLL cannot load before initialization | Absolute SDK path, x64 bitness, transitive dependencies and original OS error |
| Windows error 126 | A dependency is unresolved; this code alone does not identify it |
| Runtime init fails | Explicit directory, ABI/version and provider dependencies; retain structured error |
| Model rejected | Profile, export settings, dtype and every input/output shape |
| Timeout or queue rejection | Retain request IDs, inspect budgets/status, ack completed work and release results |
| No usable RTSP frames | Codec/capabilities, keyframes, reconnect generation and freshness |

Do not fix loader errors with random DLL copies or silently change backends when dependencies are missing.
