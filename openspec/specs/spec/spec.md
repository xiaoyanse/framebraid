# spec Specification

## Purpose
TBD - created by archiving change sdk-v1-consumer-validation. Update Purpose after archive.

## Requirements

### Requirement: 消费方基线门禁

发布流程 SHALL 含替代方案基线对比（解码 vs OpenCV/节拍精度/多路线性度），判据自动化（同输入指纹+P50/P95 口径）；基线漂移无豁免记录则阻断发布。

#### Scenario: 基线漂移阻断
- **WHEN** 发版检查跑基线脚本且对比项超出记录阈值
- **THEN** 发布被阻断，须修复或登记豁免理由后方可继续

### Requirement: 场景测试库

SDK SHALL 维护场景测试库（scripts/sdk/scenarios/），每个已修的消费方问题沉淀一个防回归场景（合成环境可跑，不依赖真实相机）；新修复按类型入库。

#### Scenario: R 系列防回归
- **WHEN** 场景库随发版检查运行
- **THEN** R1-R9 类型问题（预算边界/节拍/解码性能/流协议/可观测/绑定装载）各有至少一个场景测试守护

### Requirement: Intel 核显路径实测完备

SDK 在 Intel 核显平台（无独显）SHALL 提供完整可用路径：硬解（核显 D3D11VA）、推理（DirectML 核显 / OpenVINO 原生 CPU）、媒体软解全链；支持矩阵按实测如实标注。

#### Scenario: 核显硬解
- **WHEN** Intel 核显机器上以 PreferHardware 解码 H.264/H.265
- **THEN** 实际后端为核显 D3D11VA，帧交付与故障恢复语义与 NVIDIA 路径一致

#### Scenario: 无 GPU 推理回退
- **WHEN** GPU 推理路径不可用（无独显/驱动缺失）
- **THEN** 回退 CPU/OpenVINO 原生全链可用，回退可观测（selection 报告）

### Requirement: 帧编码输出

SDK SHALL 提供像素帧→H.264 编码能力（码率/GOP/分辨率可配），编码包语义与既有 am_encoded_packet_t 一致（引用计数/retain-release）。

#### Scenario: 编码产出
- **WHEN** 像素帧序列送入编码会话
- **THEN** 产出 H.264 编码包（关键帧间隔符合配置），会话关闭排空缓冲包

### Requirement: MP4 存档

SDK SHALL 提供 MP4 文件存档会话（open→write→close 三段式），分段时长与关键帧可配；产出文件可被标准播放器/ffprobe 解析。

#### Scenario: 存档可播放
- **WHEN** 编码会话写入 N 帧后正常关闭
- **THEN** MP4 文件可 ffprobe 解析且流参数与配置一致

### Requirement: RTMP 推流

SDK SHALL 提供 RTMP 推流会话（断线按既有退避策略重连），推流状态与故障可观测（复用源状态通道语义）。

#### Scenario: 推流与重连
- **WHEN** 推流中网络断开再恢复
- **THEN** 按退避策略重连后续推，故障期间状态可观测且缓冲策略有界

### Requirement: 模型热切换

SDK SHALL 提供 `amv_model_swap`：按槽位（B11 slot_key）原子替换运行中模型，在途请求由旧模型完成后释放，新请求即时路由新模型；消费方句柄与槽位身份不变。

#### Scenario: 换模瞬间并发请求
- **WHEN** 槽位存在在途请求时调用 swap
- **THEN** 在途请求由旧模型完成且结果不丢，swap 后新请求路由新模型，全程无请求失败

#### Scenario: 排空释放
- **WHEN** 被换出模型的所有在途请求完成
- **THEN** 模型按既有 close 语义释放（显式 close 或 DETACH 立即排空），selection 置 legacy_model

### Requirement: OpenVINO 原生引擎

Vision SDK SHALL 支持以 OpenVINO 原生 C API（ov_core 直连）作为与 ORT 并列的推理引擎，经 AM_BACKEND 新枚举值选择，五档案（pose/detect/classify/segment/obb）输入输出语义与 ORT 路径一致。

#### Scenario: 原生引擎推理
- **WHEN** 模型以 openvino-native 后端加载且 openvino.dll 在 runtime_dirs
- **THEN** 推理经 ov_core 直连执行，输出与 ORT CPU 同模型同输入在容差内一致

#### Scenario: 回退矩阵
- **WHEN** openvino.dll 缺失/模型格式不支持/设备不可用
- **THEN** 按既有多后端 selection 回退 ORT CPU，回退原因可观测（不静默）

### Requirement: 多引擎抽象

session 层 SHALL 以引擎抽象并列 ORT 与 OpenVINO 原生两条路径，ORT 现有行为零改动；GPU 帧输入（M2b §3.3a）的引擎适配位对两引擎统一。

#### Scenario: ORT 路径不受影响
- **WHEN** 以既有 backend 值加载模型
- **THEN** 行为与 E1 之前完全一致（selection/回退/统计）

### Requirement: MJPEG 流源类型

SDK SHALL 支持以 `AM_SRC_MJPEG_URL` 打开 HTTP multipart/x-mixed-replace 流源，boundary 分帧为唯一真相源（Content-Length 不参与分帧，谎报仅计数），JPEG 帧进入既有解码与像素订阅管线。

#### Scenario: 正常流逐帧送达
- **WHEN** 流源打开且服务器按 boundary 分帧推送 JPEG
- **THEN** 帧订阅按既有语义收到解码后像素帧，EXIF/预算/统计与 ISAPI 快照一致

#### Scenario: 恶意流有界防护
- **WHEN** boundary 超 70B、part 头超 64KB 或单帧超 8MB
- **THEN** 丢帧并计数，连续 3 次超限断开连接进入重连退避

### Requirement: 防误切分帧

解析器 SHALL 仅在行首 `\r\n--boundary` 上下文处分帧，并校验 part 的 Content-Type=image/jpeg 与 JPEG SOI/EOI 魔数；载荷内出现 boundary 字节序列不误切。

#### Scenario: 载荷含伪 boundary
- **WHEN** JPEG 载荷字节序列包含 boundary 字符串（非行首上下文）
- **THEN** 不切帧，该载荷完整交付解码

### Requirement: 双超时断流判定

流读取器 SHALL 区分无数据（对端黑洞）与无 boundary 进展（恶意无界流）两种超时，超时后断连重连（复用既有退避策略）。

#### Scenario: 无数据超时
- **WHEN** 连接建立后持续无字节到达超过约定时限
- **THEN** 断开重连，超时原因可观测

### Requirement: Vision GPU 帧输入

推理输入 SHALL 支持同进程 GPU 帧直传（纹理指针，借用期内有效）；frame_kind 协商失败或 GPU 帧不可用时 SHALL 回退 CPU 像素输入且不失败。

#### Scenario: GPU 帧直传推理
- **WHEN** 模型支持 GPU 输入且调用方传入 M2a GPU 帧句柄（同进程）
- **THEN** 推理经 CUDA 互操作与 GPU 预处理直达 IOBinding，全程零 CPU 回读

#### Scenario: 回退矩阵
- **WHEN** 无 CUDA 引擎/SOFTWARE_ONLY/GPU 帧句柄无效
- **THEN** 按 CPU 像素路径执行并计数回退，不失败整个请求

### Requirement: CUDA 互操作纹理生命周期

纹理注册（cudaGraphicsD3D11RegisterResource）SHALL 在 GPU 帧借用期内完成注册与 unmap；归还/释放前 SHALL unmap 注销，不跨租约持有纹理。

#### Scenario: 借用期内注册与注销
- **WHEN** GPU 帧发布→消费→归还
- **THEN** 注册/unmap 全程发生在租约窗口内，归还后无残留注册

### Requirement: 输入搬运审计

GPU 输入路径 SHALL 计数每帧拷贝次数与字节数（copies_per_frame/bytes_transferred），CPU 回退路径单列；数字经统计接口可读（M2c 验收主指标）。

#### Scenario: GPU 路径搬运计数
- **WHEN** 一帧经 GPU 直传推理完成
- **THEN** 搬运计数反映实际 GPU 内拷贝次数（目标 0 次 CPU 回读）

### Requirement: GPU 帧池创建与租约

SDK SHALL 提供 GPU 帧池的创建/借出/归还/销毁导出，语义（有界/租约/关闭排空）与既有共享帧池一致，按显存字节新类目计费。

#### Scenario: 池满拒绝
- **WHEN** GPU 帧池帧数达上限且有借出未还
- **THEN** 新发布被拒绝且不覆写活帧

### Requirement: GPU 帧订阅

像素订阅 SHALL 支持 GPU 帧交付模式（frame_kind 追加字段，struct_size 协商）；SOFTWARE_ONLY 策略下 GPU 订阅按合同报不支持，不静默降级。

#### Scenario: 硬解激活时 GPU 订阅交付
- **WHEN** PREFER_HW 实际硬解且订阅 frame_kind=GPU
- **THEN** 拉取返回 GPU 帧句柄（含元数据与共享 NT 句柄）

### Requirement: 搬运审计挂钩

GPU 帧路径 SHALL 预留搬运计数器（gpu_frames_delivered/cpu_fallbacks/bytes_transferred_est），经统计接口可读。

#### Scenario: 回退路径计数
- **WHEN** 纹理缺共享标志降级回读
- **THEN** 元数据标注 fallback 且 cpu_fallbacks 递增

### Requirement: AM_BACKEND_DIRECTML 枚举值

`am_backend_e` SHALL 包含 `AM_BACKEND_DIRECTML = 3`，作为 Windows DirectML 执行后端的标识。

#### Scenario: 枚举追加不破坏旧值
- **WHEN** 使用 SDK 1.0/1.1 头文件的调用方检查 CPU/OpenVINO/CUDA 值
- **THEN** 三值语义与位不变，DirectML 为纯追加

### Requirement: 后端矩阵第四行

`amv_query_backends` 矩阵 SHALL 为 DirectML 提供独立行，字段语义与其他行一致（hw_detected/deps_present/provider_loadable/verified）。

#### Scenario: 有运行库与核显
- **WHEN** 运行库目录含 DirectML.dll 且系统枚举到 DirectML 适配器
- **THEN** hw_detected=1, deps_present=1；策略选择后 loadable/verified 置位

#### Scenario: 无 DirectML 运行库
- **WHEN** 运行库目录与系统均无 DirectML.dll
- **THEN** hw_detected=0, deps_present=0, 后续字段不置位

### Requirement: 策略选择与回退

`am_backend_selection_t` 候选列表 SHALL 接受 DirectML 候选；device_id 字段对 DirectML 忽略（合同注明）。

#### Scenario: DirectML 首选允许回退
- **WHEN** 候选=[(DIRECTML,0),(CPU,0)]，allow_fallback=1，DirectML 会话创建失败
- **THEN** 模型加载成功且回退原因可查询，实际执行为 CPU
