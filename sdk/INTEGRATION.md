# FrameBraid 双 SDK 接入说明

安全像素输入：优先使用Python `owner.pixels().infer(model, asynchronous=True)` 或C# `owner.Pixels().Submit(model)`；门面在调用期间锁定拥有者并保持GC根。异步默认复制，提交返回后可归还共享借用，结果仍须wait/fetch/ack。`copy_planes/CopyPlanes`是额外复制；Python RgbBuffer复制构造，C#原构造仍pin用户数组，`RgbBuffer.Copy`才隔离原数组写入。旧裸视图需调用方保护。见[验证](../docs/verification/sdk-r4-owned-pixels-20261027.md)。

Python Model.notifications/C# TryTakeNotification每模型最多保留256条提示，满时丢旧。notifications_dropped/NotificationsDropped只计语言缓冲丢失；通知不保证完整，请保留提交ID并查询/fetch/ack结果。注销保留已入队提示，原生屏障完成前固定回调拥有者。

Rust辅助`am_camera::media::h264::session::H264Hub`未发布调整：每实例16个当前源，超过返回`DenyReason::TooManySources`；穷举match需补分支。停止但未退出时继续NotReady且不新开线程，旧订阅Drop绑定原Core。此辅助公开Rust API与C ABI额度分开，不自动合并多个Hub或强杀用户resolver，见[验证](../docs/verification/sdk-r3-rust-hub-20261027.md)。

本地未发布域索引：每库4096句柄/4096 Buffer上限不变，首次登记按固定容量预留；Windows x64索引与隔离区引用槽共229376字节，另计Entry/token/服务体和原生内存，空闲容量到域卸载才回收。首次内存不足资源拒绝，不发布对象；此基础存储与Context载荷预算分列，不是RSS上限。C ABI/绑定不变，见[记录](../docs/verification/sdk-r3-domain-registry-20261027.md)。

本地未发布R3：内部Context服务登记使用16个内联槽（Windows x64为512字节），按需关闭钩子对象为8个弱引用槽（本体136字节，不含Arc头）。固定基础存储与控制载荷预算分列，不是零分配或RSS上限。直接操作`Context.services`/`CloseHooks`的Rust源码调用方须按匹配提交重编译并使用新登记方法；C布局、导出及绑定不变。见[验证](../docs/verification/sdk-r3-context-services-20261027.md)。

## Admission statistics and metadata / 准入统计与元数据

未发布修复：独立解码submit的真实入队/预算拒绝纳入Context累计计数，参数错误不作背压，reset不清历史；混合来源的accepted不代表唯一帧数。订阅对象、最大队列引用槽与缩放预设容量占控制额度，直到最后对象引用释放；载荷另计，低控制预算可明确拒绝。见[验证及剩余差异](../docs/verification/sdk-r3-budget-audit-20261027.md)。当前统计查询只实现NULL/`context`；旧对象scope合同差异仍待R4核齐，不将其冒充已支持。C ABI and external buffer ownership are unchanged.

## ISAPI interval-cache reclamation / 频率缓存回收

未发布内部修复：最后一个同源同频率连续取图订阅排空时回收该频率缓存，其他频率仍运行，已交给调用方的图片继续按`amc_image_release`释放。验证见[记录](../docs/verification/sdk-r3-capture-cache-20261027.md)，复现工具`test-camera-capture-cache.py`只使用本地Digest HTTP合成图。This fixes stale interval-cache pressure without changing ABI, endpoint rules or conservative HTTP reservations.

## Local pressure verification / 本地压力验证

新增开发工具`test-camera-pressure.py`和`test-inference-pressure.py`分别验证同源慢消费者和CPU结果/通知/共享帧隔离，命令及目标卡见[记录](../docs/verification/sdk-r3-pressure-isolation-20261027.md)。结果总配额在Context内共享；若需隔离未取结果的消费者，应使用各自Context并遵守全域预算。活共享槽不能被超时强抢，满池明确拒绝；另一池仍独立受总映射预算约束。These developer checks do not promise unlimited isolation, hardware capacity or soak acceptance.

## Local recovery verification / 本地恢复验证

Windows开发环境可运行`scripts/sdk/test-camera-recovery.py`，显式提供`--dll-dir`、`--runtime-dir`（FFmpeg）、`--mediamtx`和新的仓内`--work-dir`。默认20次故障、100次源启停及5次预热，随机回环端口、自产H.264和仅本轮合成认证，不使用相机地址；输出逐次结果与所拥有进程退出记录，拒绝覆盖旧测试目录。`test-inference-recovery-ownership.py`另需CPU ORT与合成Tensor/Batch模型目录。完整命令及范围见[验证](../docs/verification/sdk-r3-c-recovery-20261027.md)。These are local developer checks, not a hardware/soak support promise; no production interface changes.

## Local Rust H264Hub bounds / 本地Rust辅助分发约束

未发布修复：`am_camera::media::h264::session::H264Hub`的每订阅队列最多36项、8 MiB载荷（`fanout::CLIENT_QUEUE_MAX_BYTES`），GOP预滚最多35包并保留配置的字节上限。长GOP或种子超限时只先给配置，等待下一关键帧；持续视频分发不因此降采样。读取`ResyncRequired`时应重建消费端参考链，再从新关键帧继续。单包超8 MiB收到`Closed(1009, "media.packetTooLarge")`，持续不消费可收到1011；终态之后不再有新配置/包/叠层。叠层低优先级，仅保留最新一项，过载时先让位视频。

This Rust helper has bounded payload and item retention, keyframe-safe resynchronization and irreversible close. It is separate from C source/subscription APIs and their Context budgets; an 8-MiB logical queue ceiling is not a process-RSS bound. Callers retain responsibility for overall source/session deployment and any messages they hold externally. See [verification](../docs/verification/sdk-r3-rust-fanout-20261027.md). No C ABI, published binaries or platform support changes.

## Local ordered model selection / 本地有序选择

ABI1.1 r9未发布，须使用同提交DLL/绑定并检查`AMV_CAP_ORDERED_SELECTION`。路径和内存均可选择；最多3个(backend, device_id)候选，CPU=0、OpenVINO CPU=1、CUDA=2。`allow_fallback=false`只尝试首项；没有列CPU就不会回退CPU；`budget_ms`为1～5000软预算，不能强杀原生Session构造。运行库仍由调用方初始化，不自动替换CPU/GPU版ORT。

```python
with VisionContext(dll_path, [ort_directory]) as ctx:
    with ctx.load_model_selected(model_path, [(2, 0), (0, 0)],
                                 profile="pose-yolo26-e2e-coco17-v1") as model:
        print(model.get_selection_report())
```

加载失败的`SdkError.attempt_id`或C# `out attemptId`非0时，可读context尝试报告；只保留最近开始的一次，已覆盖返回resultExpired。成功模型有独立缓存，Run计数仅代表core执行/输出，不代表业务已消费。旧入口返回legacy_model而不虚构新诊断。C查询采用容量+required含NUL模式，详见ABI §3.2和`inference-selection-v1.schema.json`；查询不触发驱动/ORT操作。C#完整示例见`examples/Selection.cs`，调用示例以CPU版ORT作strict负向控制。

Python方法`load_model_memory_selected`与C# `LoadModelMemorySelected`复制接受的模型字节；原数据可在加载返回后回收。结果拥有其设备/回退字符串，模型关闭后仍可读到结果release。多模型诊断计control额度，资源不足明确拒绝，不增默认额度。加速实机/故障恢复/长跑尚未验收，见[阶段记录](../docs/verification/sdk-h5-ordered-selection-20261026.md)。

原生硬件探测与直接解码自动策略见[硬件接入](../docs/design/native-hardware.md)。原Rust CPU构造和1.0 C ABI布局未改变；本地新构建的直接C解码器已执行既有PREFER_HW策略（0），SOFTWARE_ONLY为1，旧公开CPU制品不会自动升级。RTSP策略与源报告已接线，完整设备/耐压与验收仍在H2实施中。

C# 可在结果存活时调用 `result.CopyPose()`、`CopyDetect()`、`CopyClassify()`、`CopySegment()` 和 `CopyObb()` 复制结构化结果；复制数组可修改且不会更改原生结果，旧 `BorrowView()` 仍是借用。分割掩膜单次复制上限 64 MiB；结果关闭后不能再调用复制方法。

C# `result.CopyPose()`, `CopyDetect()`, `CopyClassify()`, `CopySegment()` and `CopyObb()` copy structured results while the result is alive; returned arrays are independent and mutable. `BorrowView()` remains borrowed. Segment masks have a 64 MiB cumulative copy budget; copying after close is rejected.

Python 另提供 `copy_detect()`、`copy_classify()`、`copy_segment()`、`copy_obb()`，输出结构、限制和当前验证见[记录](../docs/verification/sdk-r4-python-structured-copy-20261027.md)。结果需在调用时存活，返回值随后独立；分割掩膜会产生额外字节复制。

Python also provides `copy_detect()`, `copy_classify()`, `copy_segment()`, and `copy_obb()`. The result must remain alive during the call; returned values are independent. Masks are copied. See the linked local validation for limits and scope.

Python Pose 结果可在结果存活时调用 `result.copy_pose()` 取得独立 `(bounds, confidence, keypoints)` 元组；随后可释放结果并继续读取复制值。原 `result.view` 为借用，释放后不可用。仅为本地未发布功能。

For local unreleased Python builds, `result.copy_pose()` copies pose boxes and keypoints while the result is alive. The copied tuple survives release; `result.view` remains a borrowed low-level view.

旧 `amv_query_backends` 现只读本 context 的显式 CUDA probe 缓存；需要设备判定时先调用 `amv_probe_inference_hardware`。hw_detected=0 需结合 note 区分未知和已确认无设备。

`amv_query_backends` now reads the context probe cache. Call `amv_probe_inference_hardware` first when device visibility is required; inspect note to distinguish unknown from confirmed absence.

硬解 selection 可返回 backend_support；compiled / codec_configured 为 null 时表示未检查，不等于不支持或实际硬解成功。Rust 手写 SelectionReport 字面量需增加该字段。

Hardware selection may include backend_support; null means unchecked, not unsupported or verified decoding. Rust SelectionReport struct literals require the new field.

硬解自动选择的 JSON 原因可能由通用解码错误细化为 Inventory 分类；调用方应允许未知原因及缺失项。无设备时选择报告不提供完整编译清单。

Hardware selection can report more specific Inventory reasons; clients must tolerate unknown reasons and absent fields. No-device selection is not a complete backend build inventory.

Rust调用方可用`CpuDecoder::create_with_policy`获得初始化选择与拥有的原因报告，仍须在送包/收帧后核实实际后端。直接DLL在创建/reset时选择硬件，初始化选中但未出帧时get_info后端NULL；真实出帧后返回ffmpeg-d3d11va/dxva2/cpu。硬件运行失败先过CPU预算，递增代次并等待关键帧；仅首次输出前单启动画面允许保留原输入重试一次，显式reset才重试硬件。见[直接入口验证](../docs/verification/sdk-r2-direct-auto-20261026.md)。初始化1.5秒为原生调用间软预算，不强杀驱动；不保证GPU启动或每帧比CPU更快。

底层Rust回读可选择作用域预算复制或单缓冲复用，复用时许可须持有到清缓存/销毁；输出Vec独立。C ABI像素复制入口由SDK自动管理暂存Charge，调用方无需自行管理；CPU恢复预留可转交外部帧且仍核持帧上限。见[回读与转换缓存记录](../docs/verification/sdk-r2-readback-cache-20261026.md)。

本地未发布R5优化可及时释放未执行且已取消/替换的排队输入；正在读取的输入及含有效项的原生批次仍须等实际结束，不能把cancel当作可立即释放调用方缓冲。见[优先项验证与限制](../docs/verification/sdk-r5-priority-20261026.md)，旧发布包能力不因此改变。

本说明面向使用 SDK 的开发者。交付主体是可独立构建的 Rust 源码 SDK；DLL 目标为 `am_camera.dll` 与 `am_vision.dll`，通过 C ABI 供 C/C++、C#、Python 调用。使用运行包不需要 Rust、FFmpeg 命令行或 AM-RT 业务服务。已发布版本及其真实验证范围见[交付状态](../docs/RELEASE_STATUS.md)；源码构建及运行见 [快速接入](../docs/GETTING_STARTED.zh-CN.md)。

## 1. 先分清两种包

本地ABI 1.1新增`amc_probe_decode_hardware(ctx, codec, budget_ms)`及`amc_get_decode_hardware_report(object, buffer, capacity, required)`。先查ABI再用新增符号；新DLL接受1.0初始化，旧DLL不接受1.1。新Python高层绑定须配新DLL；C#保留核心1.0初始化，新硬件方法检查1.1。旧公开v0.1.0包取对应tag绑定，不把当前main绑定拷进旧包。

Python `CameraContext.probe_decode_hardware()`返回dict，`get_decode_hardware_report()`仅复制缓存；C#对应`ProbeDecodeHardware`/`GetDecodeHardwareReport`返回UTF8 JSON字符串。探测报告selected仅表示初始化，actual_backend为空且stream_verified=false；独立decoder真实输出后再据快照确认。快照支持context和独立decoder、关闭后保留最后结果，r7接受RTSP source句柄；source关闭返回closed且无实际活动值。JSON最低字段约束、预算、线程与所有权见[ABI 1.1合同](../docs/design/dual-dll-abi.md)，实测见[验证](../docs/verification/sdk-r2-abi11-hardware-20261026.md)。

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

### RTSP配置与转换预算（本地R3，未发布）

C入口在配置复制/包framing分配前预留工作区；成功后载荷容量和Reader保留配置继续计费，临时余量归还。原生错误时先销毁Reader缓冲再归还许可，拉取/外持的包不因Reader关闭失效。HTTP探测先预留既有4 MiB抓图包络，RTSP探测采用相同Reader；候选资源不足仍报Pending，可在释放占用或新建预算合适的Context后显式重试。公共set_limits不能通过恢复旧值提高已下调的限额。

Rust高级调用新增`Reader::open_with_budget`、`read_packet_with_budget`与`retained_buffer_bytes`；预算闭包在SDK相关分配前调用，调用方必须保留许可覆盖Reader配置、错误后缓冲及返回包实际容量。成功后可按实际保留容量拆分额度；不能在闭包返回时自动退账。原`open/read_packet`兼容保留调用方自行管理的行为。FFmpeg/网络库内部内存、allocator开销不在此回调硬限内；未消除既有包转换复制。

推理通知注册时先按完整条目（含许可）为固定队列槽预留控制字节；存储占一个控制项，活通知另外占项和关闭屏障，不重复计存储字节。空闲槽继续占字节、运行期不扩容，排队加忙回调不超过槽数；注销/停止且回调排空后释放缓冲。至少需一个存储项及一个活通知项；额度不足可在注册时明确resourceExhausted，初次失败不残留通知线程，预算拒绝新注册时原通知已排空、回调为未注册。通知丢弃不改变结果查询或请求终态。C头、布局、能力位、Python/C#签名未变，无需换ABI冻结代次。

媒体源事件也先为固定队列预付控制存储，再登记活事件项；空闲槽不提前退账，运行期不扩容。最多64项且受Context剩余控制预算限制，存储本身占一项。满载只丢待处理旧提示，不能抢占忙回调；只有忙项时丢新提示，丢弃不影响权威源状态或视频数据。注销/停止待回调排空后释放。小额度注册可明确resourceExhausted，首次失败不残留事件线程；预算拒绝新注册时原回调已排空、处于未注册状态。

源码 manifest 声明 Rust 最低 1.90；本轮已验证范围见 [当前验证状态](../docs/RELEASE_STATUS.md)，未来运行包工具链以其 build-record.json 为准，不代表逐个复验所有工具链版本。源码开发者需要兼容 Rust 与 Windows x64 MSVC 工具，在仓库根目录执行：

```powershell
cargo build --release --locked -p am-camera-ffi -p am-vision-ffi --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames
```

默认输出在 `target/release/`；设置 `CARGO_TARGET_DIR` 后以该目录为准。Rust 源码 SDK 可静态链接进宿主；同进程采用静态宿主或双 DLL 消费形态之一，不要把两种域重复装配。源码包与 Windows 包的来源版本、文件清单分别按各包 manifest 核对。

`licenses/`、第三方声明、对应源码及构建材料属于运行包材料，部署/再分发时保持与实际运行库对应。模型权重和现场凭据不随 SDK 包提供。内部构建日志、环境清单、测试数据库和现场数据不是接入所需文件；验收与发布审批仍以单独归档的证据和负责人决定为准，本说明不代替未执行平台/硬件测试。

本地r7新增`amc_source_set_decode_policy(source, policy)`；Python `Source.set_decode_policy(software_only=True)` / `Source.get_decode_hardware_report()`，C# `CameraSource.SetDecodePolicy(true)` / `GetDecodeHardwareReport()`。调用顺序：打开源→在单拥有者且未订阅像素时设置→订阅；SOFTWARE_ONLY=1不探测硬件，默认PREFER_HW=0不是“最快后端”。多拥有者或首次像素订阅后变更拒绝，可用reuse=0新开独占源；push尚无回调也冻结。相同值幂等，不触发刷新。

查询返回requested_policy/effective_policy、初始化selection、实际actual_backend、last_failure/last_status、decode_generation；仅有真实帧才stream_verified。像素失败后新关键帧恢复CPU，不重放旧RTSP包，不改变压缩配置代次；无像素消费者不创建decoder。本地后续R3已将初始化/解码移入受既有额度限制的worker（见下节），不能据此保证任意驱动故障的关闭时延；详见[限制和量化](../docs/verification/sdk-r2-rtsp-hardware-20261026.md)。本机RGBA回读硬件慢于CPU，按宿主需求选择策略，不保证自动最快。

Local H5 core: bounded Rust model selection and explicit CUDA device discovery are implemented; CPU fallback/strict and native failure diagnostics tested. Explicit C inference device probes/cached reports are also available locally; C model-selection extensions, accelerated inference and recovery acceptance remain open. See [core verification](../docs/verification/sdk-h5-selection-core-20261026.md). Hardware-vs-CPU RGBA latency alone is not a defect; CPU offload and overall budgets require measurement. / H5核心阶段已保存，未发布；不等于整包完成。

### Local RTSP parameter reuse / 本地RTSP参数复用

Local RTSP pixel initialization/decode/readback now runs in one lazy bounded worker per source, sharing the existing decoder limit; compressed-only sources create none. Overflow restarts only the pixel reference chain. Native work must actually finish before source close can report Closed; do not launch replacement sessions repeatedly when a driver call is still pending. See [bounded execution validation](../docs/verification/sdk-r3-pixel-worker-20261026.md). / 像素处理移出读流线程，队列计入现有统计；通用Media threads字段未完整覆盖，0不代表无worker。采样上限合同不变；后续本地[节拍修复](../docs/verification/sdk-r2-cadence-20261026.md)只对限帧消费者在有更新待处理包时省略旧画面物化，保留参考链；0上限不额外合并。像素worker内部最多提前50ms准备并用短sleep等待真实deadline，不阻塞压缩读流、不承诺硬实时或最低FPS。H.264三组实际交付已改善，延迟代价及全编码矩阵分别登记。

Local RTSP parameter changes reuse the existing selected device and decoder permit at the next random-access picture; queried execution is cleared until a new frame succeeds. Retained frames remain valid. Budget pressure waits for a later random-access picture; native failure follows CPU recovery without replaying the failed RTSP packet. No new API or published asset is required by this internal change; see [validation](../docs/verification/sdk-r2-rtsp-reconfigure-20261026.md). / 本地参数重建复用不代表硬件整包或真实采样帧率问题已完成。

### Local inference device reports / 本地推理设备报告

Unpublished ABI1.1 r8 only: use bindings and DLL from the same source revision. C callers invoke `amv_probe_inference_hardware(ctx, 1500)` explicitly, then `amv_get_inference_hardware_report(ctx, buffer, 65536, &required)`; required includes NUL. Zero-capacity length lookup does not probe. Python: `VisionContext(..., shared_frames=True).probe_inference_hardware()` / `.get_inference_hardware_report()`; C#: `ProbeInferenceHardware()` / `GetInferenceHardwareReport()`. Close/dispose the context normally. Examples of exact raw calls and buffer/error handling are in `scripts/sdk/test-inference-hardware.py`.

设备报告可在无ORT的纯共享帧context查询，显卡被枚举不等于ORT/CUDA可用。常规查询不刷新驱动；缺失/失败读device_probe.stopped，不要当作零设备。observations为SDK域累计观察，非当前模型健康/全GPU节点证明；模型选择仍按原C接口或新增Rust策略，本阶段不扩旧运行库替换合同。默认64KiB控制预算内缓存16KiB、查询48KiB，两者各一项；其他控制对象或并发查询可能明确resourceExhausted，应归还控制对象/减少并发，不能无限重试。关闭态仅返回空快照。

有序选择的context尝试是加载时快照，Run只刷新模型缓存。Python/C#为关闭安全串行化同一资源操作；需边执行边查状态时选择异步推理。新查询不提供后台刷新或驱动测速。

新有序加载按各候选实际配置准入，不让列表内OpenVINO的最大线程预留阻断CPU；允许回退仍须列出CPU，strict仍拒绝。Rust源码高级调用可用`Model::load_selected_admitted`提供短小准入闭包，调用方持有的许可须覆盖整个模型寿命；普通load/load_selected兼容不变，C签名/布局/能力位不变。原生Run失败不自动重放，关闭旧模型再显式加载新身份；见[资源准入验证](../docs/verification/sdk-h5-resource-admission-20261027.md)。

## Standalone decoder configuration storage / 独立解码配置存储（本地R3，未发布）

独立解码器的extradata复制先占Context帧字节预算，参数缓存另计，原生重配置的Annex B序列化暂存先预留并持有至调用结束。重置构造期间新旧上下文重叠占用，低预算可明确返回resourceExhausted；构造或缓存更新准入/分配失败不提交新配置/缓存。`amc_decoder_reset(dec, NULL)`共享不可变配置，不另复制其extradata；各原生上下文自己的参数缓存仍分别计费。重置失败保留旧代次与外持帧，不承诺任意原生驱动故障均可无损回滚。FFmpeg/驱动内存、allocator开销不冒充SDK字节硬限。

Rust `visit_access_units` synchronously visits borrowed NAL slices without a list allocation; earlier visits may happen before an invalid tail is reported. Plan changes during visitation and commit only after success. The old Vec API and existing accepted input boundary remain; this is not full-pipeline zero-copy. See [verification](../docs/verification/sdk-r3-decoder-retained-20261027.md).

Local standalone-decoder input slots and the existing 256-entry timing table reserve control storage before allocation and keep idle capacity charged until context replacement/close frees it. Reset temporarily needs both old and new storage; insufficient control budget leaves old generations and frames intact. The default control ceiling remains 64 KiB, so independent maximum limits are not a promise that every maximum fits simultaneously. Smaller queue_max_items reduces input metadata. C ABI is unchanged; see [validation](../docs/verification/sdk-r3-decoder-metadata-20261027.md). / 输入/时间元数据先准入、固定容量及持续计费。

Local RTSP pixel queues reserve their actual bounded reference slots once; the 128-entry receipt-time table evicts before insertion to prevent a transient 129th entry from doubling storage. Existing budgets, final association order and sampling policies are unchanged. This removes excess SDK container capacity, not native decoder memory or whole-process RSS; see [validation](../docs/verification/sdk-r3-rtsp-storage-20261027.md).

Local continuous ISAPI subscriptions prepay their reference slots and 120-entry FPS history, retaining idle storage credits until destruction. History evicts before insertion; rejected creation/registration does not retain a continuous-consumer slot. Low control budgets may reject start explicitly. Stop drains work; destroy/release frees the retained object storage, while externally held images keep their existing lifetime. [Verification](../docs/verification/sdk-r3-capture-storage-20261027.md).

Local inference executor storage now shares the existing 64 MiB global ceiling with in-flight preprocessing: fixed Job/worker-handle slots plus retained Context-index capacity. Index growth reserves old/new overlap before allocating and before publishing/replacing requests. Context/class limits remain separate; idle capacity stays charged until executor destruction. Context queue_bytes retains its logical data meaning, not total executor memory or RSS. Bounded stack snapshots and direct folds avoid temporary Vec lists. [Validation](../docs/verification/sdk-r3-executor-storage-20261027.md).

Local inference request tables also reserve backing capacity and growth overlap from Context control bytes. Undersized budgets can return resourceExhausted before publishing an ID or replacing a pending request. Idle capacity stays charged until the service is destroyed, but is reusable and does not count as an externally held buffer or control snapshot item. Reducing a policy does not revoke existing storage. Context queue_bytes still reports logical in-flight/results, not all control memory. [Validation](../docs/verification/sdk-r3-record-storage-20261027.md).

Local Media capture request/coalescing/cadence/consumer/subscription indexes and Flight bodies now reserve control bytes, including growth overlap and idle retained capacity. Request control-item limits remain in force. Pre-admission table failures return no ID; an already accepted asynchronous request may instead reach a failed terminal state if subsequent Flight/HTTP/cache work fails. Cache failure completes and retires the shared Flight, permitting later attempts. Closed subscriptions leave the active index without revoking their still-valid handles or held images. Lowering admission limits does not close existing sources. Metadata does not add external-buffer close barriers; queue_bytes remains logical data usage. [Validation](../docs/verification/sdk-r3-capture-tables-20261027.md).

Local ABI 1.1 r10: check `AMC_CAP_WORKER_BUDGET`, then use `amc_get_worker_budget(ctx, &limit)` / `amc_set_worker_budget(ctx, maximum)`. Default/ceiling 64 SDK workers per Context; only tighten, zero blocks new workers, and shrinking below live/reserved slots returns resourceExhausted without changing the old limit. Python: `camera.set_worker_budget(8)` / `get_worker_budget()`; C#: `SetWorkerBudget(8)` / `GetWorkerBudget()`. `examples/camera.py --worker-budget 8` configures before opening sources. A lazy pixel worker may enter resource wait after subscription admission while compressed delivery continues. Stats report SDK slots (including startup reservations), not caller/driver OS threads; peak preserves history across limit changes. Closing allows read-only query; mutations/Closed queries reject. Use matching local DLL/bindings, not public ABI 1.0 artifacts. [Evidence](../docs/verification/sdk-r3-media-workers-20261027.md).

### 模型输入的稳定性 / Model input stability

文件模型及其外部权重应在整个加载调用期间保持可信且不被修改或替换。SDK预检查按已打开文件句柄读取，检查阶段的SDK自有副本不超过256 MiB；ORT文件入口会重新按路径打开，SDK无法保证ORT看到相同文件或约束其内部读取。内存模型在同步加载返回前需保持字节有效且不可变，返回后调用方可释放或修改；不支持外部权重的限制保持不变。

Keep path models and external weights trusted and unchanged throughout loading. SDK preflight reads at most 256 MiB from an opened handle, but ORT reopens the path; the SDK cannot guarantee an identical file or cap ORT internal reads. Memory model bytes must remain valid and unchanged until synchronous loading returns; callers may release or modify them afterwards. External weights remain unsupported for memory models.
