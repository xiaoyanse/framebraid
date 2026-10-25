# Third-party materials / 第三方材料

[Project license](../../LICENSE) · [NOTICE](../../NOTICE) · [Release checklist](../../docs/RELEASE_CHECKLIST.md)

Original FrameBraid code is Apache-2.0. This does not relicense FFmpeg, ONNX Runtime, compiler runtimes, models or other dependencies. Original license texts in this directory must not be reformatted or have their copyright holders replaced.

FrameBraid 自有代码采用 Apache-2.0，不改写 FFmpeg、ONNX Runtime、编译器运行时、模型及其他依赖许可。原始许可文本不格式化、不替换版权人。

## What is in this checkout? / 仓库包含什么？

- Original license texts, the historical [materials lock](materials-lock.json), and the reviewed [minimal runtime profile](profiles/minimal-20261026.json).
- No runtime DLLs, FFmpeg source archive, controlled build-materials archive or model weights.
- ONNX Runtime's matching ThirdPartyNotices is an external material identified by archive member and SHA-256 in the profile; it is **not currently a local file** in this checkout.

包含许可原文、历史锁及受控档案，不附运行库、FFmpeg 源码/构建材料大包或模型。ORT ThirdPartyNotices 的归档成员和指纹已登记，但原文当前在仓库外，不能声称此目录已自足分发所有运行库材料。

The Windows release package physically includes the matching FFmpeg source/build archives and ORT ThirdPartyNotices under `licenses/`. The source-only checkout remains separate; see [current candidate status](../../docs/RELEASE_STATUS.md).

Windows 发布包在 licenses/ 附齐上述匹配材料；源码仓库的内容边界保持独立。

## Reviewed runtime profiles / 已审运行库档案

| Profile | Purpose and limit / 用途与限制 |
| --- | --- |
| `minimal-20261026` | Controlled FFmpeg source build, LGPL-2.1-or-later, no external media libraries; exact sources/configuration/toolchain notices still accompany binary distribution / 受控精简构建，二进制仍须配匹配源码、配置和工具链声明 |
| Historical default `materials-lock.json` | Old BtbN FFmpeg LGPL-3.0-or-later build; static-dependency source/notice closure incomplete, not for formal distribution / 旧 BtbN 构建材料闭包未齐，不用于正式分发 |

The profiles describe different binaries. Never apply the minimal profile's material status or LGPL version to old BtbN DLLs.

两个档案对应不同二进制，不把精简档案许可与材料状态套到旧 DLL。

## ONNX Runtime / 推理运行库

The recorded CPU DLL comes from `onnxruntime-openvino==1.24.1`, cp312/cp312/win_amd64, with fixed wheel/member hashes. The SDK validation uses the CPU core only; this is not an OpenVINO/CUDA provider bundle. Other wheel builds or provider packages need separate matching materials and validation. The original MIT LICENSE is in [onnxruntime/LICENSE](onnxruntime/LICENSE).

已验 CPU DLL 来源与哈希固定，仅使用 CPU 核心，不表示附带 OpenVINO/CUDA provider。其他 wheel 或 provider 单独核材料及功能。

## Acquiring materials / 取得材料

Maintainer tools live under `scripts/sdk/` at the repository root. The downloader accepts an absolute output directory outside the checkout, then an optional profile name:

```text
node scripts/sdk/fetch-third-party-materials.mjs <absolute-materials-directory> minimal-20261026
```

This command downloads only locked HTTPS artifacts and verifies size/hash. The minimal profile also requires the exact reviewed `ffmpeg-sdk-minimal-20261026-build-materials.zip`, available as a separate v0.1.0 release asset and inside the Windows package. A newly rebuilt ZIP with different bytes does not satisfy the old lock. The tool intentionally fails rather than inventing or silently accepting it. Network availability is separate from hash validation.

下载器仅取锁定 HTTPS 工件并校验。精简档案的已审构建材料 ZIP 在 v0.1.0 Release 单独提供，也包含在 Windows 包内；不能把不同字节的新 ZIP 冒充原档案。未具备会明确失败，不静默跳过。

For new runtimes, capture their own source, build configuration, dependency notices and provenance, then review/update the material profile deliberately. A generic link to ffmpeg.org or a hash without actual source availability does not provide corresponding source to recipients.

新构建须登记自身源码、配置、依赖许可和来源，再审档案，不以泛指上游网址或只有哈希代替向接收者提供对应源码。

## Distribution gates / 分发检查

Resolved Windows Cargo dependency originals and the reproducible inventory are
now in [rust/](rust/README.md), separate from native runtime profiles.

当前 Windows Cargo 依赖的原始声明与清单见 rust/，与原生运行库档案分开。

- Keep SDK Apache-2.0, FFmpeg LGPL, ORT MIT and other component requirements distinct.
- Review all Rust/native dependencies for the actual packaged build, not just the two major runtimes.
- Include matching runtime manifests, originals, source/build materials and dependency notices when distributing binaries.
- Allow replacement of compatible FFmpeg dynamic libraries and preserve LGPL rights; do not add restrictions that defeat them.
- Do not redistribute arbitrary Windows system DLLs. Use the official Microsoft redistributable when required, under its terms.
- No legal approval, source publication or binary publication is asserted by this document.

SDK 与各依赖许可分别履行，按实际构建审全部依赖，二进制附准确材料。保留 LGPL 权利，不随意分发系统 DLL。本说明不代替发布批准。
