# Performance measurement / 性能测量

These local tools call the real SDK DLL or Rust production functions. They do not
claim YOLO accuracy, hardware acceleration or camera-to-display performance.
Reports are written under ignored `.local/evidence`; synthetic fixtures stay under
`.local/tests`. Nothing is published by these commands.

这些工具测量实际调用边界，不把合成模型当真实业务性能。首轮基线和限制见
[R1 阶段验收](verification/sdk-r1-complete-20261026.md)，早期工具检查点另行保留。

## Reproduce / 复现

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
像素/张量摘要变化先做正确性审查，少交付不能伪装提速。初始固定警戒线是 P95 或有效吞吐
恶化超过 5%，触线需调查而非事后放宽；比较器不自动授予验收通过。

## Boundaries and limitations / 边界与限制

- 前处理：1920×1080 RGBA → 640×640 letterbox/NCHW/f32，单线程；摘要、输入生成及输出析构不计入 `prepare`。Rust 分配计数在独立调用中采集，计请求字节而非峰值/RSS/FFmpeg 原生堆；计数关闭仍保留轻量分支。
- 解码：单个 H.264 IDR AU 重复输入，单线程 CPU；分列 send/peek 和 copy_rgba。SwsContext 计数包裹真实 FFmpeg 函数，不替换其实现；创建/释放计数含预热和关闭。不是网络、B 帧、多流或硬解基准。
- 推理：真实 Vision DLL、CPU、合成 Identity 模型，异步 submit → fetch 计时含轮询及消费等待；不含结果验证/释放。每项核帧 ID 和完整张量字节。负载墙钟包含构造、验证及归还，不能将其吞吐当纯 ORT 吞吐。
- 采集：真实 Media DLL 对本机随机端口HTTP Digest替身取合成JPEG，单请求/预算内/过载慢消费三档；校验请求ID、完整图片、接收时间和归还后资源。4个SDK采集worker预算不等于4条真实网络连接，连接共享可能使实际HTTP峰值为1。
- 观察开关比较只增减额外 stats 拉取，不移除验证和基本时钟采样。合成小模型中轮询和系统调度噪声明显，三轮结果须联合检查，不挑最快一轮。
- 基础负载预算见 `scripts/sdk/perf-budgets.json`，摘要写入每次报告；本轮局部/合成数据不能替代真实模型、R3/R5长跑或硬件验收。进程内存含Python/运行库，只采边界不证明运行期间绝对峰值。

上述命令不连接现场相机，不改宿主，不重算 ABI 冻结。核心候选还须按 OpenSpec
核来源修订、生命周期和定向测试。发布/支持范围维持 [Support](SUPPORT.md) 的现状。
