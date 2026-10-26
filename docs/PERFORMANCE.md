# Performance measurement / 性能测量

These local tools call the real SDK DLL or Rust production functions. They do not
claim YOLO accuracy, universal hardware support or camera-to-display performance.
Reports are written under ignored `.local/evidence`; synthetic fixtures stay under
`.local/tests`. Nothing is published by these commands.

这些工具测量实际调用边界，不把合成模型当真实业务性能。首轮基线和限制见
[R1 阶段验收](verification/sdk-r1-complete-20261026.md)，早期工具检查点另行保留。

Native decode measurements may explicitly request `--backend d3d11va --adapter 0`
or `--backend dxva2`; CPU remains the default. The tool checks real hardware output,
records backend/LUID and separates transfer/scale timing. These are function-boundary
measurements, not browser playback or end-to-end FPS. See [R2 budget/cache validation](verification/sdk-r2-readback-cache-20261026.md).

硬件路径需明确请求，并以真实输出核后端；报告GPU回读与颜色转换分段，不把偏好或短测等同完整硬件支持。构建源码摘要包含拆出的转换/回读模块，比较前核输入、实际后端、设备和输出校验一致。

## Acceptance policy / 验收口径

Local R2 direct tensor preparation now removes one full temporary f32 allocation/copy while preserving complete tensor bytes and transforms. Allocation counts, paired timings and actual CPU Pose/retained-input checks are recorded in [verification](verification/sdk-r2-prepare-direct-20261026.md). The measured local preparation gain is not end-to-end inference, CPU offload or a published release claim.

R2直写最终对齐张量已有限验证：每次640×640减少4,915,200字节临时分配/复制，保持完整输出；实际源采样低交付另行保留，不因本优化关闭。

Current decisions use [MET-2](PERFORMANCE_ACCEPTANCE.md): separate resource offload, useful delivery, latency, correctness and recovery. A hardware path is not required to beat CPU latency. Legacy commands, JSON fields and 5% alerts remain unchanged. The separate MET-2 stream tool below now measures process CPU time, per-PID GPU counters and A/A variation; it does not automatically grant performance acceptance.

用户已授权恢复开发。S1新增工具与基线见[本轮记录](verification/sdk-met2-s1-20261026.md)，旧工具/数据不改写；历史评价见[复核记录](verification/sdk-metrics-review-20261026.md)。

## Reproduce / 复现

### MET-2 paced GOP / 固定节拍连续GOP

`benchmark-decode-stream.py` calls the real Media DLL, preloads a bounded H.264 MP4 fixture with repeated parameter sets, and paces complete Annex-B access units by DTS. CPU time includes Python/SDK/native libraries and observation; it is not SDK-exclusive cost. Warm-up, steady-state delivery and trailing B-frame drain are separate. GPU counters use Windows PDH English names, retain per-process/device/engine identities, and report missing observations as unknown. GPU engines are never summed into one percentage.

先按[测前目标卡](verification/sdk-met2-s1-target-20261026.md)固定场景。工具只接受新的仓内证据目录，保存DLL实际构建信息/摘要、输入/运行库/工具指纹、每轮raw及工具源码副本；未知不填0。`review.json`仅汇总各策略A/A最大最小值与跨度，不自动判定收益/回归。原`performance.v1`比较器不改。

```powershell
python -B -m unittest discover -s scripts/sdk -p 'test_perf_*.py' -v
python -B scripts/sdk/benchmark-decode-stream.py --dll-dir .local/build/current/release --runtime-dir .local/runtimes/ffmpeg-h2-20261026/runtime --input .local/tests/met2-s1/source.mp4 --target-card docs/verification/sdk-met2-s1-target-20261026.md --output .local/evidence/met2-s1-new --observation-controls
```

样本由本机FFmpeg备料：`testsrc2` 1920×1080/30 FPS、10秒，libx264/yuv420p，`bframes=2:keyint=30:scenecut=0:repeat-headers=1`、4字节MP4 NAL长度。备料/ffprobe不参与解码窗口。默认每策略三轮、2秒预热/6秒窗口，另各一轮关闭采样；GPU采样250ms/最多4096条，源码与输入在测前准备，不在处理热路径落盘。

这是S1单路调用与资源基线；不包含网络、显示、全图跨语言复制、模型精度、混合消费者、多路或耐压。公开SDK/ABI、默认选择策略、支持范围均不因工具变化扩大。新鲜度200ms等数值仅属目标卡实验线，不是产品SLA。

### MET-2 real-model CPU baseline / 真实模型CPU基线

`benchmark-inference-resources.py` uses the frozen Pose model and the real Vision image/async path. It separates process/model construction from paced 5 FPS response and resource measurements. CPU-runtime OpenVINO/CUDA strict rejection and allowed CPU fallback run in separate processes; they do not verify accelerator execution. See the [target card](verification/sdk-met2-s3-target-20261026.md) and [results](verification/sdk-met2-s3-20261026.md).

```powershell
python -B scripts/sdk/benchmark-inference-resources.py --dll-dir .local/build/current/release --runtime-dir .local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/onnxruntime-cpu --model .local/tests/met2-s3/yolo26n-pose.onnx --input .local/tests/met2-s3/input.png --target-card docs/verification/sdk-met2-s3-target-20261026.md --output .local/evidence/met2-s3-new
```

仅接收档案指定SHA的测试模型；模型、图片和运行库自行备料于私有测试目录，工具不下载/发布。默认三轮20请求/4秒，另观察关闭；有界子进程及输出源码副本用于追溯。输出校验计进程CPU但不计submit→fetch延迟，GPU未知不写0；raw/P95与整数毫秒SDK区间不冒充精确ORT耗时。没有硬件EP运行库时只能验证缺依赖与CPU回退，不能称硬件选择整包完成。

### MET-2 live sampling / 实际RTSP节拍

The [cadence A/B](verification/sdk-r2-cadence-20261026.md) uses bounded warmup-queue draining after resource-counter initialization for both binaries. Three interleaved H.264 pairs restore useful capped delivery, at higher measured receipt age still within the predeclared guard. Exact selection intervals are verified separately from pull timestamps; raw preview failures are retained. / 三组交替对照不以配置FPS或CPU低当收益，全编码/硬件及长跑未关闭。

`benchmark-rtsp-sampling.py` creates only an isolated loopback MediaMTX and a paced FFmpeg publisher. It measures encoded-only, 5/10/30 FPS ceilings and mixed consumers against real QPC receipt age, retaining actual delivery rather than claiming configured rates. See the [target card](verification/sdk-met2-s2-target-20261026.md) and [observed gaps](verification/sdk-met2-s2-20261026.md). Active queued SDK frames may consume external-frame quota; leak checks require source/queue drain.

```powershell
python -B scripts/sdk/benchmark-rtsp-sampling.py --dll-dir .local/build/current/release --runtime-dir .local/runtimes/ffmpeg-h2-20261026/runtime --input .local/tests/met2-s1/source.mp4 --mediamtx .local/tools/mediamtx-h2/mediamtx.exe --target-card docs/verification/sdk-met2-s2-target-20261026.md --output .local/evidence/met2-s2-new --work-dir .local/tests/met2-s2-new
```

仅使用自行备料的本地工具/视频，默认三轮2秒热身/4秒窗口，退出只关闭自建进程；不连接用户相机。80%等是目标卡调查线，`max_fps`是最高速率合同，不是最低交付保证。新脚本不提供真实转换次数或历史二进制收益证明；已观察30 FPS配置低于调查线，不能将工具正常退出当优化验收。

Local sampling ablation uses the Release `native_sampling` test with explicit `AM_RT_FFMPEG_DIR` and `AM_SDK_H264_SAMPLE`. It runs real decode and production fanout under a virtual 30 FPS clock, observes native sws_scale calls, excludes full-plane hash/pull work from timing and retains 5% alerts. See [sampling measurements](verification/sdk-r2-sampling-20261026.md); this is not RTSP network throughput or a historical-binary A/B.

Direct DLL native-policy smoke: `python scripts/sdk/test-decoder-hardware.py --dll-dir <release> --runtime-dir <matched-ffmpeg> --samples-dir <synthetic-inputs>`, adding `--software` for a same-build CPU control. Records actual backend, generations, pixel hashes, create/first-receive times; requires the documented four synthetic fixtures and real hardware for native mode. See [direct decoder verification](verification/sdk-r2-direct-auto-20261026.md). Cold-start costs are not sustained-stream FPS or a historical-binary A/B.

在仓库根执行，先准备匹配 FFmpeg/ORT 运行库；下列目录是本地候选示例，
不属于源码 SDK 的固定依赖。`--output` 必须是 `.local/evidence` 下尚不存在的目录，
原始基线不会被覆盖。Release 与 Debug 不能直接对照。

```powershell
python -B -m unittest discover -s scripts/sdk -p test_perf_common.py -v
cargo build --release -p am-vision-ffi -p am-camera-ffi --features shared-frames
python -B scripts/sdk/benchmark-prepare.py --output "$PWD/.local/evidence/prepare-new"
python -B scripts/sdk/benchmark-decode.py --runtime-dir .local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/ffmpeg --input .local/tests/r1-baseline-inputs/frame.h264 --output "$PWD/.local/evidence/decode-new"
python -B scripts/sdk/benchmark-inference.py --dll-dir .local/build/current/release --runtime-dir .local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/onnxruntime-cpu --model .local/tests/r1-baseline-inputs/tensors/image.onnx --output "$PWD/.local/evidence/inference-new"
python -B scripts/sdk/benchmark-capture.py --dll-dir .local/build/current/release --output "$PWD/.local/evidence/capture-new"
python -B scripts/sdk/benchmark-observation.py --dll-dir .local/build/current/release --runtime-dir .local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/onnxruntime-cpu --model .local/tests/r1-baseline-inputs/tensors/image.onnx --output "$PWD/.local/evidence/observation-new"
python -B scripts/sdk/perf_common.py .local/evidence/prepare-before/round-1.json .local/evidence/prepare-after/round-1.json
```

合成输入生成（FFmpeg CLI 仅用于备料，不是 SDK 解码时启动子进程）：

```powershell
New-Item -ItemType Directory -Force .local/tests/r1-baseline-inputs
ffmpeg -hide_banner -loglevel error -n -f lavfi -i testsrc2=size=1920x1080:rate=1 -frames:v 1 -c:v libx264 -threads 1 -preset ultrafast -tune zerolatency -f h264 .local/tests/r1-baseline-inputs/frame.h264
python -B scripts/sdk/generate-tensor-samples.py --output .local/tests/r1-baseline-inputs/tensors
```

模型生成需 ONNX/NumPy，基准调用本身不依赖 Python 推理引擎。持续比较保留原模型字节，
不要重新导出后假定身份相同。基准 Rust 示例可以独立运行；Python 包装器负责绑定身份。

## Evidence contract / 证据约定

JSON `schema=framebraid.performance.v1`：

| 字段 | 含义 |
| --- | --- |
| `identity` | 负载、输入/模型/运行库 SHA-256、机器、实际后端、线程、预算、预热及计时边界；不适用的摘要必须使用明确的 `not-applicable:原因` |
| `build` | 完整源码提交、clean/dirty/unknown、制品 SHA-256 与 Release/Debug；局部示例另存关键源码摘要，DLL 工具另存载入版本 notes |
| `counts` | attempted = delivered + rejected + dropped；一个成功交付对应一个延迟样本 |
| `samples_ns` | 原始单调钟时长；最多 100,000 项，不在生产热路径写文件 |
| `elapsed_ns` | 推理为负载实际墙钟；局部基准为函数耗时之和，不能把两者当同一种端到端吞吐 |
| `unknown` | 没有接口边界的数据明确 null；SDK 毫秒级执行时间不伪装微秒精度 |

分位数为 nearest-rank：排序后取 ceil(p×N)，不插值。N<1000 时 P99 仍输出观察值，
但 `p99_representative=false`，不能当可靠尾延迟结论。1000 也不自动证明统计收敛。
比较器要求 identity 完全相同，允许源码/制品变更，拒绝 Debug/Release 混比；
像素/张量摘要变化先做正确性审查，少交付不能伪装提速。历史比较器固定警戒是P95或有效吞吐恶化超过5%，仅保留为调查标签；不构成统一验收线。MET-2按测前目标卡及A/A/绝对影响判断，不能事后为候选放宽硬预算；当前比较器不自动授予验收通过。

## Boundaries and limitations / 边界与限制

- 前处理：1920×1080 RGBA → 640×640 letterbox/NCHW/f32，单线程；摘要、输入生成及输出析构不计入 `prepare`。Rust 分配计数在独立调用中采集，计请求字节而非峰值/RSS/FFmpeg 原生堆；计数关闭仍保留轻量分支。
- 解码：单个 H.264 IDR AU 重复输入，单线程 CPU；分列 send/peek 和 copy_rgba。SwsContext 计数包裹真实 FFmpeg 函数，不替换其实现；创建/释放计数含预热和关闭。不是网络、B 帧、多流或硬解基准。
- 推理：真实 Vision DLL、CPU、合成 Identity 模型，异步 submit → fetch 计时含轮询及消费等待；不含结果验证/释放。每项核帧 ID 和完整张量字节。负载墙钟包含构造、验证及归还，不能将其吞吐当纯 ORT 吞吐。
- 采集：真实 Media DLL 对本机随机端口HTTP Digest替身取合成JPEG，单请求/预算内/过载慢消费三档；校验请求ID、完整图片、接收时间和归还后资源。4个SDK采集worker预算不等于4条真实网络连接，连接共享可能使实际HTTP峰值为1。
- 观察开关比较只增减额外 stats 拉取，不移除验证和基本时钟采样。合成小模型中轮询和系统调度噪声明显，三轮结果须联合检查，不挑最快一轮。
- 基础负载预算见 `scripts/sdk/perf-budgets.json`，摘要写入每次报告；本轮局部/合成数据不能替代真实模型、R3/R5长跑或硬件验收。进程内存含Python/运行库，只采边界不证明运行期间绝对峰值。

上述命令不连接现场相机，不改宿主，不重算 ABI 冻结。核心候选还须按 OpenSpec
核来源修订、生命周期和定向测试。发布/支持范围维持 [Support](SUPPORT.md) 的现状。
