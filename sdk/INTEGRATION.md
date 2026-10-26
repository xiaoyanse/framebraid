# FrameBraid 双 SDK 接入说明

原生硬件探测与直接解码自动策略见[硬件接入](../docs/design/native-hardware.md)。原Rust CPU构造和1.0 C ABI布局未改变；本地新构建的直接C解码器已执行既有PREFER_HW策略（0），SOFTWARE_ONLY为1，旧公开CPU制品不会自动升级。RTSP和完整设备诊断仍在H2实施中。

Rust调用方可用`CpuDecoder::create_with_policy`获得初始化选择与拥有的原因报告，仍须在送包/收帧后核实实际后端。直接DLL在创建/reset时选择硬件，初始化选中但未出帧时get_info后端NULL；真实出帧后返回ffmpeg-d3d11va/dxva2/cpu。硬件运行失败先过CPU预算，递增代次并等待关键帧；仅首次输出前单启动画面允许保留原输入重试一次，显式reset才重试硬件。见[直接入口验证](../docs/verification/sdk-r2-direct-auto-20261026.md)。初始化1.5秒为原生调用间软预算，不强杀驱动；不保证GPU启动或每帧比CPU更快。

底层Rust回读可选择作用域预算复制或单缓冲复用，复用时许可须持有到清缓存/销毁；输出Vec独立。C ABI像素复制入口由SDK自动管理暂存Charge，调用方无需自行管理；CPU恢复预留可转交外部帧且仍核持帧上限。见[回读与转换缓存记录](../docs/verification/sdk-r2-readback-cache-20261026.md)。

本地未发布R5优化可及时释放未执行且已取消/替换的排队输入；正在读取的输入及含有效项的原生批次仍须等实际结束，不能把cancel当作可立即释放调用方缓冲。见[优先项验证与限制](../docs/verification/sdk-r5-priority-20261026.md)，旧发布包能力不因此改变。

本说明面向使用 SDK 的开发者。交付主体是可独立构建的 Rust 源码 SDK；DLL 目标为 `am_camera.dll` 与 `am_vision.dll`，通过 C ABI 供 C/C++、C#、Python 调用。使用运行包不需要 Rust、FFmpeg 命令行或 AM-RT 业务服务。已发布版本及其真实验证范围见[交付状态](../docs/RELEASE_STATUS.md)；源码构建及运行见 [快速接入](../docs/GETTING_STARTED.zh-CN.md)。

## 1. 先分清两种包

本地ABI 1.1新增`amc_probe_decode_hardware(ctx, codec, budget_ms)`及`amc_get_decode_hardware_report(object, buffer, capacity, required)`。先查ABI再用新增符号；新DLL接受1.0初始化，旧DLL不接受1.1。新Python高层绑定须配新DLL；C#保留核心1.0初始化，新硬件方法检查1.1。旧公开v0.1.0包取对应tag绑定，不把当前main绑定拷进旧包。

Python `CameraContext.probe_decode_hardware()`返回dict，`get_decode_hardware_report()`仅复制缓存；C#对应`ProbeDecodeHardware`/`GetDecodeHardwareReport`返回UTF8 JSON字符串。探测报告selected仅表示初始化，actual_backend为空且stream_verified=false；独立decoder真实输出后再据快照确认。快照支持context和独立decoder、关闭后保留最后结果，当前不接受RTSP source句柄。JSON最低字段约束、预算、线程与所有权见[ABI 1.1合同](../docs/design/dual-dll-abi.md)，实测见[验证](../docs/verification/sdk-r2-abi11-hardware-20261026.md)。

探测是同步耗时操作，宿主应放普通工作线程，不在UI线程或SDK回调内执行；SDK回调内直接拒绝探测重入。预算只在原生调用间检查，不能强杀驱动。纯快照查询不做驱动/会话工作。

| 内容 | 当前独立源码仓库 | 拟交付 Windows x64 运行包 |
| --- | --- | --- |
| 根目录 | `Cargo.toml`、`Cargo.lock`、`crates/` | `bin/`、`lib/`、`runtime/`、`licenses/` |
| 头文件 | `sdk/include/` | `sdk/include/` |
| Python/C# 封装 | `sdk/bindings/` | `sdk/bindings/` |
| 示例 | `sdk/examples/` | `sdk/examples/` |
| 模型档案 | `crates/vision/resources/profiles.json` | `sdk/profiles.json` |
| 用途 | 独立构建、源码集成 | 直接接入已有 DLL |

**当前源码仓库保留 `sdk/`，不要照搬旧平铺源码包的相对路径。** 下文 `SDK_ROOT` 指向包含 `include/bindings/examples` 的那一层，当前仓库为 `仓库根/sdk`。源码不附运行库和模型权重，独立构建后仍须提供相应依赖。若自行构建 DLL，将 `DLL_DIR` 和 `LIB_DIR` 指向 Cargo 的实际输出目录即可，不需要先制作运行包。

先按实际解压位置设置变量。以下目录是部署示例，请替换为自己的路径：

```powershell
$PKG_ROOT = 'C:\AMRT-SDK\windows-x64'
$SDK_ROOT = Join-Path $PKG_ROOT 'sdk'
$DLL_DIR = Join-Path $PKG_ROOT 'bin/win-x64'
$LIB_DIR = Join-Path $PKG_ROOT 'lib/win-x64'
$FFMPEG_DIR = Join-Path $PKG_ROOT 'runtime/ffmpeg'
$ORT_DIR = Join-Path $PKG_ROOT 'runtime/onnxruntime-cpu'
$WORK = 'C:\AMRT-SDK\my-app'
New-Item -ItemType Directory -Path $WORK -Force | Out-Null
```

## 2. 运行环境与首版范围

- Windows x64、x64 调用进程。C# 编译指定 `x64`，Python 使用 64 位版本。
- 需要兼容的 Microsoft Visual C++ 2015–2022 Redistributable x64。已有兼容版本无需重装；缺失时使用 [Microsoft 官方安装入口](https://aka.ms/vs/17/release/vc_redist.x64.exe)，核实安装程序签名。不要复制其他电脑的 System32 DLL。
- 本运行包提供 CPU 推理依赖；CUDA、OpenVINO 的 provider、插件、驱动并不随 CPU 包交付。查询到硬件不代表相应推理后端已可用，不把编译通过视为 GPU 执行验收。
- 相机支持 RTSP H.264/H.265/MJPEG 和 ISAPI 抓图；独立解码器接收完整 H.264/H.265 Annex B AU。任意网络分片、长度前缀包需调用方先装配/转换，不能直接当完整 AU 提交。
- Java、Linux、macOS、Android/iOS 及硬件解码不是本 Windows x64 CPU 包的运行承诺。源码可移植设计与各平台实际交付分别核验。

运行库必须按角色**显式传入绝对目录**，不靠开发机全局 PATH 兜底：

| SDK | 初始化参数 | 运行库 |
| --- | --- | --- |
| camera | `runtime_dirs_utf8 = [FFMPEG_DIR]` | `avcodec-63`、`avformat-63`、`avutil-61`、`swresample-7`、`swscale-10` 对应 DLL |
| vision | `runtime_dirs_utf8 = [ORT_DIR]` | 包内固定版本 `onnxruntime.dll` |

SDK DLL 搜索与上述运行库目录是两件事。Python 用 SDK DLL 绝对路径；C# 的 P/Invoke 将 SDK DLL 放到程序目录或仅为当前进程设置 DLL 搜索 PATH；C/C++ 可将两 DLL 放在 EXE 旁。FFmpeg 与 ORT 目录仍由初始化参数传入，不要将它们混成一个目录或随意换成其他版本。

**无 ORT 的例外：** 双 SDK 编入共享帧功能，并显式启用 `AM_INIT_SHARED_FRAMES` 时，vision 可以在不传任何运行库目录的情况下只执行共享池 attach/borrow/return。它不能加载模型或执行推理。只要传了运行库目录，就按该目录严格初始化，错误不静默降级。相机 FFmpeg 在需要解码或取流时检查，不能把仅创建相机上下文成功当成 FFmpeg 已可用。

### 2.1 运行时版本与构建身份

在初始化运行库之前即可调用 `amc_get_version_info` / `amv_get_version_info`。
输出先清零并设置 `struct_size = sizeof(am_version_info_t)`；字符串由 SDK 持有，调用方不释放，
若要在卸载 DLL 后保存，应先复制。现有结构和 ABI 1.0 不变：

| 字段 | 当前本地修复后的行为 |
| --- | --- |
| `package_version_utf8` | 来自构建该 crate 的 Cargo 包版本，不再固定追加 `-development`；开发期未升包版本的提交通过 notes 区分 |
| `build_target_utf8` | Cargo 实际 target triple，例如 `x86_64-pc-windows-msvc`；不代表其他目标已运行验收 |
| `runtime_notes_utf8` | 构建时 `source_commit`、`source_state`、`profile`、`support_shared_frames` 与 `runtimes=on-demand`；诊断键值非新的机器兼容协议 |

`source_state` 只核对 `Cargo.toml`、`Cargo.lock`、`crates/`、`src/` 的源码输入，
分为 clean/dirty/unknown；不代表整个文档工作区干净。提交为完整 Git SHA；
无自身 Git 元数据或无法核实时明确 unknown，不借用外层宿主仓库提交。
此身份是编译时快照，不在 DLL 运行时执行 Git，也不含本机源码路径或凭据。
`support_shared_frames` 仅说明公共支撑层的编译特性；每库实际能力与启用状态仍须查询，不能由它推断。

Runtime version diagnostics use the Cargo package version, actual target and build-time source
identity. Source archives without their own Git metadata report unknown identity. Returned
strings remain SDK-owned until module unload. A target triple does not imply platform acceptance.

下游应核对实际加载的 DLL 绝对路径、对应制品 SHA-256、匹配的头文件/绑定，再做 ABI 与能力
协商。提交号加 dirty 标记不是修改后源码的精确摘要，不能取代制品清单；旧版已发布 DLL
仍保留旧行为，本地修复不会自动更新下载包或宿主程序。

## 3. 模型由调用方提供

权重不包含在交付包中。调用方负责取得模型及其使用授权，按 `profiles.json` 的模型家族、导出参数、输入输出和前处理匹配后再加载。首版五类档案为：

| 类型 | 固定档案 ID |
| --- | --- |
| Pose | `pose-yolo26-e2e-coco17-v1` |
| Detect | `detect-yolo11n-nms-v1` |
| Classify | `classify-yolo11n-prob-v1` |
| Segment | `segment-yolo11n-nms-v1` |
| OBB | `obb-yolo11n-nms-v1` |

这不是“任何版本 YOLO 自动兼容”的承诺。档案加载执行相应前后处理；自定义网络通过 RAW 命名张量入口接入，图像 RAW 模式须显式指定前处理。分类概率、分割掩膜、OBB 角度等不能按普通矩形框解释，结构和几何约定以公开头文件及设计说明为准。

`examples/inference.py`、`Inference.cs`、`inference.c`、`inference.cpp` 是固定 `1×3×2×2 FLOAT Identity` 合成输入示例，**不能把任意 YOLO 权重作为其模型参数直接运行**。若运行这些示例，须自备符合此精确形状的 Identity ONNX；接真实 YOLO 使用下面的档案加载流程。

## 4. Python：直接用自己的图片和 Pose 模型

Python 封装仅用标准库 ctypes，不要求额外安装 onnxruntime Python 包。复制整个 `bindings/python`，不要只复制 `safe_sdk.py`；它依赖同目录的 `amrt_sdk.py` 与 `load_diagnostics.py`。

在 `$WORK/run_pose.py` 保存以下内容。参数依次是接口目录、DLL 目录、ORT 目录、模型路径和 JPEG 图片路径：

```python
import sys
from pathlib import Path
sdk, dlls, ort, weights, image = map(Path, sys.argv[1:])
sys.path.insert(0, str(sdk / 'bindings/python'))
from safe_sdk import VisionContext

with VisionContext(dlls / 'am_vision.dll', [ort]) as vision:
    with vision.load_model(weights, 'pose-yolo26-e2e-coco17-v1') as model:
        request_id, result = model.infer_encoded(image.read_bytes())
        with result:
            try:
                print('result task:', result.view.task)
                # 在 result 存活期间读取 result.view.pose；长期保存先复制结果。
            finally:
                vision.ack(request_id)
```

```powershell
python "$WORK/run_pose.py" "$SDK_ROOT" "$DLL_DIR" "$ORT_DIR" 'C:\Models\pose.onnx' 'C:\Images\input.jpg'
```

PNG 输入调用 `infer_encoded(data, png=True)`。异步入口传 `asynchronous=True` 获得请求 ID，随后 `vision.wait(id) → vision.fetch(id) → vision.ack(id)`，取得的每一个结果仍须单独关闭。同步超时可能返回仍在运行的请求 ID，不要直接丢弃它。

相机示例入口：

```powershell
# 调用方先安全注入 AM_SDK_SOURCE_URL、AM_SDK_USERNAME、AM_SDK_PASSWORD；不要把密码写进URL或脚本。
python "$SDK_ROOT/examples/camera.py" --dll-dir "$DLL_DIR" --runtime-dir "$FFMPEG_DIR"
# 需要 RTSP 解码帧时显式选择：
python "$SDK_ROOT/examples/camera.py" --dll-dir "$DLL_DIR" --runtime-dir "$FFMPEG_DIR" --rtsp
```

该示例默认单次 ISAPI 抓图，`--rtsp` 拉取一张解码帧验证链路，不是持续播放器。连续抓图、编码包订阅、直接 AU 解码等完整接口在 `amrt_sdk.py`；主要拥有权封装在 `safe_sdk.py`。SDK 不持久化凭据，地址中不得嵌入用户名密码。

## 5. C#：编译封装并接入真实模型

使用 `bindings/csharp/*.cs` 全部文件。无 NuGet 依赖；可用 .NET Framework 的 x64 C# 编译器，也可纳入自己的 .NET 项目。P/Invoke 是 Cdecl，字符串由封装转 UTF-8。

真实模型的调用主线如下；`weights`、`jpeg`、`ortDir` 为调用方已有绝对路径：

```csharp
using (AmrtSdk.VisionContext vision = new AmrtSdk.VisionContext(new string[] { ortDir }, false))
using (AmrtSdk.Model model = vision.LoadModel(weights, "pose-yolo26-e2e-coco17-v1", 0, true)) {
    ulong request;
    using (AmrtSdk.InferenceResult result = model.InferEncoded(System.IO.File.ReadAllBytes(jpeg), false, out request, null)) {
        try {
            AmrtSdk.amv_result_t view = result.BorrowView();
            System.Console.WriteLine("result task: " + view.task);
        } finally { vision.Acknowledge(request); }
    }
}
```

将上述逻辑放入自己的 `Program.cs` 的 `Main`，然后编译；`/out` 的目录须已存在：

```powershell
$sources = (Get-ChildItem "$SDK_ROOT/bindings/csharp/*.cs").FullName
$csc = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
& $csc /nologo /platform:x64 /warnaserror "/out:$WORK/MySdkApp.exe" $sources "$WORK/Program.cs"
$env:PATH = "$DLL_DIR;" + $env:PATH
& "$WORK/MySdkApp.exe"
```

PATH 改动只作用于当前 PowerShell 和子进程；也可将 SDK DLL 部署在自己的 EXE 目录。不要安装进 System32。`using` 按子资源在前、父资源在后退出，`BorrowView/BorrowPixel` 返回的指针只在拥有者存活期间有效；长期保存用复制接口。

## 6. C / C++17：公开头、导入库和真实调用顺序

C 入口为 `include/am_camera.h`、`include/am_vision.h`，公共结构为 `am_sdk_types.h`。Windows C/C++ 链接 `lib/win-x64/*.dll.lib`，运行时仍需相应 DLL。C++ 可直接调用同一 C ABI，也可参考 `examples/inference.cpp` 的 RAII 封装；没有另一个 C++ 编译器专属 ABI。

在 x64 Native Tools 命令环境中，以实际路径设置 `SDK_ROOT`、`LIB_DIR`、`WORK`。编译已有示例的命令为：

```bat
cl /nologo /utf-8 /W4 /WX /I "%SDK_ROOT%\include" "%SDK_ROOT%\examples\inference.c" /Fe:"%WORK%\inference_c.exe" /Fo:"%WORK%\inference_c.obj" /link /LIBPATH:"%LIB_DIR%" am_vision.dll.lib
cl /nologo /utf-8 /std:c++17 /EHsc /W4 /WX /I "%SDK_ROOT%\include" "%SDK_ROOT%\examples\inference.cpp" /Fe:"%WORK%\inference_cpp.exe" /Fo:"%WORK%\inference_cpp.obj" /link /LIBPATH:"%LIB_DIR%" am_camera.dll.lib am_vision.dll.lib
```

这两个示例仅供前述 Identity 模型。实际业务改用 `amv_model_desc_t.load_mode = AM_LOAD_PROFILE`、对应 `profile_id` 和模型路径；输入构造 `am_image_input_t`，选择 JPEG/PNG/像素视图后调用 `amv_infer_image_sync` 或 `_async`。

完整顺序：`amv_init → amv_model_load → infer → result_fetch（异步）→ result_ack → result_release → model_close → object_destroy → shutdown → object_destroy`。所有带 `struct_size` 的入参先清零并填写 `sizeof`，`abi_version=AM_ABI_VERSION`；可演进结构按指针传递，字符串为 UTF-8，不能用当前 ANSI 代码页代替。

如果程序用静态导入库，DLL 装载失败可能发生在 `main` 之前；要由自己的 UI 接管此类提示，应使用系统动态装载及函数查找，检查 `LoadLibraryEx/GetProcAddress` 与 `GetLastError`。SDK 未加载时不能调用 SDK 自己的错误接口。

## 7. 生命周期、错误及恢复

- **分配与释放配对：** 结果、帧、图像、编码包、请求快照由相应 SDK release；不要用调用方 `free/delete` 释放。`ack` 释放缓存责任，不代替已取得结果的 release。
- **关闭顺序：** 先停止提交，再释放外持结果/共享借用和注销回调，关闭订阅/模型/源，最后关闭上下文。`CLOSING` 或超时保留句柄，排空后重试；只有 `CLOSED` 才 destroy，不强杀原生线程、不带在途回调卸载 DLL。
- **共享帧：** 发布、借用、归还使用 publication ID，并携带真实或明确合成的源/帧/代次身份；未知时间不能伪装成真实设备时间。活读者不被超时强回收。异步默认复制与显式保留借用是不同内存策略。
- **依赖常驻：** ORT/FFmpeg 底层库允许常驻进程，不承诺逐次彻底卸载它们；SDK 自有句柄/线程/回调仍须排空。同进程不可任意切换不兼容 ORT 路径，必要时重启宿主。

| 出错位置 | 获取方式 | 处理重点 |
| --- | --- | --- |
| SDK 还未加载 | Python `SdkLoadError.__cause__ / system_code`；C# `SdkLoadException.InnerException`；C/C++ 系统装载错误 | 先查 DLL 路径、文件、x64 位数及传递依赖；Windows 126 不能单凭代码确定缺哪一库 |
| 初始化失败 | `am?_get_init_error`；Python/C# 已封装详情 | 缺 ORT、格式或版本不兼容、显式运行库目录不正确分别处理 |
| 对象操作失败 | `am?_get_last_error`；语言异常的 SDK 码、详情和来源 | 不仅显示数字状态；相机异步连接还需查询源状态/事件 |
| 单个异步请求失败 | `amv_request_query` 的错误快照 | 先复制详情再 release 快照，避免请求错误互相覆盖 |

Python SDK 异常有 `code/message/source/retryable/fault_class`；C# 有 `Status/Detail/SourceKey`。初始化严格失败后，纠正环境并在无登记上下文的屏障下显式调用 `amv_backend_retry`，不要无限重复初始化或静默换后端。共享帧专用模式不受“必须安装 ORT”要求约束。

## 8. 从源码构建与材料

源码 manifest 声明 Rust 最低 1.90；本轮已验证范围见 [当前验证状态](../docs/RELEASE_STATUS.md)，未来运行包工具链以其 build-record.json 为准，不代表逐个复验所有工具链版本。源码开发者需要兼容 Rust 与 Windows x64 MSVC 工具，在仓库根目录执行：

```powershell
cargo build --release --locked -p am-camera-ffi -p am-vision-ffi --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames
```

默认输出在 `target/release/`；设置 `CARGO_TARGET_DIR` 后以该目录为准。Rust 源码 SDK 可静态链接进宿主；同进程采用静态宿主或双 DLL 消费形态之一，不要把两种域重复装配。源码包与 Windows 包的来源版本、文件清单分别按各包 manifest 核对。

`licenses/`、第三方声明、对应源码及构建材料属于运行包材料，部署/再分发时保持与实际运行库对应。模型权重和现场凭据不随 SDK 包提供。内部构建日志、环境清单、测试数据库和现场数据不是接入所需文件；验收与发布审批仍以单独归档的证据和负责人决定为准，本说明不代替未执行平台/硬件测试。
