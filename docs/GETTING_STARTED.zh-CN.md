# 快速接入

[English](GETTING_STARTED.md) · [文档入口](README.md)

适用于 Windows x64 源码仓库或 [v0.1.0 下载包](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0)。已有具名本地 Release 候选验证，见[候选身份及结果](RELEASE_STATUS.md)。下面命令构建自己的制品，不把已有验证结论自动套到新构建。

## 1. 构建

准备 Rust 1.90+、MSVC x64 工具及 Windows SDK，在仓库根目录执行：

```powershell
$env:CARGO_TARGET_DIR = (Join-Path $PWD '.local/build/current')
cargo build --release --locked -p am-camera-ffi -p am-vision-ffi --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames
$SDK_ROOT = Join-Path (Get-Location).Path 'sdk'
$DLL_DIR = Join-Path $env:CARGO_TARGET_DIR 'release'
```

DLL/导入库位于 `$DLL_DIR`，Cargo 首次须取得锁定依赖。构建不连接相机。

## 2. 提供原生依赖

- Media 使用兼容 FFmpeg 动态库，已审 Windows ABI 要求 `avcodec-63`、`avformat-63`、`avutil-61`、`swresample-7`、`swscale-10`。
- 真实推理需要兼容 ONNX Runtime，CPU 实测为 1.24.1，准确来源见第三方档案，同版本其他二进制不自动算已验证。
- 调用方、SDK 及依赖均为 x64，可能需要兼容 Microsoft Visual C++ 运行库。
- SDK DLL 与 FFmpeg/ORT 分别查找，初始化传绝对运行库目录，不复制系统 DLL、不依赖开发机全局 PATH。
- 仓库不附运行库和权重，见[第三方材料](../sdk/third-party/README.md)。

调用方自行准备的路径示例：

```powershell
$ORT_DIR = 'C:/FrameBraid-runtime/onnxruntime-cpu'
$FFMPEG_DIR = 'C:/FrameBraid-runtime/ffmpeg'
```

编入并显式启用共享帧后，Vision 仅共享帧初始化可以不传 ORT，但不能加载模型/推理；提供运行库目录则严格初始化。

## 3. 用自己的图片运行 Pose

使用有权使用且匹配[模型档案](../crates/vision/resources/profiles.json) `pose-yolo26-e2e-coco17-v1` 的权重。核对导出版本、形状与前处理，不只看文件名。

在自己的应用目录保存 `run_pose.py`：

```python
import sys
from pathlib import Path

sdk, dlls, ort, weights, image = map(Path, sys.argv[1:])
sys.path.insert(0, str(sdk / "bindings/python"))
from safe_sdk import VisionContext

with VisionContext(dlls / "am_vision.dll", [ort]) as vision:
    with vision.load_model(weights, "pose-yolo26-e2e-coco17-v1") as model:
        request_id, result = model.infer_encoded(image.read_bytes())
        with result:
            try:
                print("Result task:", result.view.task)
                # result 存活期间读取 result.view.pose；长期保存先复制。
            finally:
                vision.ack(request_id)
```

替换应用、模型和图片路径：

```powershell
python 'C:/MyApp/run_pose.py' "$SDK_ROOT" "$DLL_DIR" "$ORT_DIR" 'C:/Models/pose.onnx' 'C:/Images/input.jpg'
```

封装只需标准库 `ctypes`，不需要 Python onnxruntime 包；保留完整 bindings 目录。PNG 使用 `png=True`。异步 `model.infer_encoded(data, asynchronous=True)` 后依次 `vision.wait(id)`、`fetch(id)`、`ack(id)`，每个取得结果仍须释放。超时不等于取消，保留 ID 继续查询或显式取消。

现有 `inference.py/.c/.cpp`、`Inference.cs` 使用精确 `1×3×2×2` FLOAT Identity 模型，不是任意 YOLO。测试模型生成器需要 Python `onnx`，SDK 运行本身不需要。

## 4. 尝试一次相机操作

仅访问有权限的设备，通过宿主安全注入地址和凭据。示例读取 `AM_SDK_SOURCE_URL`、`AM_SDK_USERNAME`、`AM_SDK_PASSWORD`，不在 URL、代码或报告中写密码。

```powershell
python "$SDK_ROOT/examples/camera.py" --dll-dir "$DLL_DIR" --runtime-dir "$FFMPEG_DIR"
# 显式选择 RTSP 解码帧：
python "$SDK_ROOT/examples/camera.py" --dll-dir "$DLL_DIR" --runtime-dir "$FFMPEG_DIR" --rtsp
```

分别取得一张 ISAPI JPEG 或一帧 RTSP 解码图，不是连续播放器。连续取图、编码订阅及直接 AU 解码走底层接口。

## 5. 其他语言及生命周期

C/C++、C# 命令与完整退出顺序见[接入说明](../sdk/INTEGRATION.md)和[示例](../sdk/examples/README.md)。

SDK 所有结果/帧用 SDK 释放，请求确认不代替结果释放。先关子资源再关上下文，超时不授权销毁活句柄或带在途回调卸载 DLL。

## 排错

| 现象 | 检查 |
| --- | --- |
| 初始化前 DLL 加载失败 | 绝对路径、x64 位数、传递依赖、原始系统错误 |
| Windows 错误 126 | 有依赖未解析，仅凭此码不确定具体缺哪个 |
| 运行库初始化失败 | 显式目录、ABI/版本、provider 依赖，保留结构化错误 |
| 模型被拒绝 | 档案、导出参数、dtype、全部输入输出形状 |
| 超时或队列拒绝 | 保留 ID、查预算/状态、确认已完成请求并释放结果 |
| RTSP 没有可用帧 | 编码/能力、关键帧、重连代次及新鲜度 |

不要随意复制其他电脑的 DLL，也不要缺依赖时静默切换后端。
