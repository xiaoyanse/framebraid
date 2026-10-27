# FrameBraid Windows candidate / Windows 候选说明

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
