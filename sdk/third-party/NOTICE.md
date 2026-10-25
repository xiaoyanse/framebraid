# 第三方软件声明

FrameBraid 双 SDK 使用 FFmpeg 视频库和 Microsoft ONNX Runtime。许可原文保留于本目录，ORT ThirdPartyNotices 等外部材料的归档成员和指纹登记在材料档案中，不代表原文已包含于此源码仓库。具体二进制、版本、来源与材料以所选档案及未来运行包 runtime-manifest.json 为准，见 [材料说明](README.md)。

## FFmpeg

FFmpeg 由 SDK 在运行时动态加载。实际选用的运行库及其许可版本以匹配档案和 DLL 自述为准；旧 BtbN 运行库为 LGPL-3.0-or-later，不能套用 LGPL-2.1 登记。

使用者可以在接口、架构兼容的条件下替换为自行修改的 FFmpeg 动态库。对于调试这类修改所需的反向工程，不因 SDK 的其他使用约定而施加限制。LGPL 及其引用的 GPL 文本、对应源码和重建材料随所选材料档案提供；这些权利不代表其他独立 SDK 源码也适用 LGPL。

源码、配方和第三方静态依赖范围必须与具体运行库匹配；原 BtbN 档案的完整静态依赖材料尚未闭合时，不作正式外发使用。

## ONNX Runtime

Copyright (c) Microsoft Corporation。按随附 MIT LICENSE 使用；第三方版权和条款按档案从指定 wheel 的原始 onnxruntime/ThirdPartyNotices.txt 取得，二进制分发时须附上。当前 CPU 核心来源于 `onnxruntime-openvino==1.24.1` 的指定 Windows cp312 wheel，不因此包含 OpenVINO provider 或 CUDA 运行时。

## 编译器运行时和系统前置

受控重建运行库若使用 MinGW/GCC 运行时代码，随对应档案保留 MinGW 许可及 GCC Runtime Library Exception 等原始声明。Windows 系统 DLL 不从用户机器复制分发；需要 VC++ 运行库时按 Microsoft 官方安装程序及其条款处理。
