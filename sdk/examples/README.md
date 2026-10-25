# 外部语言示例

当前 FrameBraid 源码的完整接入见 [INTEGRATION.md](../INTEGRATION.md)，英文用户先读 [Getting started](../../docs/GETTING_STARTED.md)。当前仓库的 `include/bindings/examples` 均在 `sdk/` 下；旧平铺包是另一种布局。尚未公开发布 FrameBraid Windows 包。

本页令 `$SDK_ROOT` 指向含 `include/bindings/examples` 的那层。以下按仓库根目录及已构建的 DLL 演示，运行库目录由调用方准备。SDK 不附权重，inference 示例使用固定 `1×3×2×2` FLOAT Identity 模型，不能直接换成 YOLO；业务模型的真实图像入口见接入说明。

```powershell
$SDK_ROOT = Join-Path (Get-Location).Path 'sdk'
$DLL_DIR = 'D:\FrameBraid-build\release'
$LIB_DIR = $DLL_DIR
$ORT_DIR = 'C:\FrameBraid-runtime\onnxruntime-cpu'
$FFMPEG_DIR = 'C:\FrameBraid-runtime\ffmpeg'
$WORK = 'C:\MyFrameBraidApp'
$MODEL = 'C:\Models\image-identity.onnx'
New-Item -ItemType Directory -Path $WORK -Force | Out-Null
python "$SDK_ROOT/examples/inference.py" --dll-dir "$DLL_DIR" --runtime-dir "$ORT_DIR" --model "$MODEL"
```

若需自行生成 Identity 测试模型，源码包含 `scripts/sdk/generate-tensor-samples.py`，该生成工具需要 onnx Python 包，SDK 正常运行本身不需要安装它。模型名称、形状及输出语义与调用代码必须一致。

## C#

```powershell
$sources = (Get-ChildItem "$SDK_ROOT/bindings/csharp/*.cs").FullName
$csc = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
& $csc /nologo /platform:x64 /warnaserror "/out:$WORK/Inference.exe" $sources "$SDK_ROOT/examples/Inference.cs"
$env:PATH = "$DLL_DIR;" + $env:PATH
& "$WORK/Inference.exe" "$ORT_DIR" "$MODEL"
```

封装可纳入自己的 .NET 项目，无 NuGet 依赖。SafeHandle/using 不代替正常关闭屏障；父资源在子资源之后释放，长期数据须复制，不使用失效指针。

## C / C++17

在 x64 Native Tools 环境中设置同名环境变量，再执行：

```bat
cl /nologo /utf-8 /W4 /WX /I "%SDK_ROOT%\include" "%SDK_ROOT%\examples\inference.c" /Fe:"%WORK%\inference_c.exe" /Fo:"%WORK%\inference_c.obj" /link /LIBPATH:"%LIB_DIR%" am_vision.dll.lib
cl /nologo /utf-8 /std:c++17 /EHsc /W4 /WX /I "%SDK_ROOT%\include" "%SDK_ROOT%\examples\inference.cpp" /Fe:"%WORK%\inference_cpp.exe" /Fo:"%WORK%\inference_cpp.obj" /link /LIBPATH:"%LIB_DIR%" am_camera.dll.lib am_vision.dll.lib
set PATH=%DLL_DIR%;%PATH%
"%WORK%\inference_c.exe" "%ORT_DIR%" "%MODEL%"
"%WORK%\inference_cpp.exe" "%ORT_DIR%" "%MODEL%"
```

C 示例检查返回码；C++ 示例用 RAII 保持所有权和依赖顺序。同一个 C ABI 供各语言调用，不引入编译器专属 C++ ABI。运行库目录通过初始化显式传入，不能装到 System32 或依赖开发机 PATH 隐式补齐。

## 相机与所有权

`camera.py` 默认单次 ISAPI 抓图，`--rtsp` 拉一张真实解码帧。凭据由调用方安全注入 `AM_SDK_SOURCE_URL/AM_SDK_USERNAME/AM_SDK_PASSWORD`，不写进地址或脚本，不由 SDK 持久化。连续抓图/完整 AU 解码/编码包订阅见低层 `amrt_sdk.py`。

推理同步和异步均需处理请求 ID；异步 `wait → fetch → ack` 后，每个外持结果仍须 release。`ack` 释放缓存责任，不替代外持拥有者释放。共享帧须保持 publication/源/帧/代次身份；显式仅共享帧且无运行库目录可不安装 ORT，不能据此执行模型。

## 依赖错误

SDK 尚未加载时由 Python `SdkLoadError`、C# `SdkLoadException` 或调用方原生加载层报告；保留原始异常/系统码，不能调用尚未加载的 SDK。Windows 126 不直接等于 VC++ 缺失。SDK 已加载后的初始化或异步失败用既有结构化错误接口查消息、来源和重试属性。

独立源码另附验证脚本，按 `--help` 显式提供自有样本与输出目录。示例、有限 SDK 测试和真实相机/现场验收分别记录，不用静态编译通过代替实际调用。
