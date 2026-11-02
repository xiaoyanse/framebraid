# Runtime environment variables / 运行时环境变量登记表

All `AM_RT_*` / `AM_SDK_*` environment variables are registered here. A new variable must be
added to this table (name / purpose / default / doc link) before it can merge — variables that
exist only in code without a row here are a defect (E1-class incident: `AM_SDK_OPENVINO_DIR`
was load-bearing but undocumented until v0.3.2).
/ 所有 `AM_RT_*` / `AM_SDK_*` 环境变量集中本表登记；新增变量必须先过表（名称/作用/默认/文档链接四列齐全）
才允许合入。只存在代码里而无表行的变量视为缺陷（E1 类事故：`AM_SDK_OPENVINO_DIR` 长期承重却无文档，
v0.3.2 起补记）。

## Runtime behavior variables / 运行时行为变量（产品代码读取）

| Name | Purpose / 作用 | Default / 默认 | Doc / 文档 |
| --- | --- | --- | --- |
| `AM_RT_FFMPEG_DIR` | FFmpeg 运行库目录（搜索链首位：env → exe 旁 → 向上五级 `vendor/ffmpeg/win-x64/bin`） | unset → 搜索链回退 | [SUPPORT](SUPPORT.md) |
| `AM_RT_ORT_LIBRARY` | ORT 运行库显式 DLL 路径（CPU 线，vision backend 定位链） | unset → 目录探测 | [SUPPORT](SUPPORT.md) |
| `AM_RT_ORT_GPU_LIBRARY` | ORT GPU 版运行库路径（与 `AM_RT_ORT_LIBRARY` 分线） | unset → 目录探测 | [SUPPORT](SUPPORT.md) |
| `AM_RT_TRT_CACHE_DIR` | TensorRT engine 缓存目录（编译产物复用） | unset → 引擎缓存禁用/临时目录（以代码为准） | [SUPPORT](SUPPORT.md) |
| `AM_RT_VISION_BACKEND` | 示例工具 `inspect` 选用后端 | unset → 示例默认 | examples |
| `AM_SDK_OPENVINO_DIR` | OpenVINO 原生引擎（E1）运行库目录；进程级环境变量，作为 `runtime_dirs` 合同通道的显式覆盖（v0.3.2 起文档化并接线 runtime_dirs） | unset → 仅 `runtime_dirs` 通道 | [INTEGRATION](../sdk/INTEGRATION.md)、[dual-dll-abi](design/dual-dll-abi.md) |
| `AM_SDK_CUDA_DIR` | `cudart64_*.dll` 显式目录（GPU 互操作，优先于 ORT 目录） | unset → ORT 运行库目录候选 | [SUPPORT](SUPPORT.md) |
| `AM_SDK_ORT_DIR` | CUDA 互操作运行库候选目录（次序在 `AM_SDK_CUDA_DIR` 后） | unset → 目录探测 | [SUPPORT](SUPPORT.md) |

## Test-fixture variables / 测试夹具变量（仅测试与示例读取，不影响产品行为）

共性：指向 `.local/tests/` 下合成夹具（样本媒体/模型/输入目录）或本地回环 RTSP 凭据；未设置时对应
`#[ignore]` 测试跳过，不构成产品行为。清单（新增夹具变量同样须过表）：

- 媒体样本：`AM_SDK_H264_SAMPLE`、`AM_SDK_H264_ALTERNATE`、`AM_SDK_H264_P_SAMPLE`、`AM_SDK_HEVC_SAMPLE`、`AM_SDK_HEVC_ALTERNATE`、`AM_SDK_HEVC_REXT_SAMPLE`、`AM_SDK_CODEC_SAMPLES`、`AM_SDK_RECONFIG_INPUTS`、`AM_SDK_JPEG_SAMPLE`、`AM_SDK_JPEG_ALTERNATE`、`AM_SDK_JPEG_BENCH`、`AM_SDK_IO_BOUNDARY_DIR`、`AM_SDK_BATCH_SAMPLE_DIR`、`AM_SDK_SAMPLE_MIXED_ONLY`、`AM_SDK_SAMPLE_REVERSE`
- 模型夹具：`AM_SDK_MODEL`、`AM_SDK_MODEL_DIR`、`AM_SDK_RAW_MODEL`、`AM_SDK_BAD_RUN_MODEL`、`AM_SDK_MATMUL_MODEL`
- 运行库夹具：`AM_SDK_FFMPEG_DIR`（选型测试显式 FFmpeg）、`AM_SDK_ORT_LIBRARY`（batch/selection 测试 ORT）
- 本地回环源：`AM_SDK_LOCAL_RTSP_URL`、`AM_SDK_LOCAL_RTSP_USER`、`AM_SDK_LOCAL_RTSP_PASSWORD`（仅本地回环端口，不连现场设备）

> Registration rule: to add or change a variable, update this table in the same change.
> Sources verified by grep over `crates/` at v0.3.3 batch (2026-11-03); runtime-vs-fixture
> classification follows whether product code (outside `#[cfg(test)]` / examples) reads it.
