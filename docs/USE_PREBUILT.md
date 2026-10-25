# Using the Windows package / 使用 Windows 预编译包

[English README](../README.md) · [中文 README](../README.zh-CN.md)

This guide applies to the **Windows x64 v0.1.0 developer preview** containing
`bin/win-x64`, `lib/win-x64`, `runtime/ffmpeg` and `runtime/onnxruntime-cpu`.
Download from [GitHub Releases](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0). The package includes the same SDK source tree
as the separate source ZIP so documentation/profile paths and rebuild inputs
remain consistent. You do not need to compile that source to use the DLLs.

适用于含上述目录的 Windows x64 开发者预览包，从 GitHub Releases 下载。
Windows 包附同源源码以便文档和复建保持一致；直接用 DLL 无需编译源码。

## 1. Paths / 路径

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

## 2. Call / 调用

Use the Python real-image code in [getting started](GETTING_STARTED.md) from
step 3 onward, with these directory variables instead of Cargo output paths.
The same source/example/profile constraints apply. C/C++ and C# commands are
in [integration](../sdk/INTEGRATION.md); use this package's separate import-library
directory when compiling C/C++.

Python 从快速接入第 3 步开始使用以上变量，不再执行 Cargo 构建。
C/C++、C# 见完整接入，编译时 C/C++ 指向独立的 LIB_DIR。
模型不随包提供，须自己取得许可并匹配档案。

## 3. Integrity and materials / 完整性与材料

- `artifact-manifest.json`: relative filenames, sizes, SHA-256 and source commit.
- `build-record.json`: actual locked Release invocation and compile-input hashes.
- `licenses/`: matching native notices, FFmpeg corresponding source/build materials
  and a copy of the Cargo license inventory/texts.
- `sdk/third-party/rust/`: reproducible dependency notice collection, including a
  conservative superset of resolved build dependencies.

清单只记录相对路径与来源指纹，不包含私有环境快照。准备材料不等于授权发布。
运行结果与具体硬件/负载另行验收，不据一次本地通过承诺长期、GPU 或现场行为。
