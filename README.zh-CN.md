# FrameBraid SDK

![FrameBraid SDK — Media SDK 与 Vision SDK](assets/brand/framebraid-banner.webp)

[English](README.md) · [文档入口](docs/README.md) · [快速接入](docs/GETTING_STARTED.zh-CN.md) · [后续路线图](docs/ROADMAP.md)

**可复用的相机接入、视频处理与 ONNX 推理基础组件。**

FrameBraid 是 Rust 源码 SDK，通过 C ABI 供 C、C++、C#、Python 接入，一个仓库管理两个组件：

| 组件 | 用途 |
| --- | --- |
| **FrameBraid Media SDK** | ISAPI 图像、RTSP 编码流、解码帧、独立解码及像素转换 |
| **FrameBraid Vision SDK** | ONNX 执行、有界异步处理、原始张量及模型档案后处理 |

**当前状态：v0.2.0 开发者预览版。**[下载源码与 Windows x64 二进制包](https://github.com/xiaoyanse/framebraid/releases/tag/v0.2.0)。C ABI 1.1（对 1.0 的兼容新增）；已验证范围为 **Windows x64 CPU 与硬解探测**；Linux/macOS/移动端二进制及 GPU 推理验收另行推进，见[验证范围](docs/RELEASE_STATUS.md)。

## 适用场景

- 为桌面应用或既有 Python 服务增加相机接入。
- 单次或连续 ISAPI 抓图。
- 接收 RTSP 包/帧，或解码已由其他系统获取的 H.264/H.265 数据。
- 通过一套原生接口跨语言运行兼容的 ONNX 模型。
- 组合采集与推理，可在可信本地进程间共享帧。

设备配置、凭据存储、业务规则、持久化、报警与 UI 由宿主负责。本 SDK 不是完整的监控产品、浏览器播放器、视频编辑器或安全认证控制系统。

## Media SDK

- RTSP H.264/H.265/MJPEG 接入；编码包与解码帧订阅。
- 独立 H.264/H.265 解码，输入须为**完整 Annex B 访问单元**；任意网络分片不是完整帧输入。
- ISAPI HTTP Digest 抓图与有界连续采集；实际节拍取决于相机与网络。
- 拉取/回调接口、有界队列、元数据、缩放、像素转换与重连控制。
- 显式硬件解码探测与自动后端选择（D3D11VA 优先、映射可用时 DXVA2、有界 CPU 回退）；每 Context 工作线程额度与活槽/峰值统计。硬件回读不保证比 CPU 更快，以实测合同为准。
- Rust 高级 RTSP 入口 `Reader::open_with_budget`/`read_packet_with_budget` 可在 SDK 配置/包转换分配前预留工作区；`am_camera::media::codec::visit_access_units` 无需分配 NAL 列表即可检视 Annex B NAL 单元（仅在整体成功后提交调用方状态）。
- 显式端点流探测。Rust `CameraEndpoint` 生成免凭据 ISAPI/主/子流地址；该辅助不是独立的 C ABI 设备管理接口。
- 显式运行库目录与调用方注入凭据；不持久化凭据。

请核对运行时能力与合同。H.265 解码能力不代表浏览器可播 H.265。

## Vision SDK

- ONNX Runtime 集成、图像预处理与结构化结果。
- 五类**受约束模型档案**：Pose、Detect、Classify、Segment、OBB。
- 支持 dtype/shape 合同内自定义模型的原始命名张量输入/输出。
- 同步、异步与批量接口，状态、通知、确认与释放。
- **自适应攒批**（v0.2.0）：显式启用、有界、默认关闭；将动态批量轴模型 AUTO 语义下兼容项聚合为单次物理批次。固定批量轴模型明确拒绝，不填伪样本。
- CPU/OpenVINO/CUDA 选择：显式设备探测、允许列表、strict/fallback 策略与分层执行诊断。CPU 已在本仓验证；其他后端需另配依赖并验收。
- 跨语言安全像素输入：Python `owner.pixels().infer(...)` / C# `owner.Pixels().Submit(...)` 提交期间保持拥有者存活并加锁；异步输入默认复制，显式保留除外。
- Python/C# 模型通知缓冲有界（256 条、满时丢最旧）；丢弃后凭请求 ID 仍可查询结果。
- 可选共享帧，构建期编入并显式初始化启用。

本 SDK 不支持任意 YOLO 版本/导出。请对照[模型档案目录](crates/vision/resources/profiles.json)。模型权重及其许可另行获取。

## 源码构建

下载的 Windows 包内含 `bin/win-x64` 时，直接按[预编译包指南](docs/USE_PREBUILT.md)使用，无需编译。

Windows x64：Rust 1.90+、x64 MSVC 构建工具与 Windows SDK；首次构建拉取锁定的 Cargo 依赖。Node.js/Python 仅用于维护工具或 Python 示例，预编译 DLL 加载不需要。

在仓库根目录执行，构建输出保持在检出目录外：

```powershell
$env:CARGO_TARGET_DIR = (Join-Path $PWD '.local/build/current')
cargo build --workspace --locked
cargo build --release --locked -p am-camera-ffi -p am-vision-ffi --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames
```

Release 命令在 `.local/build/current/release` 输出 `am_camera.dll`、`am_vision.dll` 及导入库。历史文件名、`amc_*`/`amv_*` 符号与 crate 名保持兼容；不依赖原宿主应用。

FFmpeg 与 ONNX Runtime 动态加载；初始化时传入兼容的绝对运行库目录。构建成功不代表运行库可用。真实图像调用与依赖排障见[快速接入](docs/GETTING_STARTED.zh-CN.md)。

## 语言接入

| 语言 | 入口 |
| --- | --- |
| Rust | 本地依赖 `crates/am-camera` 与 `crates/vision`；未发布 crates.io |
| C / C++ | [Media 头文件](sdk/include/am_camera.h)、[Vision 头文件](sdk/include/am_vision.h)、[示例](sdk/examples/README.md) |
| C# | [绑定](sdk/bindings/csharp)、SafeHandle 辅助与示例 |
| Python | [绑定](sdk/bindings/python)、标准库 `ctypes` 与上下文管理器 |

Rust 专属硬件/性能示例在仓库 [`examples/`](examples/)；四语言接入示例在 [`sdk/examples/`](sdk/examples/)。

同一进程选择静态 Rust 消费或 DLL 消费之一，不混用两个运行域。Java 封装、Linux/macOS/移动端二进制与 GPU 推理验收不在首发 Windows 承诺内。

## 所有权、并发与故障

- SDK 分配经所属 SDK 释放，不用调用方 `free`/`delete`。
- 请求确认释放缓存责任，不释放已获取的结果句柄。
- 回调保持简短、保留包装引用、遵守重入限制。
- 队列/预算有界：处理拒绝、背压、过期帧与重连代次。不承诺无限 FPS、无限相机或无损交付。
- 关联视频与结果时保持帧身份/时间戳。展示用的预测不是新推理结果。
- 关闭 Context/卸载 SDK DLL 前排空子对象、回调与在途工作。底层 FFmpeg/ORT 可能常驻进程。

见[接入说明](sdk/INTEGRATION.md)与[ABI 合同](docs/design/dual-dll-abi.md)。

## 验证与边界

v0.2.0 Windows x64 **Release 候选**通过 **384 项 workspace Rust 测试**（20 项环境门槛忽略）、生成器链零漂移、ABI 冻结（11 工件）与来源指纹边界检查（239 文件）。干净包验证覆盖 C/C++17/C#/Python 四语言示例与攒批实测链（包内 DLL 端到端）。硬解矩阵（H.264/H.265，360p～4K 含 Main10）D3D11VA 实测通过并含软件对照；RTSP 故障矩阵（断流恢复、在途关闭）通过。五类模型中四类无本地权重未做端到端复跑；CUDA/OpenVINO 执行后端、现场流与长时验收如实登记未验。见 [v0.2.0 验证摘要](https://github.com/xiaoyanse/framebraid/releases/tag/v0.2.0)（VALIDATION.md 资产）与[交付状态](docs/RELEASE_STATUS.md)。

源码检出不含运行库、权重、凭据、业务库与宿主 UI。Windows 包附带匹配的 CPU 运行库及其源码/许可材料。提取清单为历史来源记录，不是持续更新的发布清单；核心文件保持指纹核查。

见[支持边界](docs/SUPPORT.md)、[当前验证状态](docs/RELEASE_STATUS.md)与[发布清单](docs/RELEASE_CHECKLIST.md)。历史计划不替代实际证据。

## 测试观察

![历史 SDK 重载稳定性](assets/benchmarks/reload-stability.png)

| 记录项 | 结果 | 范围 |
| --- | --- | --- |
| FrameBraid Rust 测试 | 384 通过；20 项环境门槛忽略 | v0.2.0 候选，Windows x64 Release |
| DLL/脚本验证组 | 干净包四语言 + 攒批实测链 | v0.2.0 包，CPU 与语言调用 |
| DLL 重载循环 | 30 次；全程 157 进程句柄 | 历史提取前 SDK |
| 预热后进程私有内存净增长 | 25 循环 0.69 MiB | 历史提取前 SDK |
| 队列压力/保留拥有者 | 37 接受、14 拒绝；37 保留/37 释放 | 历史合成 CPU 测试 |

历史数据不代表当前 Release 性能；单次硬件/二进制身份不全。不由此推导 FPS 或推理延迟结论。见[数据、队列图与方法](docs/BENCHMARKS.md)。

当前本机测量请用[可复现性能工具](docs/PERFORMANCE.md)。性能验收按 [MET-2](docs/PERFORMANCE_ACCEPTANCE.md)：CPU 卸载、有效交付、延迟与恢复分目标管理，历史判定见[复核记录](docs/verification/sdk-metrics-review-20261026.md)。

## 贡献与反馈

阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。勿张贴凭据、私有流地址或客户图像。安全反馈按 [SECURITY.md](SECURITY.md) 处理。变更见 [CHANGELOG.md](CHANGELOG.md)。

## 许可

Copyright 2026 [xiaoyanse](https://github.com/xiaoyanse)。

原始 SDK 代码采用 [Apache-2.0](LICENSE)，允许在遵守条款前提下商用与闭源使用。FFmpeg、ONNX Runtime、模型及其他依赖保留各自许可。见 [NOTICE](NOTICE) 与[第三方材料](sdk/third-party/README.md)。
