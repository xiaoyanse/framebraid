# Runtime licensing guide / 运行库许可选择指引

> 适用对象：Windows x64 运行包与自备运行库的 FrameBraid 接入方。回答一个问题——什么时候需要 GPL（libx264）构建的 FFmpeg，什么时候 LGPL minimal 档案就够了，各自分发要带什么。
> This page summarizes selection boundaries and distribution obligations; it is not legal advice. The license texts shipped under `licenses/` and each component's own license remain authoritative.

## Why this page / 为什么有这一页

v0.3.0 起 SDK 增加编码推流（S5），H.264 编码使用 libx264，随包 FFmpeg 运行库因此切换为 GPL 构建（2026-10-29 决策接受 GPL，x264 入材料闭包，提交 `1f8b8bf`）。只做解码、取图、订阅与推理的接入方并不需要接受 GPL——这条边界此前要靠逐层考古才能确认（接入方反馈 P1-7 附带），本页把它一次写清。

Since v0.3.0 the shipped FFmpeg runtime is a GPL build because S5 encoding uses libx264. Integrators that only decode, capture, subscribe or run inference do not need GPL components; this page draws that boundary once.

## The two FFmpeg builds / 两种 FFmpeg 构建

| | LGPL minimal 档案 | GPL 编码构建（v0.3.0 起随包） |
| --- | --- | --- |
| 构建配置 | 受控源码精简构建，`--disable-gpl`（档案 `minimal-20261026`） | `--enable-gpl --enable-libx264` |
| FFmpeg 许可 | LGPL-2.1-or-later | GPL-2.0-or-later（启用 GPL 部分后整体按 GPL） |
| DLL | avcodec-63 / avformat-63 / avutil-61 / swscale-10 / swresample-7 | 同名五个 DLL + `libx264-165.dll` |
| 解码、RTSP/ISAPI 取图、订阅、推理 | 可用（这些路径不调用编码器；vision 侧 onnxruntime.dll 为 MIT，与 FFmpeg 构建无关） | 可用 |
| S5 编码/存档/推流（`amc_encoder_*` / `amc_archive_*` / `amc_stream_*`） | 不可用：create/open 返回 `sdk.notSupported`（运行库缺 libx264 编码器），不静默降级 | 可用 |

判定口诀：**用到编码三入口才需要 GPL 构建；只消费视频与推理入口，LGPL minimal 档案即可，不引入 GPL。** 没有单独的编码能力位，运行库是否含 libx264 以实际 create/open 调用结果为准。

Rule of thumb: only the three encode entry points require the GPL build; decode, capture, subscription and all inference entry points run on the LGPL minimal profile and pull in no GPL code. There is no separate encode capability bit — the actual create/open result tells you whether the runtime carries libx264.

## What the package ships / 分发包里是什么

v0.3.0 起 Windows 运行包物理携带 GPL 构建闭包（材料锁 `sdk/third-party/materials-lock.json` 登记为 `ffmpeg-encode-gpl-build`：五 DLL + `libx264-165.dll`），即"开箱即用含编码"；FFmpeg 与 x264 许可文本随包分发，打包管线在生成时校验运行库实际许可与锁定许可文本一致，材料不齐拒绝正式分发。这同时意味着**包内 FFmpeg 二进制按 GPL 分发**。若你的产品分发不能接受 GPL：把 `runtime/ffmpeg` 替换为 LGPL minimal 档案的同名 DLL（动态库接口兼容，解码路径行为不变；S5 明确 notSupported），并按 LGPL 义务配齐该构建自己的源码/配置/声明材料。两个档案对应不同二进制，材料、哈希与许可状态不可互相套用。

If your distribution cannot accept GPL, substitute the LGPL minimal build's same-name DLLs (decode paths unchanged; S5 fails explicitly) and carry that build's own matching source/configuration/notices. Never mix the material status or license version of one profile onto the other build's binaries.

## Distribution obligations / 分发义务要点

- **LGPL-2.1-or-later（minimal 档案）**：动态链接不要求宿主开源。随二进制附 LGPL 许可文本与声明；向接收者提供该库二进制精确对应的源码与构建配置（minimal 档案为受控源码构建，匹配源码/构建材料随包 `licenses/` 提供）；保留最终用户替换、反向工程 DLL 的能力，不加技术或合同限制；修改过须声明。
- **GPL-2.0-or-later（编码构建：FFmpeg + libx264）**：分发这些二进制即触发 GPL——随附 GPL 许可文本与 x264 许可文本；向接收者提供与二进制精确对应的源码（FFmpeg 源码、构建配置与 x264 源码；构建档案在仓内 `.local` 留有匹配来源，分发时按 GPL 要求随附或另行提供）；与之结合形成的作品须可按 GPL 授权分发。注意许可组合：FrameBraid SDK 自有代码为 Apache-2.0，与 GPL-2.0-only 不兼容、与 GPL-3.0 单向兼容；FFmpeg/x264 的 GPL 部分为 "GPL-2.0-or-later"，分发方可选择按 GPL-3.0-or-later 履行组合分发，或让 GPL 运行库与自有代码保持独立分发。组合边界与义务解释请自行做法律复核，本页不构成法律意见。
- **ONNX Runtime（MIT）**：保留版权与许可声明及 ThirdPartyNotices；vision 侧运行库与 FFmpeg 构建选择无关。
- **VC 运行库**：不随包分发系统 DLL；需要时使用微软官方 Redistributable 安装器并遵守其条款，不复制 System32。

## Decision record / 决策与登记

- 2026-10-29：编码运行库 GPL 化（x264 入闭包）经决策接受，x264 许可文本随包分发（提交 `1f8b8bf`，材料锁 scope 同步改写）。
- LGPL minimal 档案（`minimal-20261026`）保留为"无编码"场景的自备选项；其材料状态与许可版本只对该档案的二进制成立，不套用到 GPL 构建，反之亦然。

相关入口：[第三方材料](../sdk/third-party/README.md) · [接入说明](../sdk/INTEGRATION.md) · [交付状态](RELEASE_STATUS.md)
