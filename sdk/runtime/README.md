# SDK 运行库与交付材料

> 此文保留原宿主的运行库与打包工具说明，路径/制品不代表已存在的 FrameBraid 发布包。当前仓库不附运行库，公开前要求见 [发布清单](../../docs/RELEASE_CHECKLIST.md)；独立构建无需 export-source。下方历史命令不在本次文档准备中执行。

运行库由调用方显式提供。SDK 不下载依赖、不修改系统 PATH。首版运行验收为 Windows x64 CPU 基础包；其他平台、加速后端及长期运行分别留证。

## 目录与加载

| 目录 | 内容 | 用途 |
| --- | --- | --- |
| `bin/win-x64` | 两个 SDK DLL | 调用方按绝对路径加载 |
| `lib/win-x64` | 两个 `.dll.lib` | C/C++ 链接使用 |
| `runtime/ffmpeg` | avcodec-63、avformat-63、avutil-61、swresample-7、swscale-10 | camera 的 `runtime_dirs_utf8` |
| `runtime/onnxruntime-cpu` | 固定版本的 `onnxruntime.dll` | vision 的 `runtime_dirs_utf8` |
| `licenses` | 原始许可证、第三方声明、源码及构建材料 | 与运行库逐文件匹配 |

五个 FFmpeg DLL 是组件数量，并非 FFmpeg 5.x。两套运行库目录不能合并，不依赖开发机全局 PATH 兜底。OpenVINO/CUDA provider、插件及驱动不包含在 CPU 包中。

## 依赖不可用时的提示

- SDK 本身未加载：由调用方装载层报告。Python `SdkLoadError`、C# `SdkLoadException` 保留原始异常、路径和可用系统码，区分文件不存在、位数/PE 格式不匹配、传递依赖与导出不匹配；不把 Windows 126 一律认定为缺 VC++。C/C++ 调用方同样须检查系统装载结果，不能调用尚未加载的 SDK。
- SDK 已加载：通过既有初始化错误或 `get_last_error` 读取码、详情、来源、系统码。相机 FFmpeg 失败保持冻结 ABI 的错误码，详情区分缺失组件、损坏/错误位数、符号/版本、访问权限和其他装载失败；解码器与后台 RTSP 源均留存脱敏详情，不泄漏凭据或设备路径。
- ORT 缺失为 `vision.runtimeMissing`，装载/API/版本/依赖不兼容为 `vision.runtimeLoadFailed`；同进程路径冲突明确拒绝，恢复仍按显式 retry 和无登记 ctx 的屏障。显式仅共享帧且未提供运行库目录时不要求安装 ORT；不能据此执行模型。

Windows 需要兼容的 Microsoft Visual C++ 2015–2022 Redistributable x64。使用 [Microsoft 官方安装入口](https://aka.ms/vs/17/release/vc_redist.x64.exe)，安装时核实签名；不复制 System32 DLL，不强制已有兼容运行库的机器重装。SDK-G3 不要求卸载 VC++ 或禁用显卡，重点是依赖齐全可独立运行、缺失或不兼容时明确报错、不崩溃和假成功。

## 受控材料档案

| 档案 | 运行库与材料 | 当前边界 |
| --- | --- | --- |
| 默认 `materials-lock.json` | 原 BtbN FFmpeg LGPL-3-or-later 构建，含大量静态扩展库 | 其扩展库源码/声明闭包未核实，严格分发闸门继续拒绝 |
| `minimal-20261026` | 同一 FFmpeg `303510f9d1` 源码，SDK 专用子集，LGPL-2.1-or-later | H.264/H.265/MJPEG、RTSP、缩放和像素转换已实际验证；材料闭合，主项目 vendor 未替换 |

精简档案禁用外部媒体库及自动探测，保留 x86 汇编、Windows 原生网络与原生硬解接口编译能力；首版实际解码仍为软件，不据此宣称硬解已验。五份 DLL 合计 8,623,616 字节。保留源码原包、最终配置、`run.sh`、`makeVariables`、构建脚本及 GCC/MinGW/winpthreads 原始许可。详见 [构建方法](ffmpeg-build/README.md) 和 [第三方材料](../third-party/README.md)。

ORT 固定为 OpenVINO 1.24.1 **cp312/cp312/win_amd64** 原 wheel 中的核心 DLL，逐字节匹配；只分发 CPU 核心。其 LICENSE 和 ThirdPartyNotices.txt 从同一个原始 wheel 提取，不能用同版本其他 Python ABI 的 wheel 替代。

## 生成与核验

```powershell
node scripts/sdk/fetch-third-party-materials.mjs D:/FrameBraid-materials minimal-20261026
node scripts/sdk/package-delivery.mjs `
  --output D:/FrameBraid-packages/candidate `
  --dll-dir D:/FrameBraid-build/release `
  --ffmpeg-dir D:/FrameBraid-runtime/ffmpeg `
  --ort-dir D:/FrameBraid-runtime/onnxruntime-cpu `
  --materials-dir D:/FrameBraid-materials `
  --materials-profile minimal-20261026 --require-distribution-ready
```

本地构建材料 ZIP 的受控来源为已核验的保存工件，下载器不会伪造远程 URL；重新构建产生不同字节时必须重新核验和登记档案。输出必须为仓库外新目录或空目录，拒绝链接和覆盖。只接受源码受控档案，不接受调用方自述任意 JSON。

打包前逐材料/原始工件/运行库核 SHA 和大小，读取实际版本、许可、配置和 PE 导入；缺口存在时在创建输出目录前拒绝。原 BtbN 档案的缺口不会因新档案完成而被清零。`runtime-manifest.json` 记录来源提交、脏状态及逐文件清单；源码闭包另由 `export-source.mjs` 导出，不用运行包内的说明冒充源码。

`materials-present-review-required` 和空缺口表示本次匹配材料已在场，`distributionApproved=false` 保持不变：工具不代替负责人批准发布。打包本身只核完整性，不自动执行 SDK-G0～G3。运行与材料验收见交付包 `docs/verification/verification-dual-sdk-closeout-20261026.md`；未上传、推送、打标签或正式外发。

准备 Release 包时另传 `--build-record <本轮成功构建的JSON>`；打包器核两份 DLL 的大小/SHA，配置不从目录名猜测。该记录与编译输入对应关系、实际运行测试另见收据；成功打包仍不批准发布。开发者部署以接入说明中的解压路径为准，示例打包目录不是运行依赖。
