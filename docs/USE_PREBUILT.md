# Using the Windows package / 使用 Windows 预编译包

[English README](../README.md) · [中文 README](../README.zh-CN.md)

This guide covers the current Windows x64 SDK packages — from v0.2.0 through the
current release (v0.8.0, C ABI 1.6) — which unpack to a single `sdk/` tree with
`LICENSE` and `build-record.json` beside it. Packages are downloaded from
[GitHub Releases](https://github.com/xiaoyanse/framebraid/releases). Runtime
libraries and model weights are **not** bundled. The historical v0.1.0 preview
used a different four-directory layout; its original guide is preserved as the
appendix at the end of this document.

本指南面向 v0.2.0 至当前版本（v0.8.0，C ABI 1.6）的 Windows x64 SDK 包：解压后为单一 `sdk/` 树，旁附 `LICENSE` 与 `build-record.json`；运行库与模型权重**不随包**。历史 v0.1.0 预览包为四目录布局，其原始指南保留为文末附录。包从 [GitHub Releases](https://github.com/xiaoyanse/framebraid/releases) 下载。

## 1. Package contents / 包内容

`FrameBraid-SDK-windows-x64-<version>.zip` unpacks to one directory:

`FrameBraid-SDK-windows-x64-<版本>.zip` 解压后为一个目录：

```text
<package root>/
├── LICENSE
├── build-record.json
└── sdk/
    ├── am_camera.dll        # Media SDK: RTSP/ISAPI/FILE sources, decode
    ├── am_vision.dll        # Vision SDK: inference
    ├── include/
    │   ├── am_camera.h
    │   ├── am_sdk_types.h
    │   └── am_vision.h
    ├── bindings/
    │   ├── python/          # amrt_sdk.py · safe_sdk.py · load_diagnostics.py
    │   └── csharp/          # 12 .cs sources, compiled into your project
    ├── INTEGRATION.md       # full integration guide (same doc inside the package)
    ├── RELEASE-NOTES.md     # per-release notes (same doc inside the package)
    └── SHA256SUMS.txt       # SHA-256 of the two DLLs
```

In the package: the two DLLs, the C headers, the Python/C# binding sources and
the two integration documents. Not in the package: FFmpeg / ONNX Runtime runtime
libraries, import libraries, model weights, the model-profile catalog and the
example sources. Runtimes are staged by you (section 3), models are obtained
under your own license (section 5), and examples live in the source archive on
the same release page.

包内只有两个 DLL、C 头文件、Python/C# 绑定源码与两份接入文档。不随包：FFmpeg / ONNX Runtime 运行库、导入库、模型权重、模型档案目录与示例源码。运行库自行准备（第 3 节），模型自行取得授权（第 5 节），示例在同发布页的源码包里。

## 2. Paths / 路径

Unzip, enter the package root, then set:

解压后进入包根目录，在 PowerShell 设置：

```powershell
$PKG_ROOT = (Get-Location).Path
$SDK_ROOT = Join-Path $PKG_ROOT 'sdk'
$DLL_DIR  = $SDK_ROOT   # both DLLs sit directly in sdk/
```

The `INTEGRATION.md` examples take an interface directory and a DLL directory as
separate arguments; in this package both resolve to `sdk/`. All applications and
DLLs must be x64, and a compatible Microsoft VC++ 2015–2022 x64 redistributable
may be required. Rust, the FFmpeg command-line tools and a Python onnxruntime
package are not needed to consume the DLLs.

包内 INTEGRATION.md 示例把"接口目录"与"DLL 目录"作为两个参数传入；本包中两者都落到 `sdk/`。进程与依赖均为 x64，可能需兼容的 Microsoft VC++ 2015–2022 x64 运行库；直接用 DLL 不要求 Rust、FFmpeg 命令行工具或 Python onnxruntime 包。

## 3. Runtime libraries / 运行库（不随包，调用方自备）

Runtime libraries are passed **explicitly as absolute directories** at
initialization; relying on the global PATH is not part of the contract:

运行库在初始化时**显式传入绝对目录**；不把全局 PATH 兜底当作合同的一部分：

| SDK | Init parameter | Runtime DLLs |
| --- | --- | --- |
| camera | `runtime_dirs_utf8 = [FFMPEG_DIR]` | FFmpeg shared build: `avcodec-63`, `avformat-63`, `avutil-61`, `swresample-7`, `swscale-10` |
| vision | `runtime_dirs_utf8 = [ORT_DIR]` | ONNX Runtime CPU line `onnxruntime.dll` |

- Keep the FFmpeg and ORT directories separate; do not mix them into one
  directory or swap versions silently. Version compatibility is defined by the
  matrix in [SUPPORT](SUPPORT.md).
- Product-read environment variables (`AM_RT_FFMPEG_DIR`, `AM_RT_ORT_LIBRARY`,
  `AM_RT_ORT_GPU_LIBRARY`, `AM_SDK_OPENVINO_DIR`, `AM_SDK_CUDA_DIR`,
  `AM_SDK_ORT_DIR`) are registered in [ENVVARS](ENVVARS.md); the explicit
  `runtime_dirs` parameter remains the primary channel.
- GPU deployment: the DirectML-line ORT and the CPU/CUDA-line ORT are mutually
  exclusive within one process — DML runs in a dedicated process whose PATH
  excludes the CUDA-line ORT directory (verified in v0.8.0; see the package's
  RELEASE-NOTES.md). CUDA interop additionally resolves `cudart64_*.dll` via
  `AM_SDK_CUDA_DIR` / `AM_SDK_ORT_DIR`.
- Shared-frames exception: with `AM_INIT_SHARED_FRAMES` a vision context can
  perform attach/borrow/return without any runtime directory, but cannot load
  models or run inference.
- Licensing: decoding and inference work with an LGPL-minimal FFmpeg build;
  encoding or RTMP streaming needs a GPL build with libx264 — see
  [runtime licensing](RUNTIME-LICENSING.md).

- FFmpeg 与 ORT 目录分开，不混装一个目录、不静默换版本；版本兼容以 [SUPPORT](SUPPORT.md) 矩阵为准。
- 产品读取的环境变量（`AM_RT_FFMPEG_DIR`、`AM_RT_ORT_LIBRARY`、`AM_RT_ORT_GPU_LIBRARY`、`AM_SDK_OPENVINO_DIR`、`AM_SDK_CUDA_DIR`、`AM_SDK_ORT_DIR`）统一登记于 [ENVVARS](ENVVARS.md)；显式 `runtime_dirs` 参数仍是主通道。
- GPU 部署：DML 线 ORT 与 CPU/CUDA 线 ORT 同进程互斥——DML 用专用进程，其 PATH 不挂 CUDA 线 ORT 目录（v0.8.0 实证，见包内 RELEASE-NOTES.md）；CUDA 互操作另经 `AM_SDK_CUDA_DIR` / `AM_SDK_ORT_DIR` 解析 `cudart64_*.dll`。
- 共享帧例外：启用 `AM_INIT_SHARED_FRAMES` 的 vision context 可在无运行库目录下执行 attach/borrow/return，但不能加载模型或推理。
- 许可：纯解码/推理用 LGPL minimal 档案 FFmpeg 即可；编码或 RTMP 推流需含 libx264 的 GPL 构建，见[运行库许可选择指引](RUNTIME-LICENSING.md)。

## 4. Bindings / 绑定

**Python** — standard-library `ctypes` only; no onnxruntime Python package.
Copy the whole `sdk/bindings/python` directory, not just `safe_sdk.py` (it
imports `amrt_sdk.py` and `load_diagnostics.py` from the same directory):

**Python**——仅用标准库 ctypes，不需要 onnxruntime Python 包。复制整个 `sdk/bindings/python` 目录，不要只复制 `safe_sdk.py`（它依赖同目录的 `amrt_sdk.py` 与 `load_diagnostics.py`）：

```python
import sys
from pathlib import Path
sdk, dlls, ort = map(Path, sys.argv[1:4])
sys.path.insert(0, str(sdk / 'bindings/python'))
from safe_sdk import VisionContext

with VisionContext(dlls / 'am_vision.dll', [ort]) as vision:
    ...  # load_model / infer_encoded / ack — package INTEGRATION.md §4
```

**C#** — compile **all** `sdk/bindings/csharp/*.cs` into your x64 project; no
NuGet dependency (the .NET Framework x64 `csc.exe` also works). P/Invoke is
Cdecl and strings are converted to UTF-8 by the wrappers. The full walkthrough,
including a `csc` command line, is in the package's INTEGRATION.md §5.

**C#**——把 `sdk/bindings/csharp/*.cs` **全部文件**编入你的 x64 工程；无 NuGet 依赖（.NET Framework x64 `csc.exe` 亦可用）。P/Invoke 为 Cdecl，字符串由封装转 UTF-8。含 `csc` 命令行的完整流程见包内 INTEGRATION.md §5。

**C/C++** — headers are in `sdk/include`. Current packages ship **no import
libraries**: load the DLLs dynamically (`LoadLibraryEx`/`GetProcAddress`) or
generate import libraries with your own toolchain, keeping both DLLs beside the
EXE. The call order is in the package's INTEGRATION.md §6.

**C/C++**——头文件在 `sdk/include`。现行包**不带导入库**：用系统动态装载（`LoadLibraryEx`/`GetProcAddress`）或用自有工具链自行生成导入库，并把两个 DLL 放在 EXE 旁。调用顺序见包内 INTEGRATION.md §6。

SDK DLL discovery is separate from the runtime-library directories: Python uses
the absolute DLL path; C# P/Invoke resolves the SDK DLLs from the application
directory or the process PATH; C/C++ loads them from beside the EXE. Runnable
example sources (`camera.py`, `inference.*`, C# sample programs) live in the
source archive, not in the package; the snippets in INTEGRATION.md are
self-contained.

SDK DLL 的查找与运行库目录是两件事：Python 用绝对路径；C# P/Invoke 从程序目录或进程 PATH 解析；C/C++ 从 EXE 旁装载。可运行示例源码（`camera.py`、`inference.*`、C# 示例程序）在源码包里而不在包内；包内 INTEGRATION.md 的片段可独立使用。

## 5. Models / 模型（不随包）

Model weights are not included; you obtain them and their licenses separately.
Inference uses the five fixed profile families — `pose-yolo26-e2e-coco17-v1`,
`detect-yolo11n-nms-v1`, `classify-yolo11n-prob-v1`, `segment-yolo11n-nms-v1`,
`obb-yolo11n-nms-v1` — and the catalog (`crates/vision/resources/profiles.json`
in the source archive) defines the matching export parameters. This is not an
"any-YOLO-is-compatible" promise; see the package's INTEGRATION.md §3.

模型权重不随包，须自行取得并匹配许可。推理走五类固定档案——`pose-yolo26-e2e-coco17-v1`、`detect-yolo11n-nms-v1`、`classify-yolo11n-prob-v1`、`segment-yolo11n-nms-v1`、`obb-yolo11n-nms-v1`——档案目录（源码包内 `crates/vision/resources/profiles.json`）定义匹配的导出参数。不存在"任何 YOLO 自动兼容"承诺，见包内 INTEGRATION.md §3。

## 6. Integrity / 完整性

- `sdk/SHA256SUMS.txt` covers the two DLLs; `build-record.json` records the
  source commit, release tag, locked Release command, compiler and per-input
  SHA-256 hashes.
- The release page additionally carries `SHA256SUMS.txt` and
  `build-record.json` for the published assets, plus the bare DLLs and the
  source archive.
- Before ABI and capability negotiation, match the actually loaded DLL files to
  the release checksums; the version/identity entry points are described in the
  package's INTEGRATION.md §2.1.

- `sdk/SHA256SUMS.txt` 覆盖两个 DLL；`build-record.json` 记录源码提交、发布 tag、锁定的 Release 命令、编译器与逐输入 SHA-256。
- 发布页另有制品级 `SHA256SUMS.txt` 与 `build-record.json`，以及裸 DLL 与源码包。
- ABI 与能力协商前，先核对实际加载的 DLL 文件与发布校验和一致；版本/身份入口见包内 INTEGRATION.md §2.1。

## Appendix: the v0.1.0 preview package (historical) / 附录：v0.1.0 预览包（历史）

The [v0.1.0 developer
preview](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0) used a
four-directory layout — `bin/win-x64`, `lib/win-x64`, `runtime/ffmpeg` and
`runtime/onnxruntime-cpu` — with the runtime libraries bundled and the same SDK
source tree as the separate source ZIP included. Everything below describes
**that package only**; do not apply these paths to v0.2.0+ packages.

[v0.1.0 开发者预览包](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0)为四目录布局——`bin/win-x64`、`lib/win-x64`、`runtime/ffmpeg`、`runtime/onnxruntime-cpu`——运行库随包，并附与独立源码包相同的 SDK 源码树。以下内容**仅适用于该包**，不得套用到 v0.2.0+ 包。

### A.1 Paths / 路径

Unpack to your own directory, enter that directory, then set:

解压到自己的目录并进入，在 PowerShell 设置：

```powershell
$PKG_ROOT = (Get-Location).Path
$SDK_ROOT = Join-Path $PKG_ROOT 'sdk'
$DLL_DIR = Join-Path $PKG_ROOT 'bin/win-x64'
$LIB_DIR = Join-Path $PKG_ROOT 'lib/win-x64'
$FFMPEG_DIR = Join-Path $PKG_ROOT 'runtime/ffmpeg'
$ORT_DIR = Join-Path $PKG_ROOT 'runtime/onnxruntime-cpu'
```

All applications and DLLs must be x64. Rust, the FFmpeg command-line program,
a Python onnxruntime package and the original host application are not required
for DLL consumption. Compatible Microsoft VC++ runtime may still be needed.
Native runtime lookup is explicit, separate from SDK DLL lookup.

进程与依赖均为 x64。直接用 DLL 不要求 Rust、FFmpeg CLI、Python onnxruntime
或原宿主；可能仍需兼容 VC++ 运行库。SDK 与原生运行库分开查找。

### A.2 Call / 调用

Use the Python real-image code in [getting started](GETTING_STARTED.md) from
step 3 onward, with these directory variables instead of Cargo output paths.
The same source/example/profile constraints apply. C/C++ and C# commands are
in [integration](../sdk/INTEGRATION.md); use this package's separate import-library
directory when compiling C/C++.

Python 从快速接入第 3 步开始使用以上变量，不再执行 Cargo 构建。
C/C++、C# 见完整接入，编译时 C/C++ 指向独立的 LIB_DIR。
模型不随包提供，须自己取得许可并匹配档案。

### A.3 Integrity and materials / 完整性与材料

- `artifact-manifest.json`: relative filenames, sizes, SHA-256 and source commit.
- `build-record.json`: actual locked Release invocation and compile-input hashes.
- `licenses/`: matching native notices, FFmpeg corresponding source/build materials
  and a copy of the Cargo license inventory/texts.
- `sdk/third-party/rust/`: reproducible dependency notice collection, including a
  conservative superset of resolved build dependencies.

清单只记录相对路径与来源指纹，不包含私有环境快照。准备材料不等于授权发布。
运行结果与具体硬件/负载另行验收，不据一次本地通过承诺长期、GPU 或现场行为。
