# SDK 专用 FFmpeg 运行库构建

此构建用于双 SDK 的独立分发候选；不覆盖主项目 `vendor/ffmpeg`，也不改变 SDK 接口。源代码与原运行库保持同一 FFmpeg 提交 `303510f9d1`，但去除与首版无关的第三方静态库，从而能够按实际产物提供完整材料。

## 重建

在 Windows x64 上准备 Python 3.12+、MSYS2 MinGW64 GCC、binutils、基础 MSYS shell 工具和 zstd。脚本只向显式输出目录下载、校验并展开 NASM 2.16.03、GNU Make 4.4.1、GNU diffutils 3.12；不会安装系统软件或修改 PATH。实际构建保留 x86 汇编优化。

```powershell
python scripts/sdk/build-ffmpeg-runtime.py `
  --source-archive D:/FrameBraid-materials/ffmpeg-303510f9d1.tar.gz `
  --output-root D:/FrameBraid-ffmpeg-build `
  --msys-root C:/msys64 --jobs 6
```

源码归档 SHA-256 固定为 `5a1063443078be230709462c289f10b6695dfc88e7fb7009d5a1c58885562f2a`。归档下载源与原始提交由第三方材料清单登记；脚本不接受同名但不同字节的源码。相同配置的专属构建目录可显式加 `--resume` 继续编译。脚本给出可重复的构建配方，不承诺跨工具链版本、路径与时间的逐字节确定性。

输出包括五个 DLL、实际版本与配置、PE 直接导入列表、源码和工具摘要、构建日志、`config.h`、LGPL 2.1 原文及 GCC Runtime Library Exception / MinGW 运行库许可文本。FFmpeg 使用 Windows 线程；GCC/libatomic 所需的 winpthreads 通过末尾 `EXTRALIBS=-Wl,-Bstatic -lwinpthread -Wl,-Bdynamic` 静态链接，并附其 MIT 类原文。该 make 变量与 configure 参数分别记录，不能仅凭 FFmpeg 的 configuration 字符串推断最终链接内容。若意外引入 libgcc/libwinpthread 动态依赖则拒绝生成完成记录。

## 能力边界

- 解码器及解析器：H.264、HEVC/H.265、MJPEG。
- 输入：RTSP、RTP/SDP、Annex B、MP4/MOV、Matroska、MPEG-TS、MJPEG；网络 TCP/UDP/RTP/HTTP/HTTPS（Windows Schannel）。
- 格式转换：swscale、swresample；H.264/HEVC MP4 到 Annex B 等必要 bitstream filters。
- Windows 原生 D3D11VA/DXVA2 编译支持。编译开启不代表已在所有显卡通过硬件解码验收；软件解码仍需以 SDK 真实回归为准。
- 禁用自动外部库探测、GPL、nonfree、version3，不提供 FFmpeg 命令行程序、额外编码器或其他模型/编解码库。

不能把这个精简包当成原大包的等价通用 FFmpeg 发行版。是否作为 SDK 默认分发必须先通过 SDK 的 H.264/H.265/MJPEG、网络输入、像素与 ABI 验证；主项目自带运行库不随之替换。最终分发仍需同时携带受控材料清单、对应源码归档和许可证，不能只复制五个 DLL。
