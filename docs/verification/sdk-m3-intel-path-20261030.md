# M3 Intel 路径实测验证记录（2026-10-30）

范围：OpenSpec `sdk-m3-intel-path` M3.1～M3.4 的实测与如实登记。本机双 GPU（NVIDIA RTX 5060 Laptop 10de:2d19 + Intel Arc 140T 8086:7d51），Windows 11 Pro 10.0.26300（25H2）。

## M3.1 Intel 核显硬解（全部实测通过）

测试：`crates/am-camera/src/media/codec/selection.rs::tests::intel_igpu_d3d11va_decode_and_fallback_measured`（`#[ignore]`，`AM_SDK_FFMPEG_DIR` + `AM_SDK_CODEC_SAMPLES` 显式运行）。

实测机制：向 `create_with_adapter_source` 注入仅含 Intel 适配器的源（vendor 0x8086 定向），走真实选择→设备初始化→LUID 身份校验→解码→帧交付全链；另跑默认策略与故障注入对照。

| 检查 | 结果 |
| --- | --- |
| 适配器清点 | index 0 = RTX 5060（8.28GB）；index 1～4 = Arc 140T（同设备 4 个 DXGI 实例）；index 5 = WARP |
| 默认策略实测 | 选中 index 0 NVIDIA（477ms，attempts=1）——枚举序首个成功即返回，核显不会被默认策略命中 |
| Intel H.264 256x144 | D3D11VA，NV12（fmt=103），subresource=16，选择 142ms，attempts=1 |
| Intel H.264 1920x1080 | D3D11VA，NV12，选择 45ms |
| Intel H.265 256x144 | D3D11VA，NV12，选择 42ms |
| FFmpeg 驱动日志 | `Using device 8086:7d51 (Intel(R) Arc(TM) 140T GPU (16GB))`（三腿均实证） |
| GPU 帧交付 | `gpu_frame_info` + `gpu_texture` 非空；shared NT 句柄=0（与 M2a NVIDIA 同款平台限制，非回退） |
| 幽灵适配器故障注入 | index=8（不存在）→ D3D11VA 腿 `AdapterIdentityMismatch`（FFmpeg 回退默认设备被 LUID 校验抓到）+ Dxva2 腿 `AdapterMappingUnavailable`；回退 CPU 真实出帧 256x144 |

结论：Intel 核显 D3D11VA 硬解链路可用；R2 时代 rtsp-reconfigure 记录（[重建记录](sdk-r2-rtsp-reconfigure-20261026.md)）与本轮互相印证。

## M3.2 Intel 平台推理

- OpenVINO 原生（CPU 设备）：E1 已收口（[E1 收口记录](../../.local/evidence/sdk-e1-closeout-20261029.md)），本轮不重跑全量；E1 回退矩阵在当前 main 复测通过（见 M3.3）。
- DirectML（核显）：**本轮未实测，运行库缺失**。E2 用的 `ortdml-rt`（onnxruntime-directml 1.24.4）在归档清理中移除；当前盘上三个 onnxruntime（ort-gpu-131-final / ort-ov-124-final / ort-win-gpu）符号检查均无 `OrtSessionOptionsAppendExecutionProvider_DML`（CUDA/OV 符号对照齐全，排除探测方法误差）。恢复路径：重新获取 onnxruntime-directml 1.24 线 wheel 解包到 `.local/runtimes/`，再以 `AM_RT_ORT_LIBRARY` 指向运行（E2 流程，[E2 记录](sdk-e2-directml-20261028.md)）。
- 设备选择边界（合同级，代码实证）：
  - ort 2.0-rc.13 `ep::DirectML::with_device_id(i32)` 存在（映射 `SessionOptionsAppendExecutionProvider_DML(device_id)`）——ort 层能力在。
  - 生产 `directml_ep()`（vision lib.rs）未设 device_id → DML 用 EP 内部默认选卡（本机枚举序 NVIDIA 在前，默认设备不保证是核显）。
  - SDK 合同 backend=3 拒绝 device_id≠0（sdk/selection.rs 校验）——**公共接口无核显定向能力**，与 M3.1 适配器定向缺口同类。
  - OpenVINO EP（ORT 线）维持 M1 阻塞结论（Intel provider wheel 公开链符号缺失，[blocker 记录](sdk-gpu-m1-openvino-blocker-20261028.md)）。

## M3.3 回退矩阵（全档核对+补实测）

| 档位 | 结果 | 证据 |
| --- | --- | --- |
| OpenVINO 原生目录缺失/未设 → ORT CPU | 复测通过：attempts=2（runtime 阶段 backendStrictUnavailable + session_created 成功），真实推理 native_runs=1 delivered_runs=1（191ms/73ms），strict=true 正确拒绝回退 | `engine::openvino::tests::fallback_matrix`（E1 用例当前 main 复跑） |
| CUDA 不可用（CPU-only 运行库）→ CPU | 通过：CUDA 腿 compiled_provider 阶段失败 33µs → CPU session 51.9ms，native_runs=2 delivered_runs=1，公开字段篡改不污染执行证据 | `sdk::selection_tests::selected_models_prove_cpu_fallback_strict_and_execution_not_public_fields`（AM_SDK_ORT_LIBRARY=ort-ov-124-final，CPU-only 实证：该 DLL 无 CUDA 符号） |
| 硬解失败回软解 | 通过：幽灵适配器注入 → 两级错误记录（AdapterIdentityMismatch / AdapterMappingUnavailable）→ CPU 解码真实出帧 | 本轮 M3.1 测试故障腿 + [R2 故障矩阵](sdk-r2-h23-fault-matrix-20261027.md) |
| GPU 帧不可用回像素回读 | 沿用 R2 CPU RGBA 回读合同证据，未另测 | [单路对照](sdk-r2-current-s1-20261027.md) |

## 门禁

- `cargo fmt -p am-camera` 通过；`cargo clippy -p am-camera --all-targets` 0 警告
- `cargo test -p am-camera --lib`：71 passed / 0 failed（12 ignored 为环境门槛用例）
- 本轮实测 3 项 ignored 用例显式运行全绿（M3.1 硬解链 + E1 回退矩阵 + CUDA→CPU 选择）

## 未验边界（如实）

- DirectML 真实推理（运行库缺失，恢复后按 E2 流程补测）
- DirectML 核显定向（ort 能力在、SDK 合同未暴露；即使运行库恢复，公共接口仍只能默认选卡）
- OpenVINO EP（ORT 线）GPU 设备（维持 M1 阻塞：Intel provider wheel 链接符号公开渠道不可得）
- Intel 核显解码的池审计（GPU 帧池在 camera-ffi 内部，公共选择接口无适配器定向入口；池审计由 M2a NVIDIA 实测覆盖，池本身设备无关）
- Intel 核显解码性能对照（未测，属 MET-2 量化范畴）
