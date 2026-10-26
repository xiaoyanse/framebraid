# Release status / 交付状态

[Documentation / 文档入口](README.md) · [Release checklist / 发布清单](RELEASE_CHECKLIST.md)

Updated 2026-10-26. Version 0.1.0 developer preview; Windows x64 CPU.

更新于 2026-10-26，0.1.0 开发者预览版，首发验证 Windows x64 CPU。

Local directory consolidation and host source pinning are recorded in [verification](verification/local-consolidation-20261026.md). This maintenance work does not rebuild or replace public release assets.

本地目录与宿主版本维护见上述记录；本轮没有替换公开版本。

Local R1 measurement tools and CPU/synthetic baselines are complete; see [R1 verification](verification/sdk-r1-complete-20261026.md). This is not R2–R5, hardware, real-model or production acceptance and does not replace public artifacts.

本地R1观测与合成基线已完成，原生硬件能力、其他优化包及长跑仍未完成；未发布、不升级支持范围。

R5 priority queue retirement is locally verified ([record](verification/sdk-r5-priority-20261026.md)); hardware selection, adaptive batching and production validation remain open. No public assets were replaced.

R5第1组排队清理本地验证完成，保留微基准告警与限制；不等于整包R5或硬件能力完成。

Local R2/H2 explicit Rust hardware foundation is verified on synthetic H.264/HEVC Main ([record](verification/sdk-r2-hardware-foundation-20261026.md)); C ABI/RTSP automatic selection, global readback budgeting and production validation remain open. Published assets unchanged.

R2/H2.0已实测原生设备与显式硬解基础路径；自动选择/回退、跨语言诊断及H5推理选择尚未完成，不扩大公开CPU支持范围。

R2/H2.0b adds a locally verified Rust initialization selector ([record](verification/sdk-r2-hardware-selection-20261026.md)); DLL/RTSP integration, runtime recovery and global budgets remain incomplete. / Rust共用初始化选择与CPU回退已验，整包硬件能力仍未完成。

R2 H2.0c/1.1 readback reservations and conversion caches are locally verified ([record](verification/sdk-r2-readback-cache-20261026.md)); both pixel-copy entries use the budget adapter, but automatic native hardware activation/recovery and production/soak validation remain open. / 回读额度覆盖缓存寿命、转换缓存已验证，不改变公开CPU制品和未验事项。

## Public version / 公开版本

[Repository](https://github.com/xiaoyanse/framebraid) · [v0.1.0 downloads](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0)

The public source is a clean initial snapshot; prior local history is retained
privately. Release asset manifests bind their exact source commit, toolchain,
features and checksums. Use the release SHA256SUMS, not the historical candidate
hashes below, to verify downloaded files. Original code uses Apache-2.0; third-party
rights and model licensing remain separate.

首次公开采用精简后的初始快照，本地旧历史保留。公开制品以各自清单记录源码提交、
工具链、编译特性和指纹；下载校验使用 Release 中的 SHA256SUMS，下方是历史候选哈希。

## Local unreleased runtime identity fix / 本地未发布版本诊断修复

Runtime version text now comes from Cargo; target and source/build notes reflect compilation
inputs. C ABI 1.0 layouts/exports remain unchanged. This local fix does not replace the public
v0.1.0 assets or broaden runtime/platform acceptance. See the
[scoped verification record](verification/runtime-version-20261026.md).

已完成本地运行时版本诊断修复及定向验证；未发布、不覆盖 v0.1.0 制品，不代表重跑整包验收。
源码包无自身 Git 时明确未知，具体二进制仍按制品校验和识别。

## Historical candidate identity / 历史候选身份

- Source/build commit: `8f7fa3a444c07426a0402f931e3d390f680a5ce7`.
- Version label: `0.1.0-local`; ABI 1.0 r5; Windows x64 Release with both shared-frame features.
- Source ZIP SHA-256: `8d11d7c0b3a5ec221fdfa46b58c58ab64374b28342ce8a1c22b8151adfefda92`.
- Windows ZIP SHA-256: `16914abdf27af4133e5ce03a3d64b9348bd06aa54c90670f8cd34271931ad98a`.
- Media DLL SHA-256: `0c8442058aac35554b502d0d3bf4dc331b876f0c4f2604ba6ddc05d4d143f128`.
- Vision DLL SHA-256: `97c6705adf060a3808d3d05f0f4a5b9e2559dd4cf415b91fddcfaf396e743a75`.

Later documentation commits do not change this candidate's identity. Runtime tests
used the preceding candidate; final DLL bytes are identical. After correcting
packaged documentation links, final inventories and links were checked again.

后续文档提交不改变该候选身份。运行测试来自前一候选，最终两 DLL 与实测字节完全一致；
修正打包文档链接后，重新核验最终制品清单与链接。

## Verified locally / 本地验证

| Check / 检查 | Result / 结果 |
| --- | --- |
| Release Rust library tests | 177 passed, 0 failed, 3 ignored, 8 filtered |
| Packaged DLL groups | 11 passed: lifecycle, shared frames, native diagnostics, SDK loading diagnostics, capture, probe, decoding, notifications/statistics, language bindings, reload cycles, RTSP transport |
| Language calls | C/C++17 with `/W4 /WX`, C# and Python; CPU inference, ownership and shared frames |
| Media fixtures | Local Digest HTTP and synthetic H.264/H.265/MJPEG RTSP; no customer camera |
| Reload observation | 30 rounds, 5 warm-up; handle-count range 0; post-warm-up private-memory net change −241,664 bytes |
| Final Windows contents | 352 inventoried files, size/hash checked; 169 relative documentation/image links checked |
| Packaging regressions | 4 passed, including broken-link detection |
| Dependency materials | Matching FFmpeg source/build archives, native notices and originals for 148 resolved Cargo packages included |

Environment: Windows x64 10.0.26300.0, Intel Core Ultra 9 285H, Rust 1.97.1 MSVC,
controlled minimal FFmpeg and ONNX Runtime CPU 1.24.1. Builds reused cache;
duration is not a clean-build benchmark. Public summaries and candidate fingerprints
are retained. The original detailed private log directory is no longer at its recorded
location after cleanup; recovery of those raw logs is unconfirmed.

限定环境实测，不据此承诺固定 FPS、长期无泄漏或现场验收。
当前保留摘要及制品指纹；原详细私有日志目录清理后不在原位置，原始日志恢复尚未确认。

## Limits and remaining gates / 限制与待办

- Five-family postprocessing has unit coverage; all five frozen real-weight models
  were **not rerun end-to-end in this candidate pass**. CPU inference used a generated
  test graph. Accelerator, hardware-decoder, other OS and 24-hour/field acceptance
  remain separate.
- Source ZIP excludes runtime binaries/large external archives. The Windows candidate
  includes exact runtime materials. See [third-party materials](../sdk/third-party/README.md).
- Current history was screened for private keys, tokens and private login email;
  synthetic localhost credentials are intentional fixtures.
- Superseded preparation/extraction reports and logo exploration assets are archived
  internally, with byte-verified originals. Public release guidance describes the
  current candidate. Generator inputs, ABI compatibility history, extraction
  fingerprints and original licenses remain intact.
- Owner authorized public source and binary distribution on 2026-10-26. FrameBraid
  is the public SDK maintenance source; host dependency migration remains separate.
- GitHub private vulnerability reporting is enabled. First delivery is a developer
  preview; future publication still requires authorization.

五类真实权重、加速器、硬解、跨平台及长期/现场验收不混同本轮结果。
过程资料已归档，公开入口已精简；本次已获发布授权，宿主迁移及未来版本发布另行安排。
