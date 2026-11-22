# Proposal: sdk-intel-direct（Intel 侧 GPU 帧直传——OV remote context 生产化）

## Why

Intel 平台（核显/无 NVIDIA）机器上 GPU 帧直传不可用：`AMV_CAP_GPU_FRAME_INPUT` 置位与 GpuPipeline CUDA 探测硬绑，OV 会话喂 GPU 帧必 notSupported，宿主只能走像素路径（CPU prepare 13.4ms 量级）。OV 原型批（.local/evidence/sdk-ovproto-20261116，Arc 140T 实跑）已实证 remote 通路：D3D11 NV12→VA_SHARED/VA_SURFACE remote 张量→OV preprocess→infer 零错误码、正确性三证、前处理 13.4ms→~1.4ms 边际、端到端 ≈3.1×（对 OV CPU 线 ≈5.9×）。合同 L774-775 引擎适配位条款（"OpenVINO 原生引擎落定后按本输入合同适配，接口不变"）为本批兑现预告。

排期：协调线 2026-11-19 排期令（不等联测反馈，联测反馈到达后自然并入验证段）。

## What Changes

### 合同（ABI 次版本 1.7→1.8 兼容新增；先合同后生成链）

1. **能力位 bit10 `AMV_CAP_GPU_FRAME_INPUT` 语义扩展**（PM 裁决 1，参照 GPU_CASCADE 先例口径 L1063）：从"当前运行库 CUDA 互操作入口可用"扩展为"任一互操作入口可用（CUDA 运行库或 OV remote context：openvino_c 可载且 GPU 设备在域）"；实际可用性接受时按会话后端校验（notSupported 显式拒绝不静默降级，既有 L770 条款不变）。OV 侧探测不触发推理编译（对应 L783"查询不触发初始化"的 OV 域表述；core 枚举 OnceLock 进程级缓存）。
2. **§3.3a 前处理条款增补**（PM 裁决 2）：几何/归一化/裁剪语义与像素路径同档案同规则不变；NV12→RGB 色彩转换 GPU 域注记——CUDA 与 OV 直传均按引擎互操作实现 BT.601 系（与像素路径 BT.709 limited 存在已知色域差，CUDA 线既有先例）；验收容差分级：CPU 参考输出位级口径（conf ≤1e-3）与 GPU 域统计口径（top conf ≤5e-2，sdk-ovgpu 先例）分档写明。OV preprocess 构造陷阱（scale 除法语义/pad 归一化前 114）作为实现注记入合同附录级说明。
3. **设备域条款 OV 侧扩展**：OV 会话定向的 GPU 设备与 D3D11 纹理所属适配器须 vendor 一致（复用 §3.5b 厂商核对词机制），不一致显式拒绝。
4. **审计口径**：gpu_inputs/input_copies_per_frame/input_cpu_fallbacks 对 OV remote 路径同样适用（remote 臂零 host 上传=copies 0 的口径写明）。

### 实现（vision 侧窄批；PM 裁决 3）

- **ov_sys.rs 新绑定**：ov_core_create_context / ov_core_compile_model_with_context / remote tensor 族（ov_tensor_create_remote 等，原型 L30-34 已核属性键与序数）约 6-10 符号，四版本线核对。
- **OV 引擎 remote 路径**：GPU 帧输入在 OV GPU 会话的处理链——remote context 创建（VA_SHARED/VA_DEVICE=D3D11 设备）+适配器一致性校验+NV12 双平面 remote 张量+preprocess 图构造（两坑规避）+compile_model_with_context+绑定推理。
- **能力位置位点**：vision-ffi lib.rs bit10 条件扩展（CUDA 探测 OR OV remote 探测）。
- **生成链**：constants.json abiMinor 7→8+四生成物同步（头文件冻结工件归因重算）。

### 范围外（明确不做）

- 相机句柄族接线（amc_gpu_frame_* 七导出+frame_kind 订阅，issue #3 主体）——CUDA/OV 共用面，独立批。
- DML 直传、级联 GPU 路径 OV 化（级联 §GPU 帧路径维持 CUDA:0 既有约束）。
- OV preprocess 生产级 resample 内核替换（resample_ref__u8 标量内核，后续优化项登记）。

## Impact

- 门禁 **L3**（跨模块协议+能力位/次版本+公开合同；archive-gate-levels 客观触发：跨模块协议+公共 API 契约）。
- 触碰：docs/design/dual-dll-abi.md（+types）、sdk/abi/constants.json、生成物四件、crates/vision/src/engine/{openvino.rs,ov_sys.rs}、crates/vision-ffi/src/{lib.rs,inference/input.rs}、测试与 e2e、CHANGELOG/INTEGRATION/MIGRATION/RELEASE_STATUS/ROADMAP、boundary 登记。
- 下游：兼容新增——旧绑定（1.7 及以下 init）行为零变化（bit10 置位条件变化仅使新机器上"更多置位"，notSupported 兜底不变）；vl pin v0.6.3 不受影响。
- 验收锚：D3 参考输出+对照器（GPU 域 5e-2 统计档）+原型对拍口径复用（bus=5/zidane=3 身份零漂移+top conf 门）。
