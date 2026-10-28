# 双 DLL C ABI 接口设计（1.0历史冻结与1.1本地扩展）

> FrameBraid source reference: historical host reports/OpenSpec are not included. For actual validation and current support, see [documentation](../README.md). Historical status text is preserved; this note does not change ABI semantics.

状态：**1.0实施冻结**——2026-10-25用户授权收口并继续DLL/共享帧实施；当前定稿以[冻结记录](dual-sdk-abi-freeze-v1.md)和`sdk/abi/freeze-v1.json`为准。本文保留各轮历史整改和原候选讨论，历史“待回审/候选”不再覆盖上述当前定稿。冻结是设计状态，不等于121个入口已实现或SDK-G0～G3通过。

本地R2新增ABI 1.1，以[兼容扩展冻结依据](dual-sdk-abi-freeze-v1.1.md)和当前freeze-v1.json为准；1.0正文与历史快照保留。JSON字段约束也纳入冻结：`sdk/abi/decode-hardware-report-v1.schema.json`。源码新增不等于已发布，旧v0.1.0制品仍为ABI 1.0。
依据（冲突时以此顺序为准）：camera-sdk (historical host reference, not included: `../../openspec/changes/media-v2-h264-unified-player/specs/camera-sdk/spec.md`)、inference-sdk (historical host reference, not included: `../../openspec/changes/media-v2-h264-unified-player/specs/inference-sdk/spec.md`)、native-sdk-contract (historical host reference, not included: `../../openspec/changes/media-v2-h264-unified-player/specs/native-sdk-contract/spec.md`) 三份 delta spec > [dual-dll-sdk.md](dual-dll-sdk.md) > 本文。本文不勾选任何实施任务。

**交付主体口径（2026-10-25 用户修正）**：交付物是可脱离 AM-RT 独立构建的**源码 SDK**（am-camera+am-vision+内部纯函数依赖+独立构建清单）；本文的 Windows x64 DLL 只是编译产物之一，非 Rust 宿主经本 C ABI 消费。首版验收仍 Windows x64；其他平台单列为「首版设计、后续平台实施」（不默认宣称编译或运行已通过，与 native spec 口径一致），不扩大首版。

首版平台：Windows x64。性能口径：实测完整调用路径，不承诺零拷贝/纳秒级。

R2本地Rust兼容新增的设备探测/显式硬解合同见[native hardware](native-hardware.md)。不改变本文1.0 C ABI，直接解码/RTSP的自动选择及诊断导出仍须按H2后续任务完成；设备探测成功不能作为C接口已硬解的证明。

本地H2.3a已将既有PREFER_HW策略接入直接C解码器，新增实现语义与恢复约束见上述硬件合同及[验证](../verification/sdk-r2-direct-auto-20261026.md)。布局、枚举和1.0冻结未变；get_info的NULL未定/实际后端语义沿用附录，RTSP基本接线r7已完成，完整设备/耐压诊断仍未完成，不扩张旧公开制品承诺。

H5本地Rust核心选择/状态合同见[inference selection](inference-selection.md)；本阶段无新增C导出或结构，旧C模型入口沿用原策略。C策略、显式推理探测与缓存报告后续先扩合同再实施，不能用Rust核心完成替代跨语言交付。

## 0. 范围与读法

- 两个 DLL：`am_camera.dll`（视频 SDK）、`am_vision.dll`（推理 SDK），同一进程内加载，各自独立初始化；本文给出 C 头文件级设计：句柄体系、全部导出函数签名、公共合同、错误码、资源预算与规格对照矩阵。
- **首版范围口径（B01，与三 specs 统一）**：H.264/**H.265** 接入与解码（SDK-DEC-01）、**独立压缩数据解码入口**（SDK-DEC-02/N01，§2.8）、ISAPI **单次+连续取图**（SDK-DEC-04/N02，§2.7）、YOLO **五类**结构化结果（SDK-DEC-03，§3.6/§4）、C/C++/C#/Python 调用方验证（SDK-DEC-05）。SDK 的 H.265 能力与浏览器播放承诺分层验收（§10.1）。
- 首版内置 YOLO 全家族五类业务适配（Pose/Detect/Classify/Segment/OBB）；明确不进核心：PTZ/音频/录像检索回放/GB28181/ONVIF/网络转发/业务报警/模型训练（6.5 候选目录），接口不以空占位冒充。
- **结构定义分工**：本文正文给出函数签名、调用流程与合同语义；全部公开结构的完整字段定义、布局规则与常量数值表收敛在附录 [dual-dll-abi-types.md](dual-dll-abi-types.md)（B09 权威蓝本）。正文与附录冲突时以附录为准。

### 0.1 终审整改对照索引（B/N → 落点）

| 编号 | 落点 | 编号 | 落点 |
|---|---|---|---|
| B01 全文口径统一 | §0/§2.1/§2.5/§4/§8/§9 | B09 结构布局与完整定义 | §1.4 + 附录全文 |
| B02 五类结果结构 | §3.6 + 附录 §3 | B10 预算归属与双限 | §6 + 附录 §4 |
| B03 异步闭环+请求快照 | §2.7/§2.10/§3.6/§5.4 | B11 实时槽公平+批量回执 | §3.5/§3.7 |
| B04 连接前认证流程 | §2.2/§2.4 | B12 帧身份与关联 | §2.5/§2.6/§3.3/§5.6 + 附录 §2 |
| B05 raw 模型加载 | §3.1/§3.2 | B13 认证终态与故障分型 | §2.4/§2.9/§7 |
| B06 同步超时语义 | §3.3/§3.4/§5.1 | B14 ORT 共存与后端四层 | §3.1/§3.2 |
| B07 关闭三段式 | §1.1/§5.3/§5.4 | B15 依赖闭包清单 | §10.2 |
| B08 retain/release 统一 | §3.6/§5.1/§5.6 | N01 独立解码器 | §2.8 + 附录 §2.6 |
| N02 连续 ISAPI | §2.7 + 附录 §2.7 | N03 能力归属分层 | §10.1 |

### 0.2 回审整改对照索引（P1～P9 → 落点，2026-10-25 第二轮）

| 编号 | 问题域 | 落点 |
|---|---|---|
| P1 关闭与销毁两阶段 | §1.1/§1.2/§5.4（am?_object_destroy；CLOSED 句柄在册、destroy 后不可用） |
| P2 嵌套布局与命名统一 | §1.3/§1.4 + 附录 §1.3/§3.4/§3.5（可演进结构指针化；am_encoded_packet_t 规范名） |
| P3 图像 retain 与错误快照边界 | §2.7（amc_image_retain/release）/§5.1/§5.3（白名单补 retain）/§1.2+附录 §1.2（am_init_error_t） |
| P4 共享执行器与线程预算 | §1.2/§6（执行器进程单例同账本；total_thread_budget+threads 统计） |
| P5 视图布局与时间字段 | §2.6/§3.3 + 附录 §2.5/§3.3（plane_size_bytes/color_matrix/color_range/received_at_ms/clock_quality/时间原点） |
| P6 批量三值与槽全覆盖 | §3.4（张量带 params）/§3.5（NATIVE/SPLIT/AUTO+逐项 slot_key/raw_preprocess）/§3.7 |
| P7 独立解码器配置出口 | §2.1（BSF 能力位）/§2.8 + 附录 §2.6（output_pixel_format/hw_policy/get_info/RESET_PENDING） |
| P8 五类几何合同定死 | §3.6 + 附录 §3.5（Segment 位序 LSB-first；OBB 角度/顶点序全局约定）/§4.2（冻结样本） |
| P9 后端恢复与统计语义 | §3.1（amv_backend_retry；subgraph_cpu_nodes=-1；无 TRT 枚举）+ 附录 §3.1/§3.2 |

### 0.3 三轮整改对照索引（六组遗留 → 落点，2026-10-25 第三轮）

| 编号 | 问题域 | 落点 |
|---|---|---|
| P1 残留 | destroy 隔离区容量与回收边界 | §1.1/§5.4 + 附录 §6（隔离区上限 4096/库，满后最老地址复用；destroy 后使用=UB 显式条款） |
| P2 残留 | am_pixel_view_t 按值嵌入可演进的 am_frame_assoc_t | §5.6（assoc 字段指针化） |
| P3 残留 | init 单槽与请求快照字符串有效期 | §1.2 + 附录 §1.2（am_init_error_t 内嵌 char message[256] 固定缓冲）；§2.10/§3.7/§5.4 + 附录 §1.3（请求快照改句柄制：TTL 内可重复读、已移交句柄至 release 有效、出账后 query 返回 resultExpired） |
| P4 未通过 | 共享执行器无真实装配方式 | §1.2/§6 + 附录 §1.1（首版限定一个进程一种消费方式：宿主静态链接或 DLL 二选一，禁止并存；进程内原子标志检测，冲突返回 sdk.domainConflict）（四轮修订：原子标志检测降为部署约束+尽力检测，见 §0.4） |
| P6 未通过 | 固定 batch>1 尾项 SPLIT 违反模型输入形状 | §3.5 + 附录 §3.2（尾项不足完整批次→明确拒绝，逐项回执 vision.insufficientBatchRejected；填充组批列后续候选） |
| P8 未通过 | 分类输出实为概率/OBB 首顶点不唯一/导出参数未定死 | §4.2（classify-yolo11n-prob-v1+SHA+Softmax 节点实证；四类导出参数全量定死）；§3.6 + 附录 §3.5（OBB 首顶点公式定义+非正方形数值算例） |
| 映射表 | "缺口=0"口径失实 | §8 缺口判定改为"设计映射齐全；本轮修订项待复核确认"，矩阵未关闭 |
| 回审材料 | 三轮整改未登记 | verification-dual-sdk-remediation-20261025.md (historical host reference, not included: `../verification/verification-dual-sdk-remediation-20261025.md`) 追加三轮段（提交号由 PM 收编时填） |

### 0.4 四轮收口对照索引（三处残留 → 落点，2026-10-25 第四轮）

| 编号 | 问题域 | 落点 |
|---|---|---|
| P3 残留 | 快照释放归属（公共 am_ 版本两库同释，与「分配方释放」铁律冲突） | §2.10/§3.7/§5.1/§5.4/§8.3 + 附录 §1.3（快照释放拆分 `amc_request_snapshot_release`/`amv_request_snapshot_release`，句柄必须回分配方库，跨库=handleInvalid；公共 am_ 版本删除） |
| P4 残留 | 跨模块检测缺共同登记点（宿主与 DLL 各编译一份静态原子变量无法互检） | §1.2/§6/§7.1/§8.3 + 附录 §1.1/§6.2（单选降为**部署约束**（部署方保证二选一）；SDK 尽力检测（同名符号/环境探测等实现期探索）不承诺自动互检；并存超限按各自预算拒绝不视为 SDK 缺陷；删除「进程级原子标志检测」承诺）+ native-sdk-contract spec「总资源预算」Requirement/Scenario 同步修订（并存=违反部署约束，行为不在承诺范围） |
| P8 残留 | ①角度方向文字（逆时针）与公式（u=(cosθ,sinθ)，y-down 系视觉顺时针）冲突；②档案仅导出参数+SHA，缺前处理/NMS/掩膜顺序/参考结果与容差 | ①§3.6 + 附录 §3.5（文字统一改「图像坐标系（y 向下）视觉顺时针为正」，注明公式本身即数学正角；公式与数值算例不动）；②§4.2（classify 前处理按 ultralytics 8.4.163 pipeline 定死；detect/segment NMS 默认 conf=0.25/iou=0.7/max_det=300+可覆盖；segment 掩膜合成顺序写死；obb 旋转 IoU=probiou 或等价；参考结果+容差表结构，实现期生成入库）+ 附录 §3.2（am_infer_params_t 增 nms_iou_threshold/max_det 覆盖字段） |

### 0.5 共享帧池增补对照索引（SDK-DEC-07 → 落点，2026-10-25 终审 §11 增补）

| 落点 | 内容 |
|---|---|
| §5.7（新增）+ 附录 §1.6/§4/§6.1 | 共享帧池接口族（SDK-DEC-07，两库对称导出）：统一帧描述/池创建/发布/只读借用/归还/关闭/统计、生命周期合同、释放入口分库、上限维度与候选值 |
| §2.1/§3.1 + 附录 §2.1/§3.1 | 能力位 AMC_CAP_SHARED_FRAME_POOL / AMV_CAP_SHARED_FRAME_POOL 与「未启用/未编入/平台不支持」三态查询 |
| §4.2 | 共享帧输入的前处理与普通视图一致（不因传输方式改变前处理语义） |
| §5.6 尾段 / §6 表 / §7.1 / §9-O11 | CPU帧池首版/GPU共享后续分层；本轮O11配置与安全合同已细化，见共享帧专项合同；其他冻结项另核 |
| §8.1/§8.2/§8.3 | 映射矩阵新增共享帧池三行（camera 发布侧 / inference 消费侧 / native 公共能力，31→34 Requirement） |

### 0.6 实施工件与冻结状态（2026-10-25）

用户已授权审计方修正后接手开发。公开头文件见 `sdk/include/am_camera.h`、`am_vision.h`、`am_sdk_types.h`；`scripts/sdk/generate-abi.mjs`从本文函数签名和附录完整类型生成，`sdk/abi/constants.json`补齐功能与能力位数值。`am-sdk-abi`以同一蓝本生成Rust值类型；Windows x64的C编译器实测布局见`sdk/abi/layout-win-x64.json`，字段偏移由547项布局断言锁定。

这些工件现按1.0实施冻结记录收口；121个入口实现和SDK-G0～G3仍分别待验，不把6.1设计完成扩大为整包交付。生成器`--check`用于发现正文、头文件和Rust值类型漂移，不能代替功能测试。

## 1. 总体结构与 ABI 版本协商

### 1.1 扩展：原生解码探测与拥有者快照（R2/H2，ABI 1.1 本地开发）

新增兼容导出，不改变既有结构布局、枚举、内存所有权或函数签名；旧1.0冻结记录保留，新冻结链引用旧记录。两库报告共享ABI 1.1，新实现接受1.0/1.1初始化，拒绝更高次版本或不同主版本；旧实现拒绝1.1初始化。调用方先查实际ABI，再使用新导出，不能以在线文档猜旧DLL能力。

```c
/* 显式且同步的有限初始化探测；codec仅H264/H265，预算1..5000ms。
   取得一个decoder许可并进入共用单轮选择准入，探测后释放原生设备/codec。
   不读码流、不证明真实硬解，调用间检查软预算、不强杀驱动；回调内拒绝重入。
   依赖缺失返回明确错误；设备不可用时保存带逐项原因的CPU选择报告并成功返回。 */
AM_API am_status_t amc_probe_decode_hardware(amc_context_t* ctx,
                                            uint32_t codec, uint32_t budget_ms);
/* object为context、独立decoder或source；NULL缓冲+capacity=0查询所需字节（含NUL）。
   返回UTF8 JSON schema=framebraid.decode-hardware/1，最大64KiB。
   容量不足写required，buffer首字节置0并返回sdk.limitOutOfRange；不返回半份JSON。
   快照只复制已有状态，不探测、不创建会话；字符串由调用方缓冲拥有，属于回调查询白名单。
   context未显式probe时state=not_probed；decoder关闭后仍可查最后观察结果。
   source关闭返回closed和空执行状态；不泄露其他共享拥有者后续活动。 */
AM_API am_status_t amc_get_decode_hardware_report(void* object, char* buffer_utf8,
                                                 uint64_t capacity, uint64_t* required);
/* RTSP源的共享连接策略；单拥有者、首次像素订阅前可变；同值幂等。
   策略0=PREFER_HW/1=SOFTWARE_ONLY；共享或冻结后变更=modeConflict。
   不启动探测，ISAPI源=modeConflict，回调内拒绝重入。 */
AM_API am_status_t amc_source_set_decode_policy(amc_source_t* source, uint32_t policy);
```

本地r7兼容扩展支持source快照（kind=source），包含requested_policy/effective_policy、decoder_active与最近原因；源排空期间返回closing、完成后才返回closed，两者均隐藏实际执行状态。RTSP故障/冷却/配置代次的边界见[native-hardware](native-hardware.md) H2.3b。r6原件保留，1.1尚未对外发布，开发绑定仍须与同提交DLL配套。

JSON必需字段schema、kind、state、selection、actual_backend、last_failure；context有探测完成的单调时钟completed_at_ms。selection沿用拥有的Rust选择报告，包含policy、selected（后端与设备）、attempts（初始化/目标配置/资源/预算/忙等原因）、fallback、elapsed_ms。selected只代表初始化选择，实际码流只能由decoder真实帧确定；probe报告actual_backend始终null，设备已释放。decoder的state区分initialized/verified/recovered_cpu/closed，实际值是最近观察结果，不表示关闭后仍持有设备；last_failure保留原生解码错误类别与可用的错误号。普通查询不会刷新设备列表，显式probe/reset才刷新。GPU标识无凭据、地址或运行库路径；LUID为无符号整数，调用方不得经浮点损失精度或与CUDA ordinal混同。

未发布的R2兼容扩展：selection新增可选 `backend_support` 数组，依次报告 d3d11va 和 dxva2 的 `compiled` 与 `codec_configured`（布尔或 null）；null 表示未检查，false 表示明确不支持，设备未发现也不等同运行库未编译。SoftwareOnly、预算/准入未获时保持 null，检查并不创建设备，也不证明实际码流成功。现有客户端须忽略未知 JSON 字段；该 Rust 公开结构新增字段，使用结构字面量的调用方需补字段，不变更 C 结构布局或导出。

探测按策略找到首个可初始化设备即停止，attempts仅表示已经尝试的候选，不是完整显卡清单或全部profile支持证明。该限制不取代后续完整设备/源策略任务；不得把一次成功泛化成所有GPU/码流兼容。

探测缓存每context最多一份，探测阶段预留64KiB控制额度，完成后按缓冲capacity及拥有者开销收缩，许可覆盖缓存寿命；同context最多一轮，重叠调用返回tryAgain。显式新探测先撤销旧缓存，期间查询not_probed，失败不伪装成旧成功。查询decoder的序列化暂存预留32KiB控制额度；余额不足明确返回resourceExhausted，不为查询扩大默认控制预算。无持久化、后台探测线程或无界诊断历史。本扩展不承诺RTSP自动硬解已完成，也不把GPU冷启动当吞吐提升。

### H5.0b 显式推理设备探测与只读报告（ABI 1.1 本地r8）

```c
AM_API am_status_t amv_probe_inference_hardware(amv_context_t* ctx, uint32_t budget_ms);
AM_API am_status_t amv_get_inference_hardware_report(amv_context_t* ctx, char* buffer_utf8,
                                                   uint64_t capacity, uint64_t* required);
```

probe只接受活动context，回调内拒绝；预算1..5000ms，最多16个CUDA ordinal，进程单轮原生驱动准入，重叠返回tryAgain。不创建ORT模型、GPU计算上下文或更换运行库，无ORT纯共享帧context也可用。原生调用不能强杀，超软预算据实标记。缺驱动/驱动失败/预算中止作为完整报告内stopped状态保存，成功返回表示已记录结果，不表示设备或provider可用。参数、准入、资源和句柄错误仍返回错误码。显式新probe先清旧报告，失败不冒充旧成功；无后台探测。

get只复制缓存设备报告及进程累计后端观察，schema=framebraid.inference-hardware/1，最大64KiB含NUL；NULL+0查长度，小缓冲写required、首字节清0并返回limitOutOfRange，缓冲与required不重叠；并发状态变化时按新required重试或直接分配64KiB。回调中允许只读查询。state为not_probed/probe_complete/closing/closed；completed_at_ms为单调完成时间，未探测为null。device_probe含CPU可用逻辑并行度、CUDA驱动/初始化/版本、可见数/最多16设备、stopped、elapsed_ms和soft_budget_exceeded；CPU并行度不是物理核心数，CUDA ordinal/UUID不得当DXGI标识。缺驱动不是已确认无GPU。observations字段accelerator_domain/loaded_mask/executed_mask只表示当前SDK域的历史观察，不是当前模型健康、全节点GPU放置或运行库依赖清单。runtime_initialized区分是否初始化ORT，不据显卡存在设provider成功。

未发布的 H5 迁移：C 旧矩阵 `amv_query_backends` 不再自行加载 CUDA 驱动；仅读取同 context 最近的显式 probe 快照。尚未探测、探测失败而设备数未知时 CUDA 的 hw_detected 为0且 note 明确 unknown；已验证设备数为0时 note 明确 no-device，非零时为1。旧调用方若依赖一次查询立即探测，应先调用显式 probe 后查询或读取详细报告；旧 C 结构布局及字段值域不变。探测刷新期间旧快照撤销，不将上次成功误认为当前。Rust 独立 runtime::query 现亦迁移为未知快照，详见 inference-selection.md；旧依赖隐式探测的 Rust 调用方需改用显式 probe。

关闭中/完成后可查询空快照，隐藏设备与观察，绝不重新分配服务或访问驱动；destroy后句柄不得再用。缓存每context一份，probe预留16KiB内部控制额度（设备名称原始最多256字节，UTF8容错最多768字节），报告缓存不作为外部租约阻塞shutdown；查询临时预留48KiB（含32KiB定容序列化缓冲，无JSON树副本），余额不足resourceExhausted。该阶段当时不改旧query_backends；后续缓存迁移见上方H5迁移说明，模型策略和恢复仍按H5后续，不以两导出宣称全量完成。英文/中文接入说明与Python/C#便捷封装配套同提交DLL，旧开发r7不能加载新入口。旧1.0/1.1结构/枚举/导出全保留；r7冻结原件保留，新r8指纹绑定JSON最小合同。

### 1.1 句柄体系与对象生命周期总则（B07）

```
amc_context_t（相机库上下文=显式资源域，每进程可多实例，预算各自独立 §6）
 ├─ amc_source_t        视频源（RTSP/ISAPI 地址直连或已解析流地址；可按权限复用底层连接）
 │   ├─ amc_encoded_sub_t   编码流订阅（推或拉，每订阅独立游标与预算）
 │   ├─ amc_frame_sub_t     解码帧订阅（采样节拍+缩放档，推或拉）
 │   │   └─ amc_frame_t     帧句柄（可 retain 供跨库/异步使用）
 │   └─ amc_capture_sub_t   连续抓图订阅（N02；推或拉）
 ├─ amc_decoder_t       独立解码器（N01；与网络源解耦，父对象为 ctx）
 ├─ amc_shared_pool_t   共享帧发布池（SDK-DEC-07；ctx子对象，关闭等远端借用安全排空）
 ├─ am_encoded_packet_t / am_captured_image_t / am_probe_result_t   编码包（引用计数）/抓图/探测结果
 └─ request_id (uint64_t)   异步请求（抓图/推理；快照可查询 §5.4）

amv_context_t（推理库上下文=显式资源域）
 ├─ amv_model_t         模型句柄（档案或 raw 模式；后端策略；身份版本；同帧可供多模型）
 │   └─ request_id (uint64_t)  异步推理请求（快照可查询终态）
 └─ amv_result_t        推理结果（结构化五类+命名原始张量；引用计数 §3.6）
```

**对象生命周期：关闭三段式 + 句柄销毁两阶段（B07/P1，全部对象统一）**：close 与 destroy 职责分离——**close 后句柄仍登记可查询，destroy 后句柄不可用**。生命周期为「请求关闭 → 等待/查询 → 关闭完成（CLOSED）→ 句柄销毁（destroy）」：

1. **请求关闭**：对象专属 close 函数（`am?_shutdown` / `amc_source_close` / `am?_unsubscribe` / `amc_capture_continuous_stop` / `amc_decoder_close` / `amv_model_close`，带 timeout_ms），或异步入口 `am?_object_request_close`（仅标记，不等待；回调内与重入白名单允许，§5.3）。
2. **等待/查询**：close 带 timeout 即等待一轮；超时返回 `AM_LIFECYCLE_CLOSING`（资源保留），经 `am?_object_query_state` 轮询终态。四态：RUNNING / CANCEL_REQUESTED / CLOSING / CLOSED（§5.4）。
3. **关闭完成（CLOSED）**：close 返回或查询到达 `AM_LIFECYCLE_CLOSED` 即关闭屏障达成——此后不再有任何回调/输出，工作资源已释放或出账；**句柄仍登记在册**：`object_query_state` 持续返回 CLOSED、重复 close 幂等返回 AM_OK、其余业务调用返回 `sdk.handleClosed`；**回调与 user_data 自此才可释放**（屏障前禁止）。
4. **句柄销毁（destroy，P1）**：`am?_object_destroy(handle)` 是句柄最终释放的唯一入口——仅 CLOSED 状态可调（对非 CLOSED 对象调用返回 `sdk.handleInvalid` 附当前状态）；销毁后登记表移除、句柄地址进入**隔离区**（隔离区容量边界见下段）。缓冲类句柄（packet/frame/result/image）不经 close/destroy，直接引用计数 release（§5.1）。

**隔离区容量与回收边界（P1 三轮）**：隔离区为**每库进程内最多保留 4096 个已销毁句柄地址**（候选值，§6 表已登记）——隔离区未满时**同地址永不复用**（防晚到并发查询命中新对象）；隔离区满后**最老的已销毁地址允许复用**（环形淘汰，业界惯例），此时与新句柄的地址碰撞在所难免。因此合同显式声明：**destroy 后继续使用旧指针=未定义行为（UB）**，SDK 不对其实施任何防护或承诺（§5.3 回调资源释放条款与「释放后返回 handleClosed」承诺均不覆盖已 destroy 句柄）；调用方在 destroy 后不得再以任何方式引用旧句柄。隔离区条目不占用工作资源（仅地址记录，固定小内存），长期反复创建销毁不产生持续增长的资源占用。

**并发窗口安全（P1）**：只读查询（query_state/status/get_info/stats）在 CLOSING/CLOSED 全程任意线程并发安全（CLOSED 后如实返回终态）；destroy 属变更类调用，与同句柄其他调用按既有「变更类串行」规则外部串行；**隔离区未满时**（常态）地址永不复用，与 destroy 竞态的晚到查询最坏得到 `sdk.handleClosed`/`sdk.handleInvalid`，不会命中其他对象；**隔离区满（4096）后最老地址复用**，此极端下旧指针可能命中新对象——这正是「destroy 后使用=UB」条款存在的理由，SDK 不承诺防护。「释放后返回 handleClosed」的承诺仅覆盖已 close 未 destroy 的登记句柄；destroy 后的野指针调用不在承诺范围。

**模块与依赖寿命（2026-10-26用户确认）**：SDK对象、外持和回调排空、SDK自有工作线程停止join后，两个SDK模块须能安全卸载/重载。ORT/FFmpeg底层运行库允许由宿主进程持有至退出，不随SDK强制卸载；不得遗留指向已卸载SDK的回调/SDK自有线程。预热后反复装卸须核私有字节、句柄和线程无持续增长，运行库路径/后端冲突仍拒绝，切换需重启宿主。详见[冻结r5](dual-sdk-abi-freeze-v1.md)。

**父子顺序**：订阅/解码器/连续抓图先于源完成关闭与销毁（destroy），源先于 ctx，模型与其在途请求先于 ctx。对仍有子对象打开的父对象请求关闭：SDK **级联请求**全部子对象关闭，按子先父后顺序完成屏障；调用方也可先手动关闭子对象再关父级。销毁（destroy）同样子先父后；SDK 不在子对象工作仍引用父资源时销毁后者。

销毁顺序示例：`amc_encoded_unsubscribe`（→CLOSED）→ `amc_frame_unsubscribe` → `amc_capture_continuous_stop` → `amc_source_close` → 各对象 `amc_object_destroy`（子先父后）→ `amc_shutdown` → `amc_object_destroy(ctx)`；子对象未销毁时关闭上级走关闭屏障级联排干（§5.4）。
同句柄并发许可：只读查询任意线程并发；变更类调用（close/destroy/set_*/unsubscribe）同句柄串行。

### 1.2 ABI 版本协商入口

```c
/* 两库对称。返回 (主版本<<16)|次版本；主版本不符禁止 Init */
AM_API uint32_t amc_get_abi_version(void);
AM_API uint32_t amv_get_abi_version(void);
/* 包/运行库摘要（无 ctx 可调，SDK-G3 依赖诊断用） */
AM_API am_status_t amc_get_version_info(am_version_info_t* out_info);
AM_API am_status_t amv_get_version_info(am_version_info_t* out_info);
/* 版本协商初始化：params.abi_version 不支持 → sdk.abiUnsupported，不按错误布局执行 */
AM_API am_status_t amc_init(const amc_init_params_t* params, amc_context_t** out_ctx);
AM_API am_status_t amv_init(const amv_init_params_t* params, amv_context_t** out_ctx);
/* 关闭屏障（三段式第 1+2 步）：超时返回 state=Closing（资源保留），query 轮询终态；
   CLOSED=屏障达成（句柄在册，§1.1）；句柄最终释放经 amc_object_destroy（§5.4） */
AM_API am_status_t amc_shutdown(amc_context_t* ctx, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amv_shutdown(amv_context_t* ctx, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amc_shutdown_query(amc_context_t* ctx, am_lifecycle_state_t* out_state);
AM_API am_status_t amv_shutdown_query(amv_context_t* ctx, am_lifecycle_state_t* out_state);
/* 进程级 init 失败单槽快照（P3）：并发语义显式——槽内为最近一次 init 失败，原子读不撕裂，
   无 TTL 静默失效；attempt_seq 递增可判是否已被更新的失败覆盖；无失败记录 code=AM_OK；
   P3 三轮：message 内嵌固定缓冲 char[256]（附录 §1.2）——读取返回即自包含快照，
   彻底消除「下个失败覆盖后字符串失效」窗口，调用方无需与 attempt_seq 对赌字符串有效性 */
AM_API am_status_t amc_get_init_error(am_init_error_t* out);
AM_API am_status_t amv_get_init_error(am_init_error_t* out);
```

`amc_init_params_t` / `amv_init_params_t` 完整定义见附录 §1：均为 struct_size + abi_version + 显式 runtime_dirs（B15/SDK-G3）。**消费方式单选合同（P4 三轮定约、四轮检测口径降级，两库对称）**：首版限定**一个进程一种消费方式**——同一核心（am-camera / am-vision）在进程内要么由宿主静态链接消费（静态核心直接 init），要么经 DLL 消费（加载 am_camera.dll / am_vision.dll 后 init），**禁止两种形态并存**。**单选=部署约束（P4 四轮）：由部署方/宿主保证（进程内二选一），SDK 不承诺自动互检**——宿主与各 DLL 为分别编译的模块，不存在跨模块共同的登记点，各自持有的静态原子变量互相不可见、无法据此可靠互检（三轮「进程级原子标志检测」承诺作废）。SDK 承诺口径为**尽力检测**（实现期探索手段，如进程内同名符号探测、加载器环境探测等）：`am?_init` 检测到进程内已存在另一形态的消费者（如宿主静态核心已创建核心实例后再加载 DLL 调 init，或反之）时返回 `sdk.domainConflict`（错误详情注明检测到的现有消费形态），明确拒绝、不静默并存；**未检出而实际并存**时两形态各按自身预算独立记账与拒绝（超限 `sdk.resourceExhausted`），由此导致的资源超限不视为 SDK 缺陷、行为不在承诺范围。在该单选约束下，**6.2a 的 executor 进程内单例即真实满足共享语义**——同进程只有唯一消费入口，执行器实例、在途账本（domain_infer_slots）与线程预算天然只有一本账，不存在「宿主与 DLL 各一份执行器」的装配缺口。推理侧参数：infer_threads、总线程预算 `total_thread_budget`（执行器+ORT intra/inter-op+预处理线程合计上限，0=机器核数默认；用量经 stats.threads_active/peak 观测）与执行器域在途预算 `domain_infer_slots`（首个创建执行器的 init 定值；同进程后续 amv_init 须与现存实例一致（0=继承现值），不一致返回 `sdk.invalidArgument` 附说明）。

### 1.3 命名、调用约定与 UTF-8

- 前缀：相机库 `amc_`、推理库 `amv_`；两库公共结构/枚举/常量用 `am_` 前缀，定义一致（单一真源头文件生成，双头文件均含）。**类型名统一规范（P2）**：公共类型一律 am_ 前缀且为唯一规范名、无 amc_/amv_ 别名——如编码包类型=`am_encoded_packet_t`（管理函数按所属库前缀：`amc_encoded_packet_retain/release`）。字符串全部 UTF-8、NUL 结尾；路径由调用方保证 UTF-8；DLL 不修改调用方缓冲。
- 不导出 Rust/FFmpeg/ORT 内部对象；公开头文件从受控 FFI 声明生成或一致性校验。
- 调用约定细则（Windows x64 单一调用约定、`AM_API` 不修饰名导出、布尔 uint32_t、无位域）见附录 §5；常量数值表（冻结候选）见附录 §6。

### 1.4 结构 size/version 演进与布局策略（B09）

**演进规则**：

- 每个公开结构首字段 `uint32_t struct_size`，调用方填 `sizeof(自身编译版本)`。
- 小版本兼容：只在**结构尾部追加**字段；DLL 按已知前缀解释，size 更大忽略尾部、更小按旧版处理（旧字段语义不变）；枚举值不复用不重排；释放函数签名永不改；新能力经能力位发现。
- 破坏兼容：提升 ABI 主版本，导出可区分入口（v2 前缀 `amc2_/amv2_` 或独立 DLL 名），与 v1 可并存加载。
- `struct_size` 小于 DLL 认识的最小版本 → `sdk.structSizeInvalid`。

**布局策略（B09 核心，附录为权威蓝本）**：

1. **固定叶结构**：所有按值嵌入的结构为固定大小叶结构，字段显式列出、无内嵌变长成员（可含指向独立缓冲的 const 指针，字段集冻结）；编译器自然对齐，跨语言包装按 C 布局逐字段复刻（如 `am_keypoint_t`、`am_scale_preset_t`、`am_roi_map_t`、`am_spatial_transform_t`、`am_model_identity_t`、五类 result 头、`am_backend_actual_t`）。
2. **变长内容一律指针+count 引用**：数组、字符串、掩膜等变长数据不在结构体内联，统一经 `const T* + count`（或 `+stride`）指向独立缓冲；数组元素本身必须是固定叶结构或固定叶结构的指针，**元素步长恒为 `sizeof(T)`（无压缩数组）**，指针数组元素为 8 字节句柄。**数组元素为固定布局（P2）**：作为数组元素的结构（tensor/keypoint/detect_box/segment_instance/obb_instance/probe_candidate 等）字段集冻结、永不增长——需要演进的能力改走指针数组（元素=8 字节指针/句柄）或新函数入口，不通过增大元素 sizeof(T) 演进。
3. **可演进结构不按值嵌入（P2）**：凡带 `struct_size` 的可演进结构，**不得按值嵌入任何其他结构**——一律 `const T*` 指针引用（被指对象自带 struct_size 判版），杜绝「嵌套按值嵌入可扩展结构导致外层字段位移」破坏尾部追加规则。按值嵌入仅限固定布局子结构（字段冻结、无 struct_size）；其演进经外层结构尾部追加**新指针字段**表达（落点：请求快照 error、批量项 image、结果 frame/transform、抓图 meta、模型描述 backend 等）。
4. **版本化访问器**：可演进的大结构（error/limits/stats/请求快照/结果）只增尾部字段 + `struct_size` 判版；包装层不得按偏移硬编码访问未冻结字段。
5. **释放归属**：凡经指针暴露的 DLL 拥有缓冲，随所属对象的 release/close/destroy 释放；长期持有由调用方复制或 retain 拥有者句柄（§5.1）。

## 2. am_camera.dll 函数清单

### 2.1 能力查询

```c
/* 能力位+像素格式位集+各上限；结构只增位，旧消费者读旧字段 */
AM_API am_status_t amc_query_capability(amc_context_t* ctx, amc_capability_t* out);
```

`amc_capability_t`（完整定义见附录 §2.1）要点：`feature_bits` 含 `AMC_CAP_ENCODED_H264 | AMC_CAP_ENCODED_H265 | AMC_CAP_DECODE_H264 | AMC_CAP_DECODE_H265 | AMC_CAP_DECODE_MJPEG | AMC_CAP_DECODER_STANDALONE | AMC_CAP_CAPTURE_ISAPI | AMC_CAP_CAPTURE_CONTINUOUS | AMC_CAP_PUSH | AMC_CAP_PULL | AMC_CAP_REUSE | AMC_CAP_SCALE | AMC_CAP_BSF_ANNEXB | AMC_CAP_BSF_LENGTH_PREFIX | AMC_CAP_SHARED_FRAME_POOL`；`pixel_formats` 为 AM_PF_* 位集（首版集合 §9-O4）；含 max_sources / max_decoders / max_subscribers_per_source / 帧字节预算。能力位=0 不冒充支持（B01：H.265 与连续抓图均显式位）。**封装格式支持范围与能力位对齐（P7）**：Annex B 与长度前缀各占一位独立声明，位=0 即该封装不可用（首版长度前缀位=0，仅 Annex B），请求未开位的封装返回 `camera.codecUnsupported`。**共享帧池三态（SDK-DEC-07）**：`shared_pool_state` 区分 `可用 / 未启用 / 未编入 / 平台不支持` 四值（附录 §1.6 枚举）——能力位只表达「本构建具备该能力」，三态说明当前为何不可用；任一不可用状态下调共享帧池接口按对应错误明确拒绝，不得偷偷退化成其他有损策略。

### 2.2 凭据（宿主注入，DLL 不持久化；B04 连接前认证）

凭据注入时机分两种，均明确、无鸡生蛋歧义：

```c
/* ① 原子等价流程（推荐）：随 source_open 一次性传入（desc.creds / desc.provider，§2.4），
   open 返回即开始认证——初次连接前凭据已就位，不出现"无凭据连接"窗口 */
/* ② 已打开源的更新/失效：运行期凭据轮换（认证最终失败进入 AUTH_WAITING 后等新凭据，B13） */
AM_API am_status_t amc_credentials_set(amc_source_t* src, const am_credential_data_t* creds);
AM_API am_status_t amc_credentials_set_provider(amc_source_t* src, const am_credential_provider_t* provider);
AM_API am_status_t amc_credentials_invalidate(amc_source_t* src, uint32_t known_version);
```

- reuse=1的句柄属于共享权限域。轮换影响该域全部消费者并递增源代次；旧权限不误复用新连接。共享provider的user_data只能在on_drop后释放，关闭其中一个Source不代表最后消费者已退出；源事件与各订阅回调仍按其句柄单独排空。
- 注入后凭据版本+1；provider 回调在认证最终失败或版本失效时取新凭据（fetch 返回新凭据+新版本号；失败返回错误码，SDK 停留在 AUTH_WAITING 不沿旧凭据风暴重试，B13）。
- `amc_credentials_invalidate`：清本源认证/探测缓存，后续认证与探测用新凭据；认证失败与超时可区分（§7）。
- 探测请求 `am_probe_request_t.creds` 直接带凭据（探测期借用，§2.3）。
- 诊断、错误消息与统计中禁止出现密码、令牌或含凭据的完整地址（DLL 侧统一脱敏，§7 尾段）。
- `am_credential_data_t` / `am_credential_provider_t`（fetch+on_drop+超时）完整定义见附录 §2.2；on_drop 在注销屏障后回调一次，此后可释放 user_data。

### 2.3 流探测（时效+失败分类）

```c
/* 多候选并行探测：每候选独立结果（可达/编码/尺寸未知分级/观察时间/有效期）；支持强制刷新；
   超时、认证失败、不支持、信息未知在候选结果内分类，不互相冒充。
   候选引用为调用方已解析的具体地址（RTSP/ISAPI URL），不做 DEVICE_REF 解析（B04，§2.4） */
AM_API am_status_t amc_probe_streams(amc_context_t* ctx, const am_probe_request_t* req, am_probe_result_t** out);
AM_API am_status_t amc_probe_result_release(am_probe_result_t* result);
```

探测状态六态 `am_probe_status_e`：OK / TIMEOUT / AUTH_FAILED / UNREACHABLE / UNSUPPORTED / PENDING（仍在探测或缓存过期未定）。每候选 `am_probe_candidate_t` 独立返回（reference/codec/width/height/fps/observed_at/valid_until；未知项不伪造），探测成功 TTL 默认 300s、失败 30s（候选值，§9-O1）。不保证任意品牌恰有三条固定流地址；候选由调用方给出（地址生成政策留宿主）。完整结构见附录 §2.3。

### 2.4 视频源（B04 认证流程 / B12 源身份 / B13 状态机与故障分型）

**连接前认证流程（B04）**：`amc_source_open` 的 `am_source_desc_t` 可原子携带凭据（`creds` 或 `provider` 二选一）。流程二选一：

- **原子流程（推荐）**：`desc.creds != NULL`（或 provider）→ open 返回后源直接进入 CONNECTING 并完成认证。
- **分步流程**：`desc.creds == NULL` → 源停在 `AM_SRC_WAITING_CREDENTIALS`（**不发起任何网络连接**），调用方随后 `amc_credentials_set/set_provider` → 自动进入 CONNECTING。凭据注入前不产生认证失败。

**地址类型与 DEVICE_REF 解析口径（B04）**：`am_source_kind_e` 三值——`AM_SRC_RTSP_URL`（RTSP 地址直连）、`AM_SRC_ISAPI_URL`（ISAPI 抓图地址，与 RTSP 分开声明，不含内嵌凭据）、`AM_SRC_DEVICE_REF`。**DEVICE_REF 本版只接受"已解析描述"**：`url_utf8` 必须是宿主解析设备引用后得到的具体流地址（RTSP 或 ISAPI URL），SDK 不内建设备引用解析器；`stream_kind_utf8`（"main"/"sub"/"isapi"）仅为宿主提示的类型标注，SDK 按地址协议与流内容实际行为执行。设备三流地址生成、DPAPI 持久化等政策留宿主（camera spec「凭据注入」Requirement）。ISAPI 地址误用于 RTSP 订阅（或反之）按协议探测结果返回 `camera.codecUnsupported`/`camera.urlInvalid`，不猜测。

**MJPEG 流源（R7，未发布兼容扩展）**：`AM_SRC_MJPEG_URL = 3`——HTTP `multipart/x-mixed-replace` MJPEG 流地址（不含内嵌凭据，凭据注入与 ISAPI 同套）。**流语义（boundary 分帧为唯一真相源）**：响应 `Content-Type` 的 `boundary` 参数（≤70B，支持引号包裹）决定分帧；`Content-Length` 不参与分帧，谎报仅计数。分帧仅在行首 `\r\n--boundary` 上下文（流首 boundary 允许无 CRLF 前缀），且 part 头 `Content-Type=image/jpeg` + JPEG SOI/EOI 魔数双校验；载荷内出现 boundary 字节序列不误切，无法区分的极端歧义降级为丢帧。**有界硬约束（防恶意流）**：boundary ≤70B / part 头 ≤64KB / 单帧 ≤8MB——超限丢帧+计数，连续 3 次超限断连进重连退避。**双超时**：无数据（对端黑洞）与无 boundary 进展（有数据但恶意无界流）分别判定，超时断连重连（复用既有指数退避）。JPEG 帧进入既有解码管线（含 EXIF 方向）与像素订阅/预算/统计语义，与 ISAPI 快照一致。

```c
/* 开源：原子凭据见上；reuse 时复用键=源配置+凭据权限哈希（不同权限不复用）；
   满载 camera.sourceLimitReached */
AM_API am_status_t amc_source_open(amc_context_t* ctx, const am_source_desc_t* desc, amc_source_t** out_src);
/* 关闭（三段式第 1+2 步）：重试等待中也可关；超时返回 Closing（可查询），CLOSED 即最终释放；
   完成后本源不再有任何回调/输出 */
AM_API am_status_t amc_source_close(amc_source_t* src, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
/* 状态查询：状态机+源身份+代次+故障分型+重连计数；只读并发安全 */
AM_API am_status_t amc_source_get_status(amc_source_t* src, am_source_status_t* out_status);
/* 重连策略：指数退避上限/抖动/重试次数；不设即默认值（§6/§9-O8） */
AM_API am_status_t amc_source_set_reconnect_policy(amc_source_t* src, const am_reconnect_policy_t* policy);
```

**状态机（B13）**：`WAITING_CREDENTIALS → CONNECTING → LIVE →（断流）RECONNECT_WAIT → … → CLOSING → CLOSED`；LIVE/RECONNECT_WAIT 期间**最终认证失败**（凭据被明确拒绝且重试耗尽；正常 Digest challenge 握手**不算**最终失败）→ `AUTH_WAITING`（停止重试，等 `credentials_set`/provider 更新后自动重连）。`am_source_status_t` 完整定义见附录 §2.4，要点：

- `source_id`（uint64，ctx 内唯一，B12：帧/包/结果关联的源身份）、`generation`（源代次）、`config_generation`（码流/分辨率配置代次）。
- `fault_class`（`am_fault_class_e`，B13 分型）：NONE / NETWORK / AUTH / RESOURCE / KEYFRAME_WAIT / DECODE——网络故障、资源不足、等待关键帧、解码失败分开上报，不笼统"断流"；假活（输入活跃但解码停顿）经 fault_class=KEYFRAME_WAIT/DECODE 与 last_frame_at_ms 组合可辨。
- 重连退避默认 1s 起、上限 30s（§9-O8 统一后口径，包络上限 60s）；退避期间 close 可打断（不无限阻塞）。
- **软件回退预算准入（B13）**：硬解失败转软解须先通过本资源域解码字节预算准入（§6），超限返回 `sdk.resourceExhausted`（fault_class=RESOURCE），不静默把全部路挤到 CPU。

### 2.5 编码流订阅（推+拉双模式；B01 H.265 / B12 源身份）

```c
/* 订阅编码输出：codec_mask 选 H.264/H.265/MJPEG；mode 推/拉单选——同订阅句柄另一模式返回
   camera.modeConflict（双模式另开订阅，独立游标）；满载 camera.subscriberLimitReached；
   首包从配置代次+可解码关键帧起发 */
AM_API am_status_t amc_subscribe_encoded(amc_source_t* src, const am_encoded_sub_desc_t* desc, amc_encoded_sub_t** out_sub);
/* 注销屏障（三段式）：CLOSED 后不再回调该订阅；推模式必须先注销再关源 */
AM_API am_status_t amc_encoded_unsubscribe(amc_encoded_sub_t* sub, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
/* 推模式注册回调（拉模式调用返回 camera.modeConflict）；传 NULL=注销（同样走屏障） */
AM_API am_status_t amc_encoded_set_callback(amc_encoded_sub_t* sub, am_encoded_cb_fn cb, void* user_data);
/* 拉模式取包：无新包且超时返回 sdk.deadlineExceeded；重同步事件经 §2.9 事件回调 */
AM_API am_status_t amc_encoded_pull(amc_encoded_sub_t* sub, uint32_t timeout_ms, am_encoded_packet_t** out_pkt);
/* 引用计数：retain 后跨异步任务持有；release 归零由相机 DLL 释放 */
AM_API am_status_t amc_encoded_packet_retain(am_encoded_packet_t* pkt);
AM_API am_status_t amc_encoded_packet_release(am_encoded_packet_t* pkt);
typedef void (*am_encoded_cb_fn)(amc_encoded_sub_t* sub, am_encoded_packet_t* pkt, void* user_data);
```

- 编码常量：`AM_CODEC_H264=0x1 / AM_CODEC_H265=0x2 / AM_CODEC_MJPEG=0x4`（**H.265 首版纳入**，B01/SDK-DEC-01）；源不支持的位订阅时即拒（`camera.codecUnsupported`）。
- 慢消费者语义：订阅队列满 → 丢弃至最近关键帧边界并发出 `AM_EVENT_ENCODED_RESYNC`（带原因与代次），其后从携带匹配参数集的可解码关键帧续发；**H.265 恢复点按其档案声明的随机访问语义判定（VPS/SPS/PPS + IDR/CRA 及预滚），不把任意标记 keyframe 的包当安全恢复点**（N03/B12）；不输出任意接回的花屏 P 帧链；不影响其他订阅者（各自预算）。
- `am_encoded_packet_t`（附录 §2.5）：codec（真实编码不冒充）/bitstream_format（Annex B；长度前缀按能力位声明，首版候选仅 Annex B，§9-O4）/flags（KEYFRAME|PARAMSETS|AU_COMPLETE；**H.265 的 PARAMSETS 含 VPS/SPS/PPS**）/config_generation/source_generation/**source_id**（B12）/seq/pts/dts/time_base/data/size。
- `am_encoded_sub_desc_t`：codec_mask / mode / queue_max_items / queue_max_bytes（默认值候选见 §6）。

### 2.6 解码帧订阅与缩放档（B12 帧身份）

本地R2节拍内部优化：sampling_max_fps仍为严格最短选择间隔，按纳秒向上取整；不是最低交付承诺。像素worker可提前一个采样周期（最多50ms）准备后短sleep等实际到期，不持发布/订阅/队列锁，停止/epoch失效后不发布。全部有效像素订阅均限帧且内部已有更新包时，可保留参考链解码、略过旧画面物化；有0上限订阅不做此额外合并。无新ABI/线程/额度；帧龄代价和本机验证见[记录](../verification/sdk-r2-cadence-20261026.md)，不外推所有编码/硬件或硬实时。

本地R3像素隔离实现：RTSP读线程只维护压缩参考链/分发，首次实际像素需求才在既有max_decoders额度中懒创建一条worker，MJPEG像素处理同样占该槽；纯压缩不占解码槽。内部队列服从既有AM_Q_ENCODED_STREAM数量/字节及总内存，另至多一项在途；满载只清像素参考链并等待下一随机访问点。源/配置/像素epoch在发布前重验，旧在途结果不贴新代次。驱动调用不持发布短锁，关闭仍等待实际执行返回；不可杀死的调用不创建替代线程、不提前报Closed。签名/布局/回调线程合同不变，通用Media threads统计的完整覆盖仍属R3审计，不能以零值当无worker证明。

```c
/* 订阅解码帧：维持解码参考帧依赖，按 sampling_max_fps 节拍输出已解码新鲜帧（三速率分离）；
   像素格式不在能力位集 → camera.pixelFormatUnsupported */
AM_API am_status_t amc_subscribe_frames(amc_source_t* src, const am_frame_sub_desc_t* desc, amc_frame_sub_t** out_sub);
AM_API am_status_t amc_frame_unsubscribe(amc_frame_sub_t* sub, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amc_frame_set_callback(amc_frame_sub_t* sub, am_frame_cb_fn cb, void* user_data);
AM_API am_status_t amc_frame_pull(amc_frame_sub_t* sub, uint32_t timeout_ms, amc_frame_t** out_frame);
AM_API am_status_t amc_frame_retain(amc_frame_t* frame);
AM_API am_status_t amc_frame_release(amc_frame_t* frame);
/* 借用视图：plane 指针+stride+格式+尺寸；指针有效期到最后一次 release（§5.1） */
AM_API am_status_t amc_frame_get_view(const amc_frame_t* frame, am_frame_view_t* out_view);
/* 帧元数据（B12 完整身份）：source_id/frame_id/seq/双代次/时间三元组+时间基+空间变换 */
AM_API am_status_t amc_frame_get_meta(const amc_frame_t* frame, am_frame_meta_t* out_meta);
/* 按需缩放：从已有帧生成目标尺寸新句柄（默认不每档预分配）；计入外部持有预算 */
AM_API am_status_t amc_frame_get_scaled(const amc_frame_t* frame, uint32_t target_w, uint32_t target_h,
                                        uint32_t method, amc_frame_t** out_scaled);
/* 帧包装为跨 DLL 像素视图（喂推理，§5.6）；帧须已由调用方持有 */
AM_API am_status_t amc_frame_as_pixel_view(amc_frame_t* frame, am_pixel_view_t* out_view);
typedef void (*am_frame_cb_fn)(amc_frame_sub_t* sub, amc_frame_t* frame, void* user_data);
```

- 缩放method固定0=最近邻、1=双线性（像素中心对齐）；其他值拒绝。缩放保留源帧身份/时间和格式，transform表达原源坐标到当前像素的比例；无法精确表示既有旋转/偏移组合时valid=0，不猜测。NV12目标要求偶数宽高，输出色彩信息随帧携带。
- `am_frame_meta_t`（B12，附录 §2.5）在原序号/代次/时间三元组基础上补齐**独立帧身份**：`source_id`（源身份）、`frame_id`（进程内唯一，与编码包无一一对应假设——一包可产多帧、丢包后帧序仍单调）、`source_generation`、`config_generation`、`pts + time_base_num/den`（时间基显式携带，默认 1/90000；ISAPI 无 PTS 时 pts=INT64_MIN 且 clock_mask 不含 PTS）、`clock_mask`（PTS|RECEIVED|CAPTURED 有效来源掩码）、`captured_at_ms`（未知=INT64_MIN，不与其他时钟混同）、`transform`（空间变换）。
- 解码帧与编码包的关联只经 PTS/时间基软关联，**SDK 不承诺一包一帧映射**；调用方不得以包序推帧序。
- 像素格式首版集合（§9-O4 候选）：RGBA8/RGB8/BGR8/NV12；plane 有效长度经 `am_frame_view_t.plane_size_bytes[4]` 与 `strides[4]` 显式表达（P5：不假设行紧密，也不按 stride×height 猜测平面长度，遍历/拷贝以 plane_size_bytes 为准）；**色彩矩阵（BT.601/709/2020NCL）与色彩范围（LIMITED/FULL）经视图 `color_matrix`/`color_range` 字段每帧显式声明**（P5：NV12 必填有效值，RGB 格式忽略；YUV→RGB 换算按声明执行，不内定系数）。
- **时间原点与映射质量（P5）**：全部 `*_at_ms` 时间戳原点=进程单调钟（Windows QueryPerformanceCounter / Unix CLOCK_MONOTONIC，即 STEADY/MONO 语义，不受系统改时影响）；跨 DLL/跨库传值不改原点，未知=INT64_MIN。`captured_at_ms` 为设备时钟映射到本地单调钟的结果，映射质量经 `clock_quality`（am_clock_quality_e：UNKNOWN / LOCAL_RECEIPT 仅本地接收 / DEVICE_SYNCED 设备同步 / EXTRAPOLATED 外推）声明，不把外推冒充同步。

### 2.6a GPU 帧订阅与显存帧池（M2a，未发布兼容扩展）

像素订阅新增 `frame_kind`（尾部追加，struct_size 协商；默认 0=像素与既有行为完全一致）。

**GPU 帧交付语义**：
- `frame_kind=1` 时，硬解（PREFER_HW 实际激活）输出的 D3D11 纹理以 `am_gpu_frame_t` 不透明句柄交付；元数据（宽/高/DXGI 格式/子资源/关联身份）经 `amc_gpu_frame_desc` 查询。**主交付物是纹理指针**（同进程 COM 对象，借用期内有效，CUDA interop / DirectML 互操作直接消费）；解码纹理池创建即带 MISC_SHARED 标志
- `shared_nt_handle` 为可选字段（2026-10-29 实测修订）：尽力经 QI IDXGIResource::GetSharedHandle 提供；Win11 25H2 runtime 拒绝纹理 QI DXGI 接口（IUnknown 正常）时为 0。句柄不可得时标 `flags=1`（信息性标注，非回读判定），不失败
- 真正的 CPU 回读降级由消费侧（M2b Vision 接线）能力协商决定，届时计入 `stats_cpu_fallbacks`；句柄缺失≠回读
- SOFTWARE_ONLY 策略下 GPU 订阅返回 `notSupported`（不静默降级为像素）
- GPU 帧借用/归还语义与共享帧池一致（租约、池满拒绝、关闭排空）

**新导出**（池创建带显存上限，计费进既有 Charge 新类目 gpu_frame_bytes；统计含搬运审计计数器——M2c 验收主指标）：

```c
AM_API am_status_t amc_gpu_frame_pool_create(amc_context_t* ctx,
    const void* cfg, amc_gpu_frame_pool_t** out_pool);
AM_API am_status_t amc_gpu_frame_borrow(const amc_gpu_frame_t* frame);
AM_API am_status_t amc_gpu_frame_return(amc_gpu_frame_t* frame);
AM_API am_status_t amc_gpu_frame_release(amc_gpu_frame_t* frame);
AM_API am_status_t amc_gpu_frame_stats(const amc_gpu_frame_t* frame,
    am_gpu_frame_stats_t* out);
AM_API am_status_t amc_gpu_frame_desc(const amc_gpu_frame_t* frame,
    am_gpu_frame_desc_t* out_desc);
AM_API am_status_t amc_gpu_frame_texture(const amc_gpu_frame_t* frame,
    void** out_texture);
```

`amc_gpu_frame_desc`/`amc_gpu_frame_texture`（M2b 补，跨 DLL 消费查询）：宿主经它们组装
`am_gpu_frame_ref_t` 传给 Vision（§3.3a）；`texture` 借用期内有效，查询不转移所有权。

**能力位**：`AMC_CAP_GPU_FRAMES`（编译编入+运行时可用的双条件）。

**同进程约束**：纹理指针与 `shared_nt_handle` 均仅供同进程跨 DLL 消费（Vision 互操作）；句柄为可选增强（平台/runtime 支持时提供），纹理指针为始终可用的交付物；跨进程共享后续。

### 2.7 抓图（ISAPI：单次同步/异步 + 连续订阅 N02；B03 闭环）

**单次抓图（同步/异步）+ 异步取回闭环（B03）**：

```c
/* 同步抓图：受 req.deadline_ms 约束；错 camera.timeout/authFailed/captureCapacity 请求级返回 */
AM_API am_status_t amc_capture_sync(amc_source_t* src, const am_capture_request_t* req, am_captured_image_t** out_img);
/* 异步抓图：接受即返回 request_id；预算不足立即 sdk.queueFull（未接受，不伪造在途） */
AM_API am_status_t amc_capture_async(amc_source_t* src, const am_capture_request_t* req, uint64_t* out_request_id);
/* 结果取回（B03）：仅终态 Done 可调；幂等（保留期内重复 fetch 返回同一图像对象）；
   过保留期或已 ack → sdk.resultExpired；图像对象随 amc_image_release 释放 */
AM_API am_status_t amc_captured_image_fetch(amc_context_t* ctx, uint64_t request_id, am_captured_image_t** out_img);
/* 取走确认：仅释放 SDK 缓存引用；调用方已 fetch 持有的图像继续计入外部持有预算（B08/B10），
   直至 release */
AM_API am_status_t amc_captured_image_ack(amc_context_t* ctx, uint64_t request_id);
/* 引用计数（P3）：回调/借用期内需跨回调持有的图像必须当场 retain（§5.3 借用期）；
   release 归零由相机 DLL 释放。命名成对：amc_image_retain / amc_image_release */
AM_API am_status_t amc_image_retain(am_captured_image_t* img);
AM_API am_status_t amc_image_release(am_captured_image_t* img);
```

异步抓图调用流程（外部开发者仅凭本节可写出）：`capture_async` → `amc_request_query` 轮询快照句柄 `am_request_snapshot_t*`（§2.10，state+不可覆盖错误详情；P3 三轮句柄制）→ state=Done 后 `amc_captured_image_fetch` 取图 → `amc_captured_image_ack` 释放缓存引用（可选）→ 读毕 `amc_request_snapshot_release` 释放快照句柄（相机库分配、回相机库释放，P3 四轮分库） → 用毕 `amc_image_release`。完成记录默认保留 60s（TTL，§6）；失败/取消/过期终态一经写入**不可覆盖**，错误详情经快照读取（B03；已移交快照至 release 前字符串可读，出账后 query 返回 sdk.resultExpired）。`am_captured_image_t` 含输出类型（编码 JPEG 字节或 CPU 帧视图）+ 时间元数据（clock_mask，B12：接收时间可得、设备采集时间可得时才填）。

**连续取图订阅（N02/SDK-DEC-04）**：

```c
/* 连续订阅：SDK 负责定时、限速、恢复和缓冲；复用单次抓图的凭据/连接/请求管理与状态通道，
   不另建调度系统。连续图片不是 H.264/H.265 视频，不隐含编码器或录像能力 */
AM_API am_status_t amc_capture_continuous_start(amc_source_t* src, const am_capture_continuous_desc_t* desc,
                                                amc_capture_sub_t** out_sub);
/* 停止（三段式第 1+2 步）：停止后不再投递迟到回调；慢回调有自己的隔离与上限 */
AM_API am_status_t amc_capture_continuous_stop(amc_capture_sub_t* sub, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amc_capture_sub_set_callback(amc_capture_sub_t* sub, am_capture_cb_fn cb, void* user_data);
AM_API am_status_t amc_capture_sub_pull(amc_capture_sub_t* sub, uint32_t timeout_ms, am_captured_image_t** out_img);
/* 实际频率/接收时间/连续失败/帧龄统计（B03/N02：不以目标值冒充实际值） */
AM_API am_status_t amc_capture_sub_get_stats(amc_capture_sub_t* sub, am_capture_stats_t* out_stats);
typedef void (*am_capture_cb_fn)(amc_capture_sub_t* sub, am_captured_image_t* img, void* user_data);
```

`am_capture_continuous_desc_t`（附录 §2.7）：target_interval_ms（采样节拍）/max_inflight（同共享采集器并发 HTTP 抓图上限，**默认 1**：错过节拍跳过不补发历史请求）/request_timeout_ms/output_kind（原 JPEG 字节或 CPU 帧）/mode（推/拉独立订阅语义，同 §2.5 互斥规则）/queue_max_items+bytes。语义：

- **共享采集器规则**：同权限且需求兼容（频率≤既有、格式/变换兼容）的多消费者共享同一采集器并遵守总限额；需求冲突（更高频率/不同变换）按公布规则分离（各自采集器），不经隐式协商。
- **实际频率可查**：stats 含 actual_fps / last_received_at_ms / consecutive_failures / newest_age_ms；相机达不到配置频率时报告实际值；**过期图片不刷新时间戳冒充新图**。
- 原 JPEG 字节可直接输出；需要像素时再解码，缩放按需进行。

内部缓存回收约束（R3，兼容修复）：最近图片只归属于仍活跃的源×采样频率；最后一个同频率订阅排空时释放该频率缓存，不要求其他频率的消费者退出。停止期间尚在完成的HTTP请求不能重新留下无主缓存；缓存更新不允许旧帧覆盖更新的帧。不改变共享采集器、输出时间戳或外部图片release语义；共享Raw仍保守按既有HTTP预留额度计费，不承诺该额度等于RSS。

### 2.8 独立解码器（N01/SDK-DEC-02；与网络视频源解耦）

已在外部取得标准 H.264/H.265 压缩数据的调用方 MUST NOT 被要求为解码而重新连接相机；解码器父对象为 ctx，不依赖任何 amc_source_t。最小调用顺序：**create → submit → receive（零/一/多帧）→ drain/reset → close（屏障）→ 释放**。

```c
AM_API am_status_t amc_decoder_create(amc_context_t* ctx, const am_decoder_config_t* cfg, amc_decoder_t** out_dec);
/* 提交压缩数据：输入单位=完整访问单元（AU）或带边界声明的包；借用至本函数返回（接受即复制入
   解码队列）。返回二分（N01）：AM_OK=已接受（不得重复提交同一数据）；
   sdk.queueFull=未接受（输入未被引用，可原样重试） */
AM_API am_status_t amc_decoder_submit(amc_decoder_t* dec, const am_decoder_input_t* pkt);
/* 取输出帧：一次 submit 可产零/一/多帧，循环调用取尽；timeout_ms=0 立即返回。
   无输出且未排干 → sdk.tryAgain（正常节奏信号，非错误）；drain 后无更多输出 → sdk.drained */
AM_API am_status_t amc_decoder_receive(amc_decoder_t* dec, uint32_t timeout_ms, amc_frame_t** out_frame);
/* 输入结束进入排空：此后不再 submit，继续 receive 直至 sdk.drained；排空的空输出=已排干非错误 */
AM_API am_status_t amc_decoder_drain(amc_decoder_t* dec);
/* 重置：清空内部缓冲并（可选）换配置；decoder_generation 递增，旧帧身份不混同 */
AM_API am_status_t amc_decoder_reset(amc_decoder_t* dec, const am_decoder_config_t* new_cfg);
/* 关闭屏障（三段式）：CLOSED 后输入缓冲不再被引用；帧对象仍按各自 release 释放 */
AM_API am_status_t amc_decoder_close(amc_decoder_t* dec, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
/* 状态：READY/DRAINING/DRAINED/RESET_PENDING/CLOSING/CLOSED + 代次 + 队列用量 */
AM_API am_status_t amc_decoder_get_status(amc_decoder_t* dec, am_decoder_status_t* out_status);
/* 信息查询（P7）：配置与实际解码路径快照——声明的输出像素格式解析结果、hw_policy、
   实际软硬件（hw_accelerated/backend_name_utf8，以实际为准不以配置冒充）；不含运行态 */
AM_API am_status_t amc_decoder_get_info(amc_decoder_t* dec, am_decoder_info_t* out_info);
```

`am_decoder_config_t`（附录 §2.6）与语义合同：

- **codec/封装**：H.264 或 H.265 各自声明 `bitstream_format`（Annex B / 长度前缀），**支持集与 §2.1 能力位对齐**（AMC_CAP_BSF_* 各位声明，未开位即拒，P7）；**不支持任意切碎字节流**——提交单位必须为完整 AU 或带边界声明的包，否则拒绝（`camera.codecUnsupported` 附说明），不让调用方猜测边界。
- **输出像素格式与软硬件策略（P7）**：create 时声明 `output_pixel_format`（am_pixel_format_e；`AM_PF_ANY`=SDK 按能力自选，解析结果经 get_info 查询）与 `hw_policy`（AM_DHW_PREFER_HW 默认——硬解失败回退软解须过 §2.4 软件回退预算准入；AM_DHW_SOFTWARE_ONLY 强制软解）；实际解码后端经 `amc_decoder_get_info` 查询，不以配置值冒充实际值。
- **状态机（P7）**：READY →（drain）DRAINING → DRAINED →（reset 清空中）RESET_PENDING → READY →…→ CLOSING → CLOSED；RESET_PENDING 期间 `submit` 返回 `camera.decoderStateInvalid`，完成回 READY 时 `decoder_generation` 已递增；CLOSING/CLOSED 走 §1.1 两阶段。
- **参数集**：create 时可带 extradata（H.264 SPS/PPS；H.265 VPS/SPS/PPS）；或经输入包 PARAMSETS 标志内联携带；两种方式均可，配置变化递增 config_generation。
- **随机访问点与预滚**：首版从携带完整匹配参数集的关键帧开始输出（预滚/参考链管理内部处理）；**不把任意标记 keyframe 的包当安全恢复点**；H.265 按其档案的随机访问语义（IDR/CRA/BLA 及预滚）判定。
- **时间基**：config 声明 time_base；输出帧 pts 按提交输入的 PTS 透传（无 PTS 输入则帧 pts=INT64_MIN，clock_mask 如实）。
- **输出帧身份（B12）**：输出 `amc_frame_t`（与 §2.6 同句柄，可 retain/get_view/get_meta/as_pixel_view 喂推理）；meta 中 `source_id`=解码器实例 ID、`source_generation`=decoder_generation（reset 递增）、frame_id 进程内唯一——解码后帧与输入包**不假设一包一帧**。
- **厂商私有混合流不默认兼容**：检测到非标准 H.264/H.265 数据明确拒绝并说明（`camera.codecUnsupported`），不承诺解析。
- **预算**：解码队列/输出帧计入本资源域预算（§6），软解与冻结硬件矩阵分别验证（SDK-G3），不以浏览器可播放证明原生解码器可用。

### 2.9 事件回调（状态/代次/重同步；B12 影响范围）

```c
/* 源级事件：连接状态变化、重连、重同步要求、代次递增；设 NULL=注销（屏障语义同 unsubscribe）；
   回调线程/重入约束见 §5.3 */
AM_API am_status_t amc_source_set_event_callback(amc_source_t* src, am_source_event_fn cb, void* user_data);
typedef enum am_source_event_e { AM_EVENT_STATE_CHANGED = 0, AM_EVENT_GENERATION_CHANGED,
  AM_EVENT_ENCODED_RESYNC, AM_EVENT_SPATIAL_CHANGED, AM_EVENT_AUTH_WAITING } am_source_event_e;
typedef void (*am_source_event_fn)(amc_source_t* src, am_source_event_e kind,
                                   const am_event_detail_t* detail, void* user_data);
```

- STATE_CHANGED=连接/重连状态（含 AUTH_WAITING 进入：最终认证失败，等凭据更新，B13）；GENERATION_CHANGED=代次递增（旧检测/叠层应失效）；ENCODED_RESYNC=慢订阅者重同步；SPATIAL_CHANGED=视场/裁切/方向变化（ROI 不承诺沿用）。
- **事件注明影响范围（B12）**：`am_event_detail_t`（附录 §2.8）含 `scope`（SINGLE_SUB/SOURCE/ALL_CONSUMERS）与 `affected_handle`（受影响订阅句柄；scope=SINGLE_SUB 时有效）——某个慢订阅重同步不误伤其他消费者，调用方可精确失效对应叠层；含 `fault_class`（B13 分型）与脱敏 reason。

### 2.10 异步请求查询与取消（相机侧；B03 快照）

```c
/* 终态快照查询（P3 三轮改快照句柄制）：Pending/Running → Done/Failed/Canceled/Expired/Replaced 恰一；
   返回 DLL 拥有的快照句柄 am_request_snapshot_t*（state + 不可覆盖错误详情 + 时间戳，B03/B09，附录 §1.3）；
   终态一经写入不可覆盖；保留期内可重复 query（每次移交独立句柄引用，内容一致），用毕 release；
   已移交句柄至 release 前内容冻结可读（含 error 字符串）——请求记录 TTL 出账不影响已移交快照，
   彻底消除「出账期间读取字符串失效」窗口；请求已出账（曾接受且保留期过）→ sdk.resultExpired，
   从未存在/不属本 ctx → AM_REQ_UNKNOWN（sdk.requestUnknown）；
   释放（P3 四轮分库）：快照句柄必须回**分配方库**的释放入口——相机侧 amc_request_snapshot_release；
   原公共 am_request_snapshot_release 拆分为 amc_/amv_ 两入口（结构类型两库同构，但句柄归属
   各自 DLL；把推理库句柄交 amc_ 释放或反之返回 sdk.handleInvalid），公共 am_ 版本删除 */
AM_API am_status_t amc_request_query(amc_context_t* ctx, uint64_t request_id, am_request_snapshot_t** out_snapshot);
AM_API am_status_t amc_request_snapshot_release(am_request_snapshot_t* snapshot);
/* 请求取消：只表达意图；终态以 query 为准（取消与完成竞态时单终态） */
AM_API am_status_t amc_request_cancel(amc_context_t* ctx, uint64_t request_id);
```

### 2.11 诊断统计与资源限额（B10）

```c
/* 读当前限额（冻结包络内的实例值，am_limits_t 完整定义见附录 §4） */
AM_API am_status_t amc_get_limits(amc_context_t* ctx, am_limits_t* out_limits);
/* 收紧限额：只能在冻结包络内调低（§6），超出返回 sdk.limitOutOfRange */
AM_API am_status_t amc_set_limits(amc_context_t* ctx, const am_limits_t* limits);
/* 统计（am_stats_t 完整定义见附录 §4）：队列深度/丢弃/帧龄/字节占用/重连计数/拒绝累计/
   连续抓图实际频率/解码器用量；scope_utf8=NULL 全局或 "source:<id>"/"sub:<id>"/"capture:<id>"/
   "decoder:<id>"；输出经 DLL 脱敏；随 amc_stats_release 释放 */
AM_API am_status_t amc_get_stats(amc_context_t* ctx, const char* scope_utf8, am_stats_t** out_stats);
AM_API am_status_t amc_stats_release(am_stats_t* stats);
/* 对象级错误详情（最近一次以该句柄失败的调用；am_error_info_t：码/UTF-8 消息/来源/可重试）；
   异步错误走 request_query 快照，不经 last_error */
AM_API am_status_t amc_get_last_error(const void* any_handle, am_error_info_t* out_info);
/* 数值错误码 → 稳定字符串名（无 ctx 可调） */
AM_API const char* amc_status_name(am_status_t status);
```

`am_stats_t` 中 **SDK 可管理字节硬限用量与原生库（FFmpeg/驱动）内存观测分开字段**（B10）：前者受 §6 预算约束，后者（`native_rss_estimate_bytes`）为参考观测值，不承诺原生库总 RSS 严格受控。

R3独立解码提交计数修复：有效解码句柄的每次`amc_decoder_submit`，实际复制入队后累计一次`accepted`，因输入队列数量/字节、同步竞争或域字节不足而返回QueueFull/ResourceExhausted时累计一次`rejected_backpressure`；参数、句柄、状态、编码校验错误不计入背压。统计为Context累计，不随reset清零；混合取流/抓图时accepted还包含其原有入队/采集口径，不把Context总数等同独立解码提交次数。本项不改变任何入队、错误码或ABI布局。

Media订阅的对象、队列引用槽和缩放预设容量属于控制元数据，构造前占用`AM_Q_CONTROL`字节/项额度，直到对象最后引用释放才归还；队列的图片/包载荷继续按原数据额度计费，不重复把引用槽当载荷。低控制预算下可明确ResourceExhausted，即使该订阅的载荷预算尚有余量；关闭不能提前释放仍被对象持有的元数据额度。

## 3. am_vision.dll 函数清单

### 3.1 能力与后端矩阵（B14：单一 ORT 运行库 + 四层后端报告）

```c
/* 能力位：图像入口/张量入口/raw 模型加载/内存模型加载（P1 开关位）/批量/原始张量出口/通知回调/
   图像前处理（raw 模型下独立声明）；dtype 位集；档案集摘要哈希（§4 表变更可检测） */
AM_API am_status_t amv_query_capability(amv_context_t* ctx, amv_capability_t* out);
/* 后端四级状态矩阵：硬件探测/依赖存在/provider 可加载/本包已验证；含同进程组合冲突行 */
AM_API am_status_t amv_query_backends(amv_context_t* ctx, am_backend_matrix_t* out);
/* 运行库重试（P9）：init 运行库加载失败不把进程置为永久不可用——进程内无存活 ctx 时可带新
   runtime_dirs 显式重初始化，成功后 amv_init 恢复可用；仍有存活 ctx → sdk.invalidArgument */
AM_API am_status_t amv_backend_retry(const amv_init_params_t* params);
```

**ORT 运行库共存合同（B14）**：

- **每进程单一兼容 ORT 运行库**：显式提供runtime_dirs的amv_init首次校验并加载onnxruntime，成功后锁定路径/版本；用户确认的例外是显式AM_INIT_SHARED_FRAMES且runtime_dirs为空，仅共享帧功能可用、模型入口报runtimeMissing，不要求ORT；本 ctx 及后续模型共用同一运行库实例，以**多个 Execution Provider 会话**覆盖 CPU/加速组合（每模型一个会话，EP 按后端策略分配）。
- **初始化失败恢复（P9）**：首次加载失败 → 进程标记 backendUnavailable，后续 amv_init 返回 `vision.runtimeMissing`/`runtimeLoadFailed`（详情经 `amv_get_init_error`，§1.2）；显式 `amv_backend_retry`（可带新 runtime_dirs）在**进程内无登记ctx（包括CLOSED未destroy项）**时重试初始化，成功清除标记——不要求重启进程，也不在仍有 ctx 存活时静默换运行库。
- 检测到进程内已存在不兼容 ORT（宿主自行加载了不同版本/位数）→ `vision.backendConflict`（错误详情注明冲突版本与来源），明确拒绝，不悄悄替换已在用的运行库；"每进程多套 ORT 并存"不在首版承诺内。
- 冻结候选组合：CPU-only / CPU+OpenVINO / CPU+CUDA（**CPU+单加速后端**；跨加速组合 backendConflict，§9-O9）；初始化顺序矩阵（先探测→加载 EP→建会话；严格/允许回退两列；正反顺序逐行验证）归 SDK-G3 实测留证，未测行标未验。

**后端四层区分（B14）**：①请求后端（`am_backend_policy_t.requested`）→ ②实际 provider（`am_backend_actual_t.actual`+device）→ ③子图 CPU 回退（`am_backend_actual_t.subgraph_cpu_nodes`，int32：**-1=provider 未报告/不可获取，0=无回退，>0=被分到 CPU 的节点/子图数**——P9：不把「不可获取」与「零回退」混同）→ ④失败/回退原因（`fallback_reason_utf8`）。加载成功但部分子图在 CPU 执行时如实报告，不以设备名宣称全 GPU 执行。

`amv_capability_t`（附录 §3.1）feature_bits：`AMV_CAP_IMAGE_INPUT | AMV_CAP_TENSOR_INPUT | AMV_CAP_RAW_MODEL | AMV_CAP_MEMORY_MODEL(P1) | AMV_CAP_IMAGE_PREPROCESS_RAW | AMV_CAP_BATCH | AMV_CAP_RESULT_RAW_TENSOR | AMV_CAP_PUSH_NOTIFY | AMV_CAP_SHARED_FRAME_POOL`；dtype_bits=AM_DTYPE_* 位集（§9-O5）；max_batch_items / max_models / infer_parallelism / result_retention_default_ms；`shared_pool_state` 与相机侧同构三态四值（可用/未启用/未编入/平台不支持，SDK-DEC-07，附录 §1.6）。`am_backend_matrix_t`行含hw_detected/deps_present/provider_loadable/verified：provider_loadable只在本进程真实会话成功后为1，verified只在该后端真实推理成功后为1；未观察时0且note注明未知，不把未知报成已失败，也不在查询时偷偷加载并选定某加速后端。包级已验证矩阵仍另行留证。OpenVINO首版固定CPU并显式配置线程，CPU子图回退不可观测时仍为-1。

### 3.2 模型加载（B05 档案/raw 双模式；路径+内存字节）与信息

```c
/* 路径加载：desc.load_mode 二分——
   AM_LOAD_PROFILE：profile_id 必须显式给出或模型元数据可唯一识别（§4），否则 vision.profileRequired
     （禁止猜测后静默出错）；
   AM_LOAD_RAW：generic 模式，不要求任何 YOLO 档案，加载任意 ONNX 数值模型（dtype 在冻结集合内），
     结果仅原始张量出口（task=generic，§3.6），图像后处理能力独立声明（见下）。
   strict 失败 → vision.backendStrictUnavailable；运行库/组合冲突 → vision.backendConflict（§3.1） */
AM_API am_status_t amv_model_load(amv_context_t* ctx, const am_model_desc_t* desc, amv_model_t** out_model);
/* 内存加载（P1）：能力位开才可用，未开返回 sdk.notSupported；bytes 借用至本函数返回（接受时复制），
   大小上限见 §6；初始化失败时本次已接管资源全部释放 */
AM_API am_status_t amv_model_load_memory(amv_context_t* ctx, const am_model_memory_desc_t* desc, amv_model_t** out_model);
/* H5 r9：旧desc.backend须NULL；旧入口原有requested/strict语义保持。
   候选/字节/路径仅借用到返回；attempt_id与model输出不可别名，须不同可写地址。
   校验/预算拒绝前attempt_id=0；准入后成功/失败均返回非零ID，失败model=NULL。 */
AM_API am_status_t amv_model_load_selected(amv_context_t* ctx, const am_model_desc_t* desc,
    const am_backend_selection_t* selection, amv_model_t** out_model, uint64_t* out_attempt_id);
AM_API am_status_t amv_model_load_memory_selected(amv_context_t* ctx, const am_model_memory_desc_t* desc,
    const am_backend_selection_t* selection, uint32_t intra_threads,
    amv_model_t** out_model, uint64_t* out_attempt_id);
/* UTF-8 JSON+NUL；required含NUL，0容量只测长；上限8192字节，短缓冲不截断JSON。
   读缓存，不探测/建Session、不等待正在运行的ORT。关闭态只返回空状态。 */
AM_API am_status_t amv_get_model_selection_report(const amv_model_t* model, char* buffer, uint64_t capacity, uint64_t* required);
AM_API am_status_t amv_get_selection_attempt_report(amv_context_t* ctx, uint64_t attempt_id, char* buffer, uint64_t capacity, uint64_t* required);
/* 关闭（三段式）：在途请求各自到达终态后释放；新旧模型身份独立，旧结果不冒充新版本（§3.7） */
AM_API am_status_t amv_model_close(amv_model_t* model, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
/* 模型签名与状态：命名输入输出/dtype/shape（动态维度+公布范围）/native_batch 声明/加载模式/
   档案 ID/实际执行后端+回退原因+子图回退；只读并发安全 */
AM_API am_status_t amv_model_get_info(const amv_model_t* model, am_model_info_t* out_info);
```

**H5 r9有序选择**：新能力位`AMV_CAP_ORDERED_SELECTION`，ABI1.1本地未发布演进，旧结构/导出不变。候选CPU=0/OpenVINO=1/CUDA=2；非CUDA device_id必须0，CUDA≤INT32_MAX，最多3项。allow_fallback=0只试首项；1也只访问允许列表，预算到期不再试下一项，不强杀已经进入ORT的调用。文件/档案/模型约定错误不靠更换设备重试。OpenVINO仍CPU；单ORT与单加速域约束保持。

选择快照schema=`framebraid.inference-selection/1`，包含state、attempt_id、outcome_status、observed_at_ms、selection（Rust有序候选/尝试阶段/原生代码/实际target/Run计数/placement_verified）。context仅缓存最近开始的一次尝试，旧尝试完成不覆盖更新ID；覆盖或未知ID返回sdk.resultExpired，调用方用自身load返回ID防止并发串读。成功模型拥有独立缓存，即使context缓存被覆盖也可查；旧模型入口的selection为null（legacy_model），继续用旧get_info，不能虚构旧模型有新诊断。Session成功与真实Run分开，Run之后才刷新计数；计数反映core输出，不等于SDK结果被业务消费。GPU节点放置未知不报已全GPU。

2026-10-27逐候选资源准入：有序加载只为当前候选调整线程预留；`resource_admission`阶段的`sdk.resourceExhausted`可在allow_fallback允许时继续下一明确候选，不能以此放宽control/模型/全局总额，不能暗加CPU。strict和legacy加载行为保持，原生构造返回前不调整下一候选；后续Run失败只结束当前请求，显式关闭/重建才产生新模型身份。

新选择每个context尝试缓存8KiB，每个新策略模型缓存8KiB，查询临时16KiB，均按既有control项数/字节准入；超额resourceExhausted，不增加预算或线程。初始化/模型失败也保存已准入尝试的稳定错误及阶段，无路径/原生消息。关闭释放模型缓存、停止后查询不分配工作服务；context缓存不作为外部持有阻塞关闭。运行期失败只结束当前请求，不自动重放；恢复由显式排空/关闭旧模型再加载新身份，完整故障/真实加速矩阵仍按H5后续验收。

**raw 模式语义（B05）**：

- 加载不要求 YOLO 档案、不做业务后处理猜测；模型元数据仅用于形状/名称校验，**不可靠识别时仍可 raw 加载**（与 profile 模式的 profileRequired 互补，两条路径都走得通）。
- **图像后处理能力独立声明**：raw 模型的图像入口默认**不可用**（SDK 不知道前处理约定）；调用方可随 infer 请求显式提供前处理描述（`am_infer_params_t.raw_preprocess`：letterbox/resize 目标、填充值、颜色序、归一化、布局，附录 §3.2），SDK 按声明执行并回填 transform 元数据（含 STRETCH 非等比 scale_x/scale_y 与 CENTER_CROP 裁切偏移，P8）；不提供则图像入口返回 `vision.preprocessRequired`。张量入口不受影响（永不隐式前处理）。
- **路径与内存模型一致语义（B05）**：两种加载均支持 PROFILE/RAW 两模式；ONNX **external-data** 解析基准：默认只解析与主模型同目录的外部权重文件（路径模式）或拒绝（内存模式，`vision.externalDataUnsupported`），不默认从工作目录找外部权重；安全范围与拒绝口径随档案表冻结（§9-O3）。
- `am_model_desc_t` / `am_model_memory_desc_t`（附录 §3.2）在原字段上增加 `load_mode`；`am_model_info_t` 增加 `load_mode` 与 `execution.subgraph_cpu_nodes`（B14 第③层）。

### 3.3 图像推理（同步/异步；内置前处理；B06 deadline / B12 关联）

```c
/* 同步单帧：JPEG/PNG 字节或跨 DLL 像素视图（§5.6）；PROFILE 模型按 §4 档案完成解码/letterbox/
   颜色布局/归一化，结果含坐标变换元数据（映射回原图）；RAW 模型按 params.raw_preprocess（§3.2）。
   deadline：params.sync_deadline_ms（0=默认 30s）。超时返回 sdk.deadlineExceeded 且返回后 SDK 不再
   引用输入缓冲（B06）；默认策略下该项工作继续在后台执行——SDK 已在返回前完成安全复制，并经
   out_request_id（非 NULL 时）返回可查询身份，调用方可 amv_request_query 追终态；
   out_request_id 传 NULL=不需要追踪（语义不变，只是拿不到身份） */
AM_API am_status_t amv_infer_image_sync(amv_model_t* model, const am_image_input_t* input,
                                        const am_infer_params_t* params, amv_result_t** out_result,
                                        uint64_t* out_request_id);
/* 异步单帧：默认复制输入；flags 选 AM_INPUT_BORROW_RETAINED 时显式保留拥有者至工作实际结束
   （§5.6）；预算满 sdk.queueFull（未接受）；异步错误经 amv_request_query 快照查询（B03） */
AM_API am_status_t amv_infer_image_async(amv_model_t* model, const am_image_input_t* input,
                                         const am_infer_params_t* params, uint64_t* out_request_id);
```

- `am_image_input_t`（附录 §3.3）：kind（JPEG/PNG/PIXEL_VIEW）/encoded+size/view/flags，新增**可选关联元数据 `assoc`**（`am_frame_assoc_t*`，NULL=无；B12：外部图像可携带 source_id/frame_id/代次/PTS/时间基/采集时间，P5 补接收时间 received_at_ms 与 clock_quality；PIXEL_VIEW 模式下若未提供则取视图自带 frame_id/代次），结果原样回填（`amv_result_t.frame`，指针），供多模型/多源结果关联。
- `am_infer_params_t`（附录 §3.3）：confidence_threshold（档案默认外覆盖）/sync_deadline_ms/slot_key（B11）/assoc 透传/raw_preprocess（RAW 模型）/flags。

### 3.3a GPU 帧输入（M2b，未发布兼容扩展）

图像推理输入新增 GPU 帧直传模式：`am_image_input_t.kind=AM_IMG_GPU_FRAME` + 尾部字段
`const am_gpu_frame_ref_t* gpu`（struct_size 协商；旧调用方零感知）。

**跨 DLL 帧引用（`am_gpu_frame_ref_t`，值结构）**：宿主经 camera 侧查询导出组装——
`amc_gpu_frame_desc`（元数据）+ `amc_gpu_frame_texture`（同进程纹理指针，借用期内有效，
签名见 §2.6a 导出清单）；纹理指针为主交付物（M2a 2026-10-29 实测修订：Win11 25H2
runtime 拒绝纹理 QI DXGI 接口，`shared_nt_handle` 平台支持时尽力提供、可为 0）。

**语义**：
- 同进程约束：`texture` 指针仅在 GPU 帧租约借用期内有效（vision 侧消费完毕前宿主不得
  归还/释放帧）；同步调用借用至返回，异步借用至请求**接受**（GPU 预处理在接受段同步
  完成，纹理不跨异步生命周期——与 encoded 输入同借用语义）
- 预处理在 GPU 完成（CUDA kernel：NV12 采样→模型输入格式 resize/letterbox/normalize，
  按档案输入形状），经 IOBinding 直达 CUDA EP 输入张量；全程零 CPU 回读
- **显式输入不静默降级**：kind=GPU_FRAME 而模型会话不支持（非 CUDA EP/互操作不可用/
  输入格式不支持）返回 `notSupported`；回退是宿主决策（能力查询后改用 PIXEL_VIEW——
  像素路径永远可用，构成自然回退），SDK 内部不做隐式转换（隐式转换需回读，正是本
  扩展要消灭的路径）
- 引擎适配位：首版实现 ORT CUDA EP（IOBinding）；OpenVINO 原生引擎（E1）落定后按本
  输入合同适配，接口不变
- 互操作生命周期：纹理注册/map/unmap 全程发生在借用窗口内，不跨租约持有（M2a 租约
  语义不变）
- 搬运审计（模型级可查，M2c 验收主指标）：`gpu_inputs`（GPU 直传帧数）、
  `input_copies_per_frame`（GPU 路径每帧 GPU 内拷贝次数）、`input_cpu_fallbacks`
  （宿主显式改用像素路径不计入——此计数器只记 SDK 内部实际发生的回退性搬运）

**能力位**：`AMV_CAP_GPU_FRAME_INPUT`（编译编入 + 当前运行库 CUDA 互操作入口可用的
双条件；查询不触发 CUDA 初始化）。

### 3.4 张量推理（命名多输入多输出；不隐式前处理；B06/B12）

```c
/* 同步张量：按名称匹配输入（不猜位置）；直接进执行器，不做颜色/布局/letterbox/归一化；
   deadline 与超时所有权语义同 §3.3（B06：返回后不引用输入；继续执行则已复制+可查询身份）；
   params 同图像入口（slot_key/关联透传/confidence_threshold；NULL=默认，P6）——实时槽与公平调度
   同样覆盖张量入口，无入口例外；
   校验失败（名称/维度/元素数/字节/dtype/连续性）请求级返回 vision.input* 系列，不从越界内存补齐 */
AM_API am_status_t amv_infer_tensors_sync(amv_model_t* model, uint32_t input_count,
                                          const am_tensor_in_t* inputs, const am_infer_params_t* params,
                                          amv_result_t** out_result, uint64_t* out_request_id);
/* 异步张量：语义同上；预算/终态/错误查询同 §3.3 */
AM_API am_status_t amv_infer_tensors_async(amv_model_t* model, uint32_t input_count,
                                           const am_tensor_in_t* inputs, const am_infer_params_t* params,
                                           uint64_t* out_request_id);
```

- 首版限数值稠密张量：字符串/Sequence/自定义算子输入明确拒绝（`vision.tensorTypeUnsupported`/`vision.ioUnsupported`）；dtype 冻结集合 F32/F16/I64/I32/U8（§9-O5）。
- `am_tensor_in_t`（附录 §3.3）新增可选 `assoc`（B12：张量输入同样可关联帧身份，结果回填）。
- 张量入口的 `raw_preprocess` 不读取、不执行，`transform=NULL`。PROFILE 的几何结果以档案输入张量平面为归一化坐标系；调用方已做过 letterbox/crop 时，自行用其变换还原原图。`assoc` 只关联身份与时间，不承载原图尺寸或空间变换，不能据它伪造原图坐标。图像入口仍由 SDK 完整还原原图。
- 同一命名输入组允许部分输入不带 `assoc`；显式提供的多个 `assoc` 字段须一致（忽略 `struct_size`），不一致返回 `sdk.invalidArgument`，不按数组顺序选择最后一个。同步/异步均在预算准入后复制数值及元数据；标量允许 `shape_len=0/shape=NULL`，连续标记须为 `AM_TENSOR_CONTIGUOUS`。

### 3.5 批量提交（B11 逐项回执；显式 batch vs 拆分单帧）

```c
/* 有界批量：逐项回执 accepted/request_id/拒绝原因——离线队列不静默丢旧（§5.5）；
   batch_mode 三值语义（P6）：NATIVE=声明模型原生批量，SDK 按模型批量能力组批 /
   SPLIT=全部逐项拆分执行（每项独立 requestId 与终态，不冒充原生 batch；仅对接受 batch=1 输入的
   模型合法——动态批量轴或无批量轴模型）/ AUTO=按模型能力自动选择（动态批量→NATIVE；
   固定批量→NATIVE；无批量→SPLIT）；
   尾项规则（P6 三轮）：固定批量轴（native_batch=N≥2）不足完整一批的尾项**明确拒绝并逐项回执**
   vision.insufficientBatchRejected——固定批量模型（如 [4,3,640,640]）不接受 batch=1 输入，
   以 SPLIT 单帧提交尾项违反模型输入形状，禁止；填充组批（pad 至整批+推理后剔除填充项）列后续
   候选不随首版；NATIVE 且模型无批量能力（native_batch=0）→ 整批拒绝 vision.batchUnsupported；
   自动攒批不属于首版；异步错误逐项经 amv_request_query 快照查询（§3.7 句柄制） */
AM_API am_status_t amv_infer_batch(amv_model_t* model, uint32_t item_count, const am_batch_item_t* items,
                                   const am_batch_options_t* options, am_batch_receipt_t* out_receipt);
AM_API am_status_t amv_receipt_release(am_batch_receipt_t* receipt);
```

- **取消粒度逐项（B11）**：batch 提交的每一项都是独立 request_id，`amv_request_cancel` 逐项取消，不整批连带；某项校验失败只拒该项（receipt.rejects[i]），不影响已接受项。
- `am_batch_item_t`：image 或 tensors 二选一（含各自 assoc，B12）+ **逐项 slot_key（P6：0=全局槽，公平与替换同样覆盖批量入口）** + **逐项 raw_preprocess（P6：RAW 模型图像项必填的逐项前处理描述；PROFILE 模型忽略；单帧入口用 am_infer_params_t.raw_preprocess）**；`am_batch_options_t`：batch_mode（NATIVE/SPLIT/AUTO 三值，P6）+wait_deadline_ms；`am_batch_receipt_t`：accepted_count + request_ids[]（被拒项=0）+ rejects[]（逐项背压/校验原因）。完整定义见附录 §3.4。
- **合法批次组成（P6 三轮修订）**：动态批量轴（native_batch=1）任意 item_count≥1（受 dyn_max 与 max_batch_items 约束，超限自动拆多批）；固定批量轴（native_batch=N≥2）逐 N 项组批，**尾批不足 N 时尾项明确拒绝**——逐项回执 `vision.insufficientBatchRejected`（receipt.rejects[i]），不静默丢项、不以 SPLIT 单帧伪装成合法提交（违反模型固定输入形状）、不自动填充；调用方须自行凑满整批或改用接受单帧的模型。AUTO 落到固定批量模型时同样按本规则拒绝尾项。
- **首版批量轴证据**：通用RAW模型仅在ONNX自定义元数据明确含`am.sdk.batch_axis=0`，且所有命名输入/输出均有兼容的共同首轴时提供原生组批；动态首轴为`-1`、范围1..64，固定首轴N须1..64并一致，固定1仍报告native_batch=0。未声明时保持通用张量能力，native_batch=0，不根据维名或shape猜测；声明为其他轴、空值或与签名矛盾时明确拒绝。该声明是模型作者的合同，不证明模型逐样本独立，允许跨batch计算；执行后仍校验所有输出实际首轴与输入批次一致。不同轴布局保留无该声明的RAW张量入口，不冒充已实现原生组批。
- **同源核心组批预算**：一次物理批次的合并输入与ORT输入副本，以及完整输出与逐项拆分副本，分别计其8字节对齐数值缓冲并存峰值；不得仅按最终拆分结果占用计费。调用方原有输入、模型及ORT/驱动内部工作区另计，数值缓冲上限不能宣传为进程RSS硬上限。源码`Model::run_batch`只执行一个物理批次；DLL批量入口另负责多组调度、逐项身份、限时等待和回执，不以核心用例代替DLL验收。
- **批量回执状态与所有权**：`AM_OK`表示已生成完整逐项回执，可能`accepted_count=0`；接受项有非零ID且rejects[i]=AM_OK，其余ID=0并给具体原因，包括模式不兼容与固定尾项。顶层参数/回执资源失败才返回函数级错误并清空输出；已经接受的项不因后项失败回滚。ids/rejects为SDK拥有的只读数组（不能修改或自行free），可移动调用方回执头后调用`amv_receipt_release`；该函数清空回执头，重复释放报handleInvalid。释放回执不取消已接受请求，也不清除其结果。回执与快照共享控制类项/字节预算，持有期间ctx不进入CLOSED。
- **物理批次与背压**：NATIVE按兼容的命名输入shape分组，固定N逐N项，动态组受公开64项包络。不得因临时队列/槽或字节不足，静默缩小原生组或改为SPLIT，否则可能改变跨batch运算结果；完整组只能接受、拒绝或限时等待。已接受离线项不被后来的实时请求替换；逐项取消不从正在运行的原生组移除输入，只改变该项终态，其余项保持同一原生运算。等待者域最多16且受ctx队列更小上限约束，wait=0不等，非零不超过配置/5s；截止后不得迟到接受，复制/保留引用后再次核对截止。调用方回调须及时返回，SDK不承诺抢占阻塞的外部回调；调用方还须使结果TTL覆盖其等待/取结果节奏。

- **自适应攒批（未发布兼容扩展，R5-2.1）**：显式启用、可关闭、有界的跨请求自动聚合，仅两个新导出，不改既有结构与语义：
  ```c
  typedef struct am_batching_options_s {
    uint32_t struct_size; uint32_t abi_minor;  /* 版本协商同全局规则 */
    uint32_t max_batch_items;                  /* 2..64；0=取模型动态上限 */
    uint32_t max_wait_ms;                      /* 0..5000；0=凑满即发不等 */
    uint32_t flags;                            /* AM_BATCHING_ENABLE=1 */
  } am_batching_options_t;
  AM_API am_status_t amv_model_configure_batching(amv_model_t* model, const am_batching_options_t* options);
  AM_API am_status_t amv_model_get_batching(amv_model_t* model, am_batching_options_t* out, am_batching_stats_t* stats);
  ```
  `am_batching_stats_t { struct_size, abi_minor, formed_batches, batched_items, timeout_batches, rejected_items }` 为累计只读计数。语义边界：
  - **默认关闭**：未 configure 或传 NULL 时，AUTO/NATIVE/SPLIT 全部维持现行逐次执行语义，零攒批；旧调用方零影响。
  - **适用范围**：仅动态批量轴模型（native_batch=1 且动态上限≥2）接受启用；固定批量轴模型 configure 返回 `vision.batchUnsupported`，不填伪样本、不放宽尾项拒绝规则。
  - **聚合条件**：同一模型实例、相同实际后端选择、逐项输入 shape 与前处理参数完全一致才同批；不一致项不等待、立即按现行规则执行/拒绝，不静默变形。
  - **触发执行**：凑满 `max_batch_items`、或 `max_wait_ms` 窗口截止、或模型关闭排空时，按单一物理批次执行；窗口截止不足 2 项时不组批、逐项执行。
  - **逐项语义不变**：每项独立 request_id/取消/TTL/回调与错误回执（B11）；已取消项不进入后续批次；批内部分项失败不回滚同批其余项。
  - **资源与公平**：等待项计入模型既有请求队列与等待者上限，不新增无界队列；等待期不占推理线程；执行占用模型既有线程与控制额度，合并批次按"同源核心组批预算"计费。
  - **运行期切换**：configure 可在任何时刻调用；切换只影响此后接受的新请求，已进入等待窗口的在途项按切换前配置走完（凑满/截止/排空三择一）后生效。查询在关闭状态返回全零计数与默认选项。
  - 能力查询与实际支持一致：无动态批量能力的模型 get 返回 flags=0；绑定/生成器同步新导出与结构；本扩展不改变 batch_mode 既有三值语义——攒批只作用于 AUTO。

- **DirectML 后端（未发布兼容扩展，E2）**：`AM_BACKEND_DIRECTML=3` 作为第四推理路径（Windows 任意 GPU：Intel/AMD/NVIDIA）。矩阵第四行字段语义同其他后端（hw=运行库目录或系统含 DirectML.dll 且枚举到适配器；deps=d3d11+DirectML 运行库；loadable=ORT 会话以 DmlExecutionProvider 创建成功；verified=真实推理成功一次）。候选元组 device_id 忽略（EP 内部选卡，合同固定）；策略回退语义与其他后端一致。显式探测报告新增 directml 段（available；不承诺具体适配器型号）。不支持该值的旧绑定将收到 invalidArgument 而非未知崩溃。

### 3.6 结果获取（B02 五类结构 / B03 快照闭环 / B08 所有权）

```c
/* 完成通知回调：仅通知（request_id+终态），不携带数据；数据一律拉取；设 NULL=注销（屏障） */
AM_API am_status_t amv_result_notify_callback_set(amv_model_t* model, am_result_notify_fn cb, void* user_data);
/* 拉取结果：按 request_id 幂等（保留期内重复 fetch 不重复执行）；过保留期或已 ack →
   sdk.resultExpired（与仍在执行可区分）；通知丢失不影响可查询 */
AM_API am_status_t amv_result_fetch(amv_context_t* ctx, uint64_t request_id, amv_result_t** out_result);
/* 取走确认（B08 修正语义）：仅释放 SDK 缓存引用与结果保留预算；
   调用方已 fetch 持有的 result 继续计入外部持有预算（B10），直至 amv_result_release */
AM_API am_status_t amv_result_ack(amv_context_t* ctx, uint64_t request_id);
/* 引用计数（B08：保留/释放拥有者，不设计孤立 tensor retain）：
   fetch 移交调用方一个引用；跨异步任务持有须 amv_result_retain；release 归零由推理 DLL 释放。
   raw_outputs 数组与 mask_bits 等内部缓冲随 result 生命周期——长期持有张量=retain 整个 result */
AM_API am_status_t amv_result_retain(amv_result_t* result);
AM_API am_status_t amv_result_release(amv_result_t* result);
typedef void (*am_result_notify_fn)(amv_model_t* model, uint64_t request_id, uint32_t terminal_state, void* user_data);
```

**`amv_result_t` 结构（B02，完整定义见附录 §3.5）**：

```c
typedef enum am_task_e { AM_TASK_NONE = 0, AM_TASK_POSE, AM_TASK_DETECT, AM_TASK_CLASSIFY,
                         AM_TASK_SEGMENT, AM_TASK_OBB, AM_TASK_GENERIC } am_task_e;
typedef struct amv_result_s {
  uint32_t struct_size;
  uint64_t request_id;
  am_task_e task;                  /* 任务判别：仅与 task 匹配的出口有效，其余 count=0/指针 NULL */
  const am_frame_assoc_t* frame;   /* 输入关联（B12）：assoc 或像素视图携带；未提供=NULL
                                       （P2 指针化：可演进结构不按值嵌入） */
  am_model_identity_t model;       /* 模型身份/版本：同帧多模型、换代不串版 */
  am_backend_actual_t execution;   /* 实际后端+设备+回退原因+子图 CPU 回退（B14 四层之②③④）；
                                      固定布局子结构按值嵌入（P2：字段冻结，演进经本结构尾部追加） */
  const am_preprocess_meta_t* transform;  /* 实际前处理变换（P2 指针化；P8 补非等比 scale_x/y 与
                                      crop 偏移：STRETCH/CENTER_CROP 亦完整回填）；张量入口 NULL */
  am_pose_result_t pose;           /* 五类出口（B02）：pose/detect/classify/segment/obb */
  am_detect_result_t detect;
  am_classify_result_t classify;
  am_segment_result_t segment;     /* 掩膜：尺寸/步长/映射/所有权，附录 §3.5 */
  am_obb_result_t obb;             /* 旋转框：顶点/角度/周期，附录 §3.5 */
  uint32_t raw_output_count;       /* 命名原始张量出口（复核用；RAW 模型唯一出口） */
  const am_tensor_out_t* raw_outputs;
  int64_t infer_start_ms, infer_end_ms;   /* 耗时分段（可观测） */
} amv_result_t;
```

- **Segment（B02）**：`am_segment_result_t { count; instances[] }`；每实例 `am_segment_instance_t`：bounds/confidence/class_id + **掩膜完整可读结构**——`mask_width/mask_height`（掩膜自身尺寸）、`mask_stride_bytes`（行步长 ≥ (width+7)/8，1bit/px 位掩膜）、`mask_bits`（const 指针，随 result 生命周期；跨 result 持有须 amv_result_retain）、`roi_map`（掩膜像素→原图坐标映射：scale_x/y+offset_x/y+valid；去 letterbox 映射已由 SDK 完成）。**位序定死（P8）：LSB-first**——字节内最低有效位=行内最左像素（x=0 → bit0），行尾不足一字节高位补 0（无效位），跨行按 mask_stride_bytes 字节对齐；SDK 输出与调用方解读按同一位序执行，不设逐档案位序开关。掩膜字节计入结果预算（B10）。
- **OBB（B02/P8）**：`am_obb_result_t { count; instances[] }`；每实例 `am_obb_instance_t`：`vertices_xy[8]`（四顶点原图归一化坐标）、center/width/height（沿角度方向与垂直方向的局部量，归一化）、`angle`、confidence/class_id。**几何全局约定定死（P8 三轮公式化、四轮方向表述修正，不再「随档案冻结」）**：角度自原图 **x 正轴起、图像坐标系（y 向下）视觉顺时针为正**（弧度制；输出范围 **[-π,π]**）——公式 u=(cosθ, sinθ) 本身即数学正角（数学惯例：数学坐标系逆时针为正），其坐标值直接在图像坐标系使用：sinθ>0 即向图像 y 正方向/向下，故正角在 y 向下图像系呈**视觉顺时针**（四轮修正：删除原「逆时针为正」表述——其在 y 向下系与公式矛盾；公式与数值算例不变）。**首顶点公式定义（消除「沿宽轴走到边中点」歧义）**：记 u=(cosθ, sinθ) 为角度方向单位向量、v=(−sinθ, cosθ) 为其垂直方向单位向量、w=沿角度方向的边长、h=垂直方向边长，四顶点为

  - **V0 = center + u·(w/2) + v·(h/2)**（首顶点：角度方向边与遍历序下一条边的交点，即从中心沿角度方向前进 w/2、再沿垂直方向前进 h/2 到达的**角点**——仅沿宽轴走 w/2 到达的是边中点，不是顶点）
  - V1 = center − u·(w/2) + v·(h/2)
  - V2 = center − u·(w/2) − v·(h/2)
  - V3 = center + u·(w/2) − v·(h/2)

  遍历序 **V0→V1→V2→V3**（y 向下的图像坐标系中呈视觉顺时针，成对存储 xyxy），`angle_normalized` 恒为 1（SDK 已按本约定规范化输出）。**非正方形数值算例（防歧义，像素坐标；入档输出=像素值按原图宽高归一化）**：θ=30°、w=200、h=100、center=(320, 240)，则 u=(0.8660, 0.5000)、v=(−0.5000, 0.8660)、u·(w/2)=(86.60, 50.00)、v·(h/2)=(−25.00, 43.30)：

  | 顶点 | 公式 | 数值 |
  |---|---|---|
  | V0 | center + u·(w/2) + v·(h/2) | **(381.60, 333.30)** |
  | V1 | center − u·(w/2) + v·(h/2) | **(208.40, 233.30)** |
  | V2 | center − u·(w/2) − v·(h/2) | **(258.40, 146.70)** |
  | V3 | center + u·(w/2) − v·(h/2) | **(431.60, 246.70)** |

  验算：V0→V1=(−173.20, −100.00) 模长 200=w 且平行 u；V1→V2=(50.00, −86.60) 模长 100=h 且平行 v；V0V2 对角线中点=(320.00, 240.00)=center。档案仅声明「模型原始输出 → 本全局约定」的映射，不各自定义顶点序/方向/起始轴。旋转 NMS 由档案（obb-*-nms 内置；e2e 档案无）执行——**旋转相似度=probabilistic IoU（probiou；不是多边形交并比），按8.4.163的分类别Fast NMS（任一更高分框达到阈值即抑制，不等同贪心NMS）（P8 四轮声明），IoU 阈值默认 0.7，conf/max_det 默认同 detect（§4.2）**——结果为 NMS 后存留实例；几何由独立参考实现对照锁定（§4.2）。
- pose/detect/classify 出口结构沿用原稿（归一化坐标原图系；classify 的 logits/概率按档案，两者不混同）——完整定义收敛附录 §3.5。
- RAW 模型结果：task=AM_TASK_GENERIC，五类出口全空，仅 raw_outputs。
- JSON 只用于低频配置与诊断，不承载逐帧大图或大张量。

### 3.7 请求生命周期、实时槽与公平调度（B11）、模型换代

```c
/* 终态快照查询（B03；P3 三轮改快照句柄制，与 §2.10 相机侧同构）：Pending/Running →
   Done/Failed/Canceled/Expired/Replaced 恰一；返回 DLL 拥有的快照句柄（含不可覆盖错误详情），
   保留期内可重复 query、已移交句柄至 amv_request_snapshot_release 前内容冻结可读
   （P3 四轮分库：推理库句柄回推理库释放，跨库传入返回 sdk.handleInvalid）；
   请求已出账 → sdk.resultExpired；从未存在 → sdk.requestUnknown；
   取消意图经 amv_request_cancel，终态以快照为准 */
AM_API am_status_t amv_request_query(amv_context_t* ctx, uint64_t request_id, am_request_snapshot_t** out_snapshot);
AM_API am_status_t amv_request_snapshot_release(am_request_snapshot_t* snapshot);
AM_API am_status_t amv_request_cancel(amv_context_t* ctx, uint64_t request_id);
/* 实时策略（B11）：槽按"模型 × 调用方源/槽身份"隔离——slot_key 由调用方随请求提供
   （am_infer_params_t.slot_key，如 source_id；缺省=全局单槽）。替换仅发生在同槽内：
   同槽超 max_inflight 且 replace_oldest=1 → 最旧请求进 Replaced 终态（可查询不消失）；
   跨槽 fair_round_robin=1（默认）时按槽公平轮转调度，单源高频不得独占模型执行 */
AM_API am_status_t amv_set_realtime_policy(amv_model_t* model, const am_realtime_policy_t* policy);
```

`am_realtime_policy_t`（附录 §3.6）：max_inflight（每槽）/replace_oldest/slot_count（槽数上限，默认 1）/fair_round_robin。全部受执行器域共享并发预算约束（§6）。**槽与公平覆盖全部推理入口（P6）**：图像单帧（`am_infer_params_t.slot_key`）、张量单帧（同 params）、批量逐项（`am_batch_item_t.slot_key`）——同一套槽身份与调度规则，无入口例外。

2026-10-26 R5本地实现补充：未执行且整项已取消/被替换的排队输入可独立归还准入，不必等待同模型活请求完成；清理不重置活请求的busy/running。原生批次仍有有效项、或实际读取未结束时保持整批输入，cancel不是强杀。此内部优化不新增ABI或自动组批，证据见`docs/verification/sdk-r5-priority-20261026.md`。

模型换代（首版语义）：热切换 = `amv_model_load` 新句柄 + 旧句柄 `amv_model_close` 排干在途；新旧结果各自携带 `am_model_identity_t` 不串版；旧句柄结果保留预算独立清退。原子无缝热切换属 6.5 后续候选，首版不承诺。

### 3.8 诊断统计与资源限额（推理侧；B10）

```c
/* 与相机侧同构：限额读写（冻结包络内收紧，am_limits_t 附录 §4）、队列用量/耗时分段/实际后端
   统计/丢弃过期替换计数；资源域归属与原生库观测分列同 §2.11（B10） */
AM_API am_status_t amv_get_limits(amv_context_t* ctx, am_limits_t* out_limits);
AM_API am_status_t amv_set_limits(amv_context_t* ctx, const am_limits_t* limits);
AM_API am_status_t amv_get_stats(amv_context_t* ctx, const char* scope_utf8, am_stats_t** out_stats);
AM_API am_status_t amv_stats_release(am_stats_t* stats);
AM_API am_status_t amv_get_last_error(const void* any_handle, am_error_info_t* out_info);
AM_API const char* amv_status_name(am_status_t status);
```

## 4. 模型档案格式（兼容单位=任务+导出档案）

兼容单位不是文件名或"YOLO 全家族"，是「task + decoderId + 输入输出规格 + 前处理 + 后处理 + 坐标约定」的冻结档案。识别不可靠时：显式指定档案、或走 RAW 模式（B05 两条路都走得通）、或拒绝，禁止猜测后静默出错。

### 4.1 档案 JSON schema（低频配置诊断通道；冻结时随交付包发布只读档案表）

```json
{
  "profileId": "pose-yolo26-e2e-coco17-v1",
  "task": "pose",
  "decoderId": "yolo26-pose-end2end-coco17-v1",
  "inputs":  [{ "name": "images", "dtype": "float32", "shape": ["batch", 3, 640, 640],
                "ranges": { "batch": [1, 1] } }],
  "outputs": [{ "name": "output0", "dtype": "float32", "shape": [1, 300, 57], "semantics": "pose-end2end-rows" }],
  "preprocess":  { "resize": "letterbox", "target": [640, 640], "fill": 114,
                   "color": "rgb", "normalize": "div255", "layout": "nchw" },
  "postprocess": { "nms": "none", "end2end": true, "maxDet": 300,
                   "kptShape": [17, 3], "kptNamesRef": "coco17" },
  "coords":    { "box": "normalized-xyxy-original", "keypoint": "normalized-xy-original" },
  "modelMeta": { "task": "pose", "end2end": "True", "kpt_shape": "[17, 3]" }
}
```

- `ranges`：动态维度公布范围（min/max 成对）；固定维不出现。
- `modelMeta`：模型元数据必须匹配的键值（加载校验，防档案与导出版本漂移）；`semantics`：输出张量行语义（供原始张量复核），行内布局说明随档案附录。
- Segment/OBB 档案在 `postprocess`/`coords` 中分别补：掩膜系数×原型合成/裁切/阈值化顺序与输出口径（`maskBits` 1bit/px+roi_map）；角度单位/周期/方向/顶点顺序与旋转 NMS 声明。

### 4.2 内置档案表（首版冻结清单——YOLO 全家族五类，2026-10-25 用户拍板）

| profileId | task | 输入→输出（dtype/shape） | 前处理 | 后处理 | 状态 |
|---|---|---|---|---|---|
| pose-yolo26-e2e-coco17-v1 | pose | f32 [1,3,640,640] 固定 → f32 [1,300,57] | letterbox 640² fill=114 RGB /255 NCHW | end2end 无 NMS，maxDet=300，COCO17 关键点（17×3） | **已有**（现行 PoseModel 校验即此档案） |
| detect-yolo11n-nms-v1 | detect | f32 [1,3,640,640] → f32 **[1,84,8400]**（4 box+80 类 ×8400 anchor，实测） | letterbox 640² fill=114 RGB /255 NCHW | 内置 NMS，默认 **conf=0.25 / iou=0.7 / max_det=300**（ultralytics 默认，P8 四轮定死为档案默认；conf/iou/max_det 可经 `am_infer_params_t` 覆盖，覆盖后参考结果容差不适用）；坐标归一化 xyxy 原图系 | **冻结样本**（yolo11n@ultralytics 8.4.163 本机导出，shape 实测；SHA已登记于profiles.json，参考输出待实现验证） |
| classify-yolo11n-prob-v1 | classify | f32 [1,3,224,224] → f32 **[1,1000]** 概率（实测） | **classify 前处理定死（P8 四轮，按 ultralytics 8.4.163 classify pipeline）**：**双线性短边缩放至224（不填充，保持宽高比）** + **center crop 224²**；归一化 **mean=0、std=1**（即仅 /255 到 [0,1]）；**RGB** 通道序 | **概率直出**（has_probability=1）：模型输出即 Softmax 概率，SDK 不再套 softmax、不套检测 NMS | **冻结样本**（SHA-256 已入档，见下段实证注记） |
| segment-yolo11n-nms-v1 | segment | 检测头 **[1,116,8400]**（4+80+32 系数，实测）+ 原型 **[1,32,160,160]**（实测） | letterbox 640² fill=114 RGB /255 NCHW（与 detect 同参数，P8 四轮定死） | **掩膜合成顺序定死（P8 四轮）**：系数×原型得到logits → 双线性**上采样到输入尺寸** → sigmoid后以0.5二值化（等价logits>0，禁止直接对logits套0.5）→ 按detect框裁切 → 去letterbox → 最近邻映射回原图；最后按LSB-first打包（顺序写死，逐档案声明相同）；NMS 默认同 detect（conf=0.25/iou=0.7/max_det=300）；掩膜随结果携带尺寸/步长/**位序 LSB-first（§3.6 定死）**/roi_map 映射 | **冻结样本**（同上） |
| obb-yolo11n-nms-v1 | obb | 检测头 **[1,20,8400]**（4 box+1 角度+15 类，实测） | letterbox 640² fill=114 RGB /255 NCHW（与 detect 同参数，P8 四轮定死） | 模型角度→**全局几何约定（§3.6 定死）**映射；内置**旋转 NMS（P8 四轮声明：旋转相似度=probabilistic IoU（probiou；不是多边形交并比），按8.4.163的分类别Fast NMS（任一更高分框达到阈值即抑制，不等同贪心NMS），IoU 阈值默认 0.7；conf/max_det 默认同 detect）**；四顶点按全局顶点序输出 | **冻结样本**（同上） |

五类各自独立验收（不套用 Pose 解码器；Segment/OBB 几何由独立参考实现对照锁定）。RAW 模式与档案表正交（B05）。**共享帧输入的前处理与普通视图一致（SDK-DEC-07）**：经共享帧池借用的帧作为图像输入时，按与 §5.6 像素视图完全相同的前处理语义执行——PROFILE 模型按 §4 档案、RAW 模型按 `am_infer_params_t.raw_preprocess`，不因传输方式（普通视图/共享帧）改变前处理语义或输出容差，也不额外隐式转换。档案表新增条目=兼容小版本能力位变更，经 `amv_query_capability` 摘要哈希可检测。每份档案冻结时明确：模型家族/版本、导出工具版本、opset、导出参数、模型/参考样本摘要、输入输出名/shape/dtype、参考结果与比较容差（终审 §5 逐档要求）。**四类新档案已从「模板候选」改「冻结样本」（P8）**：样本模型=yolo11n 系、ultralytics 8.4.163 本机导出，shape 实测如上表；e2e 导出变体列后续候选，不随本批冻结。Segment 位序与 OBB 角度/顶点序为**全局约定**（§3.6），不随档案逐份变化。

**冻结样本导出参数全量定死（P8 三轮，按备料实测 profile-samples-20261025 入表）**：四类样本导出参数统一为 **ultralytics 8.4.163 / opset 17 / batch=1 / dynamic=False / simplify=False / dtype f32**；imgsz：detect/segment/obb=**640**、classify=**224**；输入名=**images**；输出名：detect=**output0**、classify=**output0**、segment=**output0+output1**（检测头+原型）、obb=**output0**（pose e2e 既有档案 output0 不变）。**classify 输出实证注记（P8 三轮）**：classify-yolo11n-prob-v1 的样本文件 SHA-256=**30a66094446e32a8bb40fd45022c6061ba4a4f464581cf4a6180dd20b4034432**；其 output0 生产节点经实读 ONNX 计算图为 **Softmax**——**输出为概率（has_probability=1）以模型图 Softmax 节点实证，非 shape 推断**；原 classify-yolo11n-logits-v1 条目（has_probability=0）系误判作废。五类实际文件SHA、输入输出和导出参数已核入 `crates/vision/resources/profiles.json`；这是机器可读样本登记，不把尚未生成的参考结果标成通过。Pose样本为8.4.23/opset20/simplify=True，四类YOLO11为上面的8.4.163/opset17/simplify=False，不能统一覆盖成同一导出设置。

**参考结果与容差表（P8 四轮补齐：设计先定格式与容差结构，参考数据实现期生成入库）**：档案冻结不能只有导出参数+SHA——每档案必须配**一张参考图 + 参考输出**：首版参考图用各任务官方示例图或合成图（登记来源与 SHA-256），参考输出=SDK 对参考图的实际推理结果经**独立参考实现**交叉确认后**实现期生成并冻结入库**（档案文件标注「实现期生成入库」）。调用方/验收以 SDK 输出对照参考输出、按下表容差逐项判定：

| 任务 | 校验项 | 容差（候选，冻结评审定稿） |
|---|---|---|
| detect/segment | 框中心与宽高（原图系） | 误差 ≤2 px |
| detect/segment | 类别与置信度 | 实例一对一匹配（类别一致且 IoU≥0.5 配对）后 top-1 一致；conf 差 ≤1e-3 |
| classify | top-1 类别与概率 | top-1 一致；概率差 ≤1e-3 |
| pose | 关键点位置（原图系） | 误差 ≤2 px |
| segment | 掩膜与参考掩膜 IoU | ≥0.95 |
| obb | 四顶点（原图系）/角度 | 顶点误差 ≤2 px；角度误差 ≤0.5° |

参考输出版式（随档案定稿）：逐实例记录 bounds/顶点/关键点/掩膜哈希、class_id、confidence、angle 与模型档案 ID、参考图 SHA、生成环境（SDK 版本+后端）；实例数=0（空结果）同为合法参考输出。NMS 等可覆盖参数使用档案默认值时容差适用；调用方覆盖阈值/参数后偏离默认参考输出不判不合格（容差不适用，另行自备参考）。

## 5. 公共约定（两库同章）

### 5.1 内存所有权总表（B06/B08 修订）

| 缓冲/对象 | 分配方 | 释放 | 借用/retain 规则 |
|---|---|---|---|
| ctx/子句柄（两库） | 各 DLL | close 三段式至 CLOSED，再经 am?_object_destroy 最终释放（§1.1/§5.4，P1） | 句柄非缓冲；CLOSED 后句柄在册（可查询/幂等 close），destroy 后不可用 |
| am_encoded_packet_t | 相机 | amc_encoded_packet_release | pull/异步移交归调用方；回调内持有须 retain；引用计数 |
| amc_frame_t 与 view.planes | 相机 | amc_frame_release | 同上；plane 指针只读，有效期至最后 release；跨 DLL 走 §5.6 |
| am_captured_image_t / am_probe_result_t / am_stats_t | 相机 | amc_image_release / amc_probe_result_release / amc_stats_release | fetch/pull 移交；**回调/借用期内跨期持有必须当场 amc_image_retain（P3）**；ack 只释放 SDK 缓存引用（B08） |
| amv_result_t（含内部数组/字符串/掩膜 bits/raw_outputs.data） | 推理 | amv_result_release | fetch 移交一个引用；跨异步任务持有须 amv_result_retain（B08：保留拥有者，无孤立 tensor retain）；TTL/ack 只出缓存账，外部持有继续计费至 release |
| am_batch_receipt_t | 推理 | amv_receipt_release | 一次移交 |
| am_request_snapshot_t（类型两库同构） | 分配方 DLL（相机经 amc_request_query / 推理经 amv_request_query 移交） | **分库入口（P3 四轮）**：amc_request_snapshot_release（相机）/ amv_request_snapshot_release（推理）——句柄必须回分配方库释放，跨库传入返回 sdk.handleInvalid | **快照句柄制（P3 三轮）**：query 每次移交一个句柄引用；终态内容不可变，已移交句柄至 release 前字符串保证可读（请求出账不影响已移交快照）；保留期内可重复 query |
| 同步调用入参（张量/编码字节/视图/解码输入） | 调用方 | 调用方 | **借用至函数返回；超时返回后 SDK 不再引用（B06）**；若工作允许继续执行，SDK 已在返回前完成安全复制并可查询身份 |
| 异步调用入参 | 调用方（DLL 接受时复制） | DLL 内部 | 接受即复制，调用方缓冲立即可复用；AM_INPUT_BORROW_RETAINED 例外：显式保留拥有者至工作实际结束 |
| am_error_info_t / am_request_snapshot_t 内字符串 | DLL 拥有 | 随快照/句柄生命周期 | 只读；终态快照不可覆盖（B03）；**P3 边界：SDK 不提供字符串复制/释放函数——字符串只读、随所属快照或句柄生命周期失效，长期持有=调用方复制到自身缓冲** |

铁律：**DLL 分配、分配方 DLL 释放**；跨 DLL 只允许 §5.6 视图协议（retain/release 桥接函数），不得把 A 库指针交 B 库释放；调用方不得写任何借用的只读视图。

### 5.2 图像描述结构

即 `am_frame_view_t`（w/h/format/plane_count/planes[4]/strides[4]，附录 §2.5）。不假设行紧密排列；非紧密布局由 stride 表达；plane 有效长度显式声明；不支持的像素格式或无效尺寸/stride 在执行前拒绝（`camera.pixelFormatUnsupported` / `vision.imageViewInvalid`），不按紧密行误读。

### 5.3 回调约定（B07 异步关闭入口）

- **线程**：回调在 SDK 内部调度线程执行，不阻塞采集/推理；同一订阅/模型的事件**串行**投递；不同句柄间可能并发；回调内长阻塞只拖慢本句柄后续事件（慢回调隔离，§6）。
- **重入白名单**：回调内允许只读查询（status/query/stats/get_info）、**缓冲句柄 retain（P3：amc_encoded_packet_retain / amc_frame_retain / amc_image_retain / amv_result_retain——回调参数需跨回调持有时必须当场调用，与 §5.3 借用期条款闭环）**、`*_request_cancel`、**`am?_object_request_close`（异步关闭实际入口，B07：仅标记请求，立即返回，由其他线程或后续轮询推进关闭）**；**禁止**在回调内同步等待自身句柄关闭完成（close/shutdown/unsubscribe 的带超时等待形——死锁）。
- **注销屏障**：unsubscribe/set_callback(NULL)/close 返回 CLOSED 后不再调用该回调；已进入的回调执行完毕；**屏障前不得释放回调函数指针与 user_data**，CLOSED 后即可安全释放（B07）。
- **借用期**：回调参数中的包/帧/结果指针有效期到回调返回，需持有必须当场 retain。
- **异常边界**：DLL 侧 Rust panic 捕获转错误码+错误信息（unwind 不穿越边界）；调用方回调抛出的语言异常由包装层捕获转可诊断错误。回调返回的错误码仅记录不重试。

### 5.4 生命周期三段式、关闭屏障与请求终态（B03/B07）

```c
/* 通用对象状态查询（三段式第 2 步）：对 ctx 内任意对象句柄（source/订阅/decoder/model/capture_sub）
   只读并发安全；配合对象专属 close 函数使用 */
AM_API am_status_t amc_object_query_state(const void* any_object_handle, am_lifecycle_state_t* out_state);
AM_API am_status_t amv_object_query_state(const void* any_object_handle, am_lifecycle_state_t* out_state);
/* 异步关闭请求（回调内/重入白名单安全）：仅标记关闭意图并立即返回；
   由调用方其他线程或后续 object_query_state 轮询推进至 CLOSED */
AM_API am_status_t amc_object_request_close(const void* any_object_handle);
AM_API am_status_t amv_object_request_close(const void* any_object_handle);
/* 句柄最终销毁（P1 两阶段第 4 步）：仅 CLOSED 对象可调（否则 sdk.handleInvalid 附当前状态）；
   销毁后句柄不可用——登记表移除、地址入隔离区（P1 三轮：每库上限 4096，未满永不复用，
   满后最老地址允许复用；destroy 后继续使用旧指针=UB，不属承诺范围）；
   与同句柄其他调用外部串行；ctx 走 am?_shutdown 至 CLOSED 后同样经本函数销毁 */
AM_API am_status_t amc_object_destroy(void* any_object_handle);
AM_API am_status_t amv_object_destroy(void* any_object_handle);

/* 对象生命周期四态 */
typedef enum am_lifecycle_state_e { AM_LIFECYCLE_RUNNING = 0, AM_LIFECYCLE_CANCEL_REQUESTED,
  AM_LIFECYCLE_CLOSING, AM_LIFECYCLE_CLOSED } am_lifecycle_state_e;
/* RUNNING=运行；CANCEL_REQUESTED=已请求取消（≠工作已停止）；CLOSING=关闭进行中（超时停留，资源保留）；
   CLOSED=关闭完成（屏障达成：无回调无输出，句柄在册可查询/幂等 close，业务调用 handleClosed，
   回调资源可释放）；最终释放经 am?_object_destroy（P1） */
/* 已接受异步请求：恰一个终态（am_request_snapshot_t.state 取值） */
typedef enum am_request_state_e { AM_REQ_PENDING = 0, AM_REQ_RUNNING, AM_REQ_DONE, AM_REQ_FAILED,
  AM_REQ_CANCELED, AM_REQ_EXPIRED, AM_REQ_REPLACED, AM_REQ_UNKNOWN } am_request_state_e;
/* AM_REQ_UNKNOWN=ID 从未存在或不属本 ctx；已出账（曾接受且保留期过）由 query 函数返回
   sdk.resultExpired 表达（P3 三轮句柄制，不再以状态值混同「从未存在」与「已出账」） */
```

- **三段式全文合同（B07/P1）**：每类对象（ctx/source/encoded_sub/frame_sub/capture_sub/decoder/model）均按 §1.1：close(timeout) 或 object_request_close → object_query_state / close_query 轮询 → **CLOSED=关闭屏障达成**。CLOSED 后：句柄**登记在册**（query 持续返回 CLOSED、**重复 close 幂等成功**、其余业务调用返回 sdk.handleClosed）、不再产生回调、在途输入/结果资源已按工作实际完成状态释放或出账；句柄最终释放经 `am?_object_destroy`（仅 CLOSED 可调；销毁后地址入隔离区——每库上限 4096，未满永不复用、满后最老复用，destroy 后调用=UB 不在承诺范围，P1 三轮）。
- 关闭超时（timeout_ms）：返回 `AM_LIFECYCLE_CLOSING` 并保留在途输入/结果资源，查询继续轮询；不得释放仍被工作线程使用的内存；关闭等待不因无限重试而无限阻塞（退避期间可打断）。
- 同步超时 ≠ 取消请求 ≠ 工作停止：三者分别表达（deadline 返回+可查询身份 / 显式 cancel / 终态快照，B06）。
- 背压拒绝（未接受，无 request_id，sdk.queueFull）与已接受请求（必有单一终态）严格区分；通知丢失不影响查询。
- **请求终态快照（B03/B09；P3 三轮改句柄制）**：`am_request_snapshot_t`（附录 §1.3）= state + `terminal`（终态标志）+ `am_error_info_t error`（Failed/Canceled/Expired/Replaced 时错误详情，**终态一经写入不可覆盖**）+ created/finished 时间戳。两库 `am?_request_query` 统一返回**快照句柄**（分配方 DLL 拥有；相机侧 `amc_request_snapshot_release`、推理侧 `amv_request_snapshot_release` 释放——句柄必须回**分配方库**的释放入口，跨库传入返回 sdk.handleInvalid（P3 四轮分库，公共 am_ 版本删除）；保留期内可重复 query 取新引用；**已移交句柄至 release 前内容与字符串冻结可读，请求记录出账不影响已移交快照**；请求出账后 query 返回 sdk.resultExpired，从未存在返回 sdk.requestUnknown——抓图与推理同模式）。

### 5.5 六类队列配置结构

```c
typedef enum am_queue_class_e {
  AM_Q_REALTIME_FRAMES = 0,  /* 实时解码帧：丢旧留新，报帧龄/序号缺口/丢弃计数 */
  AM_Q_ENCODED_STREAM,       /* 编码包（H.264/H.265）：丢后等关键帧+参数集重同步（禁止任意接回 P 帧） */
  AM_Q_REALTIME_INFER,       /* 实时推理：按槽替换旧请求（被替换者 Replaced 终态，B11） */
  AM_Q_OFFLINE_BATCH,        /* 离线批量：有界拒绝或限时等待，不静默丢旧 */
  AM_Q_RESULTS,              /* 结果保留（两库抓图/推理结果）：TTL+取走确认；外部持有另计（B10） */
  AM_Q_CONTROL               /* 控制/错误通知：独立容量；数据队列满仍可取消/关闭/读故障 */
} am_queue_class_e;
```

```c
AM_API am_status_t amc_get_queue_policy(amc_context_t* ctx, uint32_t queue_class, am_queue_policy_t* out_policy);
AM_API am_status_t amc_set_queue_policy(amc_context_t* ctx, uint32_t queue_class, const am_queue_policy_t* policy);
AM_API am_status_t amv_get_queue_policy(amv_context_t* ctx, uint32_t queue_class, am_queue_policy_t* out_policy);
AM_API am_status_t amv_set_queue_policy(amv_context_t* ctx, uint32_t queue_class, const am_queue_policy_t* policy);
```

`am_queue_policy_t`（max_items/max_bytes/max_waiters/wait_deadline_ms/ttl_ms）与读写函数两库对称（附录 §1.4）：`am?_get_queue_policy` / `am?_set_queue_policy`。逐类读取/收紧（包络见 §6）；收紧对新接收生效，不回收已持有对象。慢订阅者只消耗自己的预算；推拉用独立订阅句柄（独立游标），同一订阅句柄内双模式互斥（`camera.modeConflict`）。有界队列 ≠ 有界总内存：总预算另受 §6 持有上限约束。

R3 字节记账明确化：推理实时与离线类的 `max_bytes` 分别约束本 Context 的该类已准入暂存，含正在执行或输入归还尚未完成的工作；离线规划元数据同属离线类。两类合计仍受本 Context 的 `preprocess_budget_bytes` 约束，所有 Context 再受共享执行器 64 MiB 硬限约束。不得拿另一类或另一 Context 的用量套用当前类的较小上限；也不得借分类绕过共享总额。项数仍沿用每模型×槽实时在途、离线单次及执行器域在途的既有约束，不改变队列策略结构或默认值。

### 5.6 跨 DLL 像素视图互操作（相机帧直接喂推理；B08 统一签名）

```c
#define AM_PIXEL_VIEW_MAGIC 0x414D5058u  /* 'AMPX'：视图协议标识，防误传裸指针 */
typedef struct am_pixel_view_s {
  uint32_t struct_size;
  uint32_t magic;                 /* 必须 AM_PIXEL_VIEW_MAGIC */
  void* owner_handle;             /* 拥有者句柄（如 amc_frame_t*），对推理库不透明 */
  void (*retain_owner)(void* owner);   /* void 返回桥接（B08），指向 amc_frame_retain_bridge */
  void (*release_owner)(void* owner);  /* void 返回桥接（B08），指向 amc_frame_release_bridge */
  const am_frame_view_t* view;    /* 布局（w/h/format/planes/strides） */
  const am_frame_assoc_t* assoc;  /* 帧身份（B12）：source_id/frame_id/代次/PTS/时间基。
                                     P2 三轮指针化：am_frame_assoc_t 带 struct_size（可演进），
                                     不得按值嵌入——assoc=NULL 表示视图未携带关联；
                                     amc_frame_as_pixel_view 填充时指向帧内常驻数据
                                     （随帧生命周期，retain 帧即保 assoc 有效）；
                                     调用方自建视图时自备 assoc 缓冲（有效期至视图最后一次使用） */
} am_pixel_view_t;

/* B08 桥接导出：void 签名与视图协议函数指针严格一致（内部转调 amc_frame_retain/release）；
   跨语言绑定不再出现 am_status_t(*)(void*) 与 void(*)(void*) 签名漂移 */
AM_API void amc_frame_retain_bridge(void* owner);
AM_API void amc_frame_release_bridge(void* owner);
```

- 推理库收到 `AM_IMG_PIXEL_VIEW`：默认**安全复制**；`AM_INPUT_BORROW_RETAINED` 时调用 `retain_owner` 显式保留，工作实际结束后 `release_owner`（调用方随即释放自己的引用也安全）。
- 推理库只经 retain/release 函数指针持有相机帧，绝不直接释放；相机帧在途期间**调用方不得卸载 am_camera.dll**（包装层以引用计数阻止提前卸载）。首版 CPU 内存路径；杜绝 JPEG 编码→解码往返。**跨进程 CPU 帧传输经共享帧池（§5.7，SDK-DEC-07：首版可选启用）；GPU 显存/硬件表面共享仍列后续可选**——「GPU 互操作后续」与「CPU 共享帧池首版可选」并存不矛盾，不以占位接口承诺。

### 5.7 共享帧池接口族（SDK-DEC-07；两库对称导出，公共内部模块）

**回审修订（2026-10-25）**：本节以[共享帧首版合同](dual-sdk-shared-frames.md)细化启用、逐次发布身份、借用→推理桥接、满池和安全回收；附录 §1.6 提供相应字段。init 尾追加 `feature_flags`（`AM_INIT_SHARED_FRAMES=0x1`，0 默认不启用）；能力位表示构建/平台支持，四值状态另报是否启用，不再只有查询而无启用入口。

**定位（逐字对齐终审 §11.1/§11.2）**：共享内存=两 SDK **公共可选能力**，首版交付 Windows x64 同机 CPU 图片帧共享；**不因此增加第三个 DLL**——公共实现为两个源码 SDK 共用的内部模块（复用代码与协议），分别编译进 `am_camera.dll`（发布侧）与 `am_vision.dll`（消费侧）导出；分别编译不会自动形成共享运行时，**共享段、资源拥有者和协议身份必须显式关联**。「可选」=调用方可不用此传输，**不是首版可以缺**（交付物须至少一种已验证启用配置；其他裁剪构建明确报不支持，见三态）。不做通用 IPC、断电不丢、跨机器；共享内存≠零拷贝（前处理/颜色转换/GPU 上传可另复制）。

**能力位与三态**：`AMC_CAP_SHARED_FRAME_POOL` / `AMV_CAP_SHARED_FRAME_POOL`（附录 §6.1 候选）+ `am_shared_pool_state_e`（可用/未启用/未编入/平台不支持，附录 §1.6）；`am?_query_capability` 一次查询两者，不可用状态明确返回、不得偷偷退化。

```c
/* —— 统一帧描述：固定宽度（uint32/uint64），不依赖 Rust 内存布局；共享段协议以
   protocol_version 判别，不匹配明确拒绝。完整定义=附录 §1.6（固定叶结构，字段冻结） */
am_shared_frame_desc_t;      /* 像素格式/尺寸/平面步长/偏移/有效字节/帧身份/连接代次/时间语义 */

/* —— 相机侧（发布者=共享段资源拥有者）—— */
AM_API am_status_t amc_shared_pool_create(amc_context_t* ctx, const am_shared_pool_config_t* cfg,
                                          am_shared_pool_attach_desc_t* out_attach,
                                          amc_shared_pool_t** out_pool);   /* cfg 上限见下；attach 凭据固定宽度、
                                                                    调用方自行在进程间传递（不含裸指针） */
AM_API am_status_t amc_shared_pool_publish(amc_shared_pool_t* pool, const amc_frame_t* frame,
                                           uint64_t* out_publication_id); /* 池内发布ID；原 source/frame 身份不变 */
AM_API am_status_t amc_shared_pool_publish_view(amc_shared_pool_t* pool, const am_pixel_view_t* view,
                                                uint64_t* out_publication_id); /* ISAPI CPU图/外部像素入口；
                                                   返回前复制入槽，完整验证布局与关联，失败不半发布 */
AM_API am_status_t amc_shared_pool_get_attach(const amc_shared_pool_t* pool,
                                              am_shared_pool_attach_desc_t* out_attach);
AM_API am_status_t amc_shared_pool_close(amc_shared_pool_t* pool, uint32_t timeout_ms,
                                         am_lifecycle_state_t* out_state); /* 三段式 §1.1；最终释放 amc_object_destroy */
AM_API am_status_t amc_shared_pool_get_stats(const amc_shared_pool_t* pool,
                                             am_shared_pool_stats_t* out_stats); /* 占用/丢帧/等待统计 */

/* —— 推理侧（消费者=只读借用）—— */
AM_API am_status_t amv_shared_pool_attach(amv_context_t* ctx, const am_shared_pool_attach_desc_t* attach,
                                          amv_shared_pool_t** out_pool);   /* 版本不符=abiUnsupported；身份/代次不符=generationMismatch */
AM_API am_status_t amv_shared_pool_borrow(amv_shared_pool_t* pool, uint64_t publication_id,
                                          uint32_t wait_ms, am_shared_frame_view_t* out_view);
                                         /* publication_id=0取最新；wait=0非阻塞，1..5000ms有界等待；
                                            out_view 输出到调用方缓冲，plane_data 只读映射 */
AM_API am_status_t amv_shared_frame_as_pixel_view(amv_shared_pool_t* pool,
                                                   const am_shared_frame_view_t* view,
                                                   am_pixel_view_t* out_view);
                                         /* 映射至既有 AM_IMG_PIXEL_VIEW；元数据/owner由内部租约持有，
                                            不引用调用方栈；helper不新增借用，retain桥接见下 */
AM_API void amv_shared_frame_retain_bridge(void* owner);
AM_API void amv_shared_frame_release_bridge(void* owner);
AM_API am_status_t amv_shared_pool_return(amv_shared_pool_t* pool, const am_shared_frame_view_t* view);
                                         /* 按 borrow_id 校验；同一帧供多模型=多次借用各自归还 */
AM_API am_status_t amv_shared_pool_detach(amv_shared_pool_t* pool, uint32_t timeout_ms,
                                          am_lifecycle_state_t* out_state); /* 三段式；最终释放 amv_object_destroy */
AM_API am_status_t amv_shared_pool_get_stats(const amv_shared_pool_t* pool,
                                             am_shared_pool_stats_t* out_stats);
```

**句柄与释放入口（分库原则，P3 先例同源）**：`amc_shared_pool_t` / `amv_shared_pool_t` 为各自库的本地句柄——**每个对象由创建它的 SDK 入口释放**（发布池=create/close→destroy 的相机侧入口；消费 attach=attach/detach→destroy 的推理侧入口）；**禁止把另一 DLL 的本地句柄或指针当成通用跨进程句柄**，跨库传入释放入口返回 `sdk.handleInvalid`。跨进程身份只经 `am_shared_pool_attach_desc_t` 显式传递（协议版本+池身份+拥有者标识+连接代次；固定宽度、可复制、不含本地地址——不把本地地址当传输协议）。

**生命周期合同（SDK-DEC-07，逐字）**：

- 实时链**可丢尚未消费的旧帧或限时等待**（drop_policy 逐池声明；丢弃计入统计，不静默）。
- **正在借用的槽不得覆盖**：存在未归还借用时该槽不可被新发布复用；满池且无可回收槽时发布侧按 drop_policy 拒绝或等待。
- **超时本身不证明读者已停止**：仅在租约排空屏障或已核验创建身份的 Windows 进程对象进入退出信号状态后回收。代次失效、PID失联、拥有者close都不单独证明停读；活读者失联时保留原槽及预算，新池必须使用新身份且继续受总预算约束。命名映射/互斥量及恢复依据见共享帧合同 §4/§5。
- **旧代次消息不能释放新槽**：携带 `pool_generation` 的控制消息与当前池代次不符时拒绝（`sdk.generationMismatch`），不得据此释放或覆盖新代次池的任何槽。
- 进程退出/重启：发布侧退出→消费者 attach 失效可诊断；消费者退出/崩溃→未归还借用由发布侧按上述安全依据路径回收，新代次池不继承旧代次借用。
- 预算准入：池槽数/总字节计入 §6 资源域账本，超限 `sdk.resourceExhausted`；统计含 published/dropped_stale/dropped_full/wait_timeouts/slots_in_use/slots_borrowed/consumers_attached/bytes_in_use（附录 §1.6）。

**上限与异步所有权**：O11 共享帧基准配置已在专项合同 §3 定义（4槽、16MiB/槽、64MiB像素/ctx、8消费者/池、32在借租约/池，等待0..5000ms）；可收紧，不代表性能已验。`return`只释放调用方初始借用；`AM_INPUT_BORROW_RETAINED` 异步已接受请求持有独立内部引用，实际执行结束前不复用槽。共享帧输入的前处理与普通视图一致（§4.2），不承诺端到端零拷贝。

### 5.8 调用约定与常量收束

- 全部字符串 UTF-8（§1.3）；Windows x64 统一调用约定，`AM_API` 导出名不修饰；布尔语义 uint32_t（0/1）；枚举值稳定；不用位域。详细规则与对齐要求见附录 §5。
- 模式/标志常量（数值冻结候选见附录 §6）：`AM_SUB_PUSH/AM_SUB_PULL`、`AM_INPUT_BORROW_RETAINED`、`AM_IMG_JPEG/AM_IMG_PNG/AM_IMG_PIXEL_VIEW`、`AM_LOAD_PROFILE/AM_LOAD_RAW`（B05）、`AM_BATCH_NATIVE/AM_BATCH_SPLIT/AM_BATCH_AUTO`（P6 三值）、`AM_CAP_ENCODED_JPEG/AM_CAP_CPU_FRAME`、`AM_CLK_PTS/AM_CLK_RECEIVED/AM_CLK_CAPTURED`、`AM_PKT_KEYFRAME/AM_PKT_PARAMSETS/AM_PKT_AU_COMPLETE`、`AM_REUSE_ALLOWED`、`AM_ASSOC_HAS_*`（关联字段可用位）、`AM_PF_ANY` 与 `AM_DHW_*`（P7 解码器）、`AM_CM_*/AM_CR_*`（P5 色彩）、`AM_CQ_*`（P5 时钟质量）。

### 5.9 语言层安全像素操作（R4兼容新增，不增加C ABI）

Python `Frame/BorrowedFrame/RgbBuffer.pixels()` 和 C# `CameraFrame/SharedBorrow/RgbBuffer.Pixels()` 返回只读操作门面 `OwnedPixels`，不提供原生指针。同步推理、默认复制异步提交、共享发布和平面复制期间强持拥有者且先锁拥有者再进入模型/发布器；close/Dispose同锁，提前显式关闭后操作拒绝，异常不越过锁。异步保护至提交返回，不等完成，也不启用BORROW_RETAINED或承诺全链零拷贝。

`copy_planes/CopyPlanes`复制声明平面（含padding）为独立字节；Python新RgbBuffer复制构造，C#旧RgbBuffer仍pin调用方数组，新增Copy工厂才隔离外部写入。只读门面不保证底层ctypes/IntPtr逃逸或外部原数组并发改写安全。旧裸pixel_view/BorrowPixel和模型低层入口兼容保留，由调用者承担完整有效期；结果裸view另按原合同，不由本门面保护。

## 6. 资源预算（B10：资源域归属 + 项数/字节双限全表；数值为候选，§9-O1）

**预算归属（B10/P4 三轮）**：相机侧预算归属单位=**Context 资源域**（amc_context_t 各自独立账本），同进程多 Context 时同源 Rust 代码不隐含共享全局变量账本，一域满载不侵占另一域。**消费方式单选（P4 三轮定约、四轮检测口径降级，§1.2）**：同一核心在进程内只允许一种消费形态（宿主静态链接或 DLL 二选一）——**单选为部署约束，由部署方/宿主保证**；SDK 尽力检测，`am?_init` 命中另一形态已存在即返回 `sdk.domainConflict`，不承诺自动互检（跨模块无共同登记点）；违反约束并存的超限按各自预算拒绝，不视为 SDK 缺陷。在该单选约束下，**推理执行器进程内单例即真实满足共享语义**——同进程只有唯一消费入口，执行器实例与在途账本（domain_infer_slots）天然唯一（与 6.2a executor 下移对齐），满足 native spec「宿主和核心在冻结的共享预算下执行」；不存在「宿主与 DLL 各配一份执行器」的装配缺口。执行器域预算由首个创建执行器的 amv_init 定值；总线程预算（执行器+ORT intra/inter-op+预处理线程）经 `total_thread_budget` 可配置、`stats.threads_active/threads_peak` 观测。SDK 可管理的字节硬限（下表）与 FFmpeg/ORT/驱动等原生库内部内存**分开声明与观测**（stats 分列，§2.11/§3.8）：后者为参考值，不承诺任意原生库总 RSS 严格受控。

| 预算项 | 类别 | 候选默认值 | 覆盖口径（B10 双限） |
|---|---|---|---|
| 编码订阅队列 | 项+字节/订阅 | 36 项 + 8 MiB | 项=现行 fanout 队列（36；旧文档 256 已过时，§9-O1）；字节含编码包缓冲 |
| GOP/参数集缓存 | 字节/源 | 4 MiB | 关键帧前参数集与预滚缓存 |
| 每源订阅者 | 个/源 | 64 | 编码+帧+连续抓图订阅总数 |
| 解码帧队列 | 项+字节/订阅 | 推 2/拉 8 项 + 32 MiB | **解码输出帧字节**（实时丢旧留新） |
| 缩放输出 | 项+字节/域 | 64 + 计入帧字节预算 | 按需缩放生成帧（外部持有另计） |
| 帧与缩放总字节 | 字节/域 | 128 MiB | 解码+缩放+等待中帧的总字节硬限（2026-10-29 由 96MiB 上调：2160p 帧峰值 93MiB 压线；转换峰值预留同步精确化 8→7 B/px） |
| 外部未释放帧（retain 后未 release） | 个+字节/域 | 256 个 + 计入上一行 | **外部持有计费**（B08/B10）：ack/TTL 不豁免 |
| 独立解码器 | 个/域 | 8 | 输入队列另计入解码字节预算 |
| 解码器输入队列 | 项+字节/解码器 | 64 项 + 16 MiB | 提交未消费的压缩数据 |
| 连续抓图订阅 | 个/源 | 4 | 共享采集器总限额内 |
| 连续抓图在途请求 | 项/采集器 | 1（默认不并发堆积） | N02 语义：错过跳过不补发 |
| 视频源数 | 个/域 | 16 | 部署预算 |
| 预处理缓冲 | 字节/域 | 64 MiB | 推理侧解码/letterbox/归一化中间缓冲（**预处理入账**，B10） |
| 模型句柄数 | 个/域 | 8 | RAW/PROFILE 合计 |
| 推理并行执行与总线程预算 | 并行 4 线程/执行器域；总预算 0=机器核数 | 4 / 0=默认 | 执行器工作线程+ORT intra/inter-op+预处理线程合计（P4：total_thread_budget 配置、stats.threads_active/peak 观测；§10.2 尾注） |
| 实时推理在途 | 请求/模型×槽 | 每槽 16 | 槽隔离与公平轮转（B11） |
| 离线批量单次 | 项/提交 | 64 | 等待者另计 |
| 结果保留（抓图+推理） | 项+字节+TTL | 256 项 + 64 MiB + 60s | **结果/掩膜字节入账**（B02/B10）：掩膜按 stride×height 计；ack 释放缓存引用，外部持有续计 |
| 控制/错误通道 | 项 | 32 | 独立于数据队列；满载时取消/关闭/读故障仍可用 |
| 内存模型加载 | 字节 | 256 MiB | P1；超限 vision.modelTooLarge |
| 共享帧池槽数/不同池身份 | 个/池与个/ctx | 4 / 4 | SDK-DEC-07；多池仍受同一ctx总字节预算，失联旧池继续入账 |
| 共享帧池单帧/像素总预算 | 字节/槽与字节/ctx | 16 MiB / 64 MiB | 每池total_bytes=槽数×单帧容量，多个池/重复attach不绕过ctx总预算 |
| 共享帧控制元数据 | 字节/池与字节/ctx | 64 KiB / 256 KiB | 映射、描述与租约控制信息单独有界计费 |
| 共享帧消费者/未归还租约 | 个/池 | 8 / 32 | 内部retain不增租约但继续占槽；超限 sdk.resourceExhausted |
| 句柄销毁隔离区 | 个/库 | 4096 | P1 三轮：已销毁句柄地址保留上限；未满永不复用，满后最老地址允许复用（destroy 后使用=UB）；条目为纯地址记录不占工作资源 |
| 默认超时 | 关闭/同步推理/同步抓图/解码 receive | 5 000 / 30 000 / 10 000 / 1 000 ms | 关闭 0=立即返回 Closing；统一口径见 §9-O8 |
| 重连退避 | initial/max/包络上限 | 1 000 / 30 000 / 60 000 ms | 抖动默认 500ms；§2.4/§9-O8 统一 |

- 所有默认值仅可在冻结包络内**调低**（set_limits / queue_policy），不可调高；用量、拒绝、丢弃计数经 stats 可观测。
- 硬解失败转软解、批量接受、连续抓图共享等新增分配路径均先过本表预算准入，超限返回 `sdk.resourceExhausted`（B13/B10）。
- RTSP的SDK配置/包转换暂存在分配前预留保守工作区，成功后配置与输出按Vec容量持续占用、归还多余预留；失败分支持额度直到Reader缓冲销毁。RTSP探测复用该准入，HTTP探测先占既有4 MiB抓图包络；候选资源不足保留既有Pending状态，不伪称网络不可达。推理通知在注册时先为固定队列槽预留完整条目存储（本机每槽48字节，不固化成跨平台常量），空闲槽继续计费，运行期不扩容；存储占一个控制项，活通知另占控制项/关闭屏障但不重复计同一槽字节，排队加忙回调不超过预留槽数。注销/停止待忙回调返回后释放缓冲及许可；初次注册额度不足返回resourceExhausted且不启动通知线程，预算拒绝新注册时原通知已排空、回调为未注册，可显式重试；通知丢弃不改写请求终态。以上覆盖SDK相关载荷，不代表FFmpeg/网络库/驱动RSS硬限。
- 媒体源事件同样在注册时预付固定存储，一个存储控制项加活事件项、不重复计槽字节；槽数受可用项/字节及64项硬限约束，排队加忙回调不超槽数。满时只替换待处理旧提示；只有忙项时丢新提示，均记dropped且不改变源状态/数据交付。注销/停止排空后释放缓冲，首次注册资源不足不启动事件线程；预算拒绝新注册时旧回调已排空且为未注册，调用方可显式重试。
- 附加负载验收（dual-dll-sdk §9 末段）以此为对照账：慢消费者/断流重连/持有到顶/持续取消重建后，内存线程句柄回到冻结预算与恢复容差内。

## 7. 错误码总表（稳定前缀+重试属性；数值候选见附录 §6）

数值布局：`am_status_t` 0=AM_OK；1–499 `sdk.*`（两库公共）；500–999 `camera.*`；1000–1499 `vision.*`。冻结时数值随头文件定死；字符串名经 `am?_status_name` 查询；详情（码+UTF-8 消息+来源+可重试）经 am_error_info / request_query。重试属性：R=可重试，M=需人工介入/换参数，N=不适用。异步错误一律经 `am?_request_query` 请求级快照查询，不用全局 lastError。

### 7.1 sdk.*（公共）

| 码 | 语义 | 重试 |
|---|---|---|
| sdk.ok | 成功 | — |
| sdk.abiUnsupported / sdk.structSizeInvalid | ABI 版本/结构 size 非法 | M |
| sdk.invalidArgument / sdk.nullArgument / sdk.utf8Invalid | 参数/空指针/非 UTF-8 | M |
| sdk.notSupported | 能力位未开（如内存模型 P1 关） | M |
| sdk.queueFull | 背压拒绝（未接受，无 requestId；输入未被引用可重试） | R |
| sdk.resourceExhausted / sdk.deadlineExceeded | 总预算（字节/持有/源/模型）到顶 / 等待超时（≠取消≠停止；B06 语义见 §5.1） | R |
| sdk.tryAgain | 解码 receive 暂无输出且未排干（N01 EAGAIN 语义，非错误） | R |
| sdk.drained | 解码排空完成（drain 后无更多输出，非错误） | N |
| sdk.resultExpired | 结果过保留期或已 ack（截图与推理统一，B03） | N |
| sdk.handleInvalid / sdk.handleClosed / sdk.requestUnknown | 句柄非法/已最终释放/请求 ID 已出账 | N |
| sdk.limitOutOfRange / sdk.outOfMemory | 限额超包络/分配失败 | M / R |
| sdk.callbackContractViolation | 回调内调用禁用接口 | M |
| sdk.domainConflict | 消费方式单选冲突（P4）：SDK 尽力检测命中进程内另一形态消费者（宿主静态链接 vs DLL）时 init 拒绝，错误详情注明检测到的形态；单选约束由部署方保证、检测不承诺全覆盖（P4 四轮：部署约束口径） | M（换进程或统一形态后） |
| sdk.generationMismatch | 代次不符（SDK-DEC-07）：attach 凭据或控制消息携带的连接代次与当前池不符——旧代次消息不释放新槽、不误配新消费者 | M（按新代次重新 attach） |

### 7.2 camera.*（视频库）

| 码 | 语义 | 重试 |
|---|---|---|
| camera.urlInvalid | 地址/引用非法（协议与类型不符亦然，B04） | M |
| camera.credentialRequired / camera.credentialInvalid | 未注入（WAITING_CREDENTIALS 前调用需认证接口）/凭据格式非法 | M |
| camera.authFailed | 认证失败（与超时区分） | M（换凭据后） |
| camera.authFinalFailed | 最终认证失败：明确凭据拒绝且重试耗尽；进入 AUTH_WAITING 等凭据更新（正常 Digest challenge 不算，B13） | M（新凭据后） |
| camera.timeout / camera.unreachable | 网络超时/不可达（fault_class=NETWORK） | R |
| camera.probePending / camera.probeExpired | 探测进行中/缓存过期 | R |
| camera.codecUnsupported | 编码/封装未支持：含厂商私有流、任意切碎字节流、H.265 误标 H.264 等（N01/B01） | M |
| camera.pixelFormatUnsupported | 像素格式未支持（含高位深直出请求，§9-O4） | M |
| camera.sourceLimitReached / camera.subscriberLimitReached | 源数/订阅者满 | R（释放后） |
| camera.captureCapacity / camera.captureFailed | 抓图槽满/执行失败 | R |
| camera.modeConflict / camera.permissionDenied / camera.streamEnded | 同订阅推拉并用 / 复用键权限不匹配 / 流结束不再重连 | M |
| camera.decoderStateInvalid | 解码器状态违规：drain 后 submit、已接受数据重复提交等 | M |

### 7.3 vision.*（推理库；沿用现行核心错误名，新增接口层）

| 码 | 语义 | 重试 |
|---|---|---|
| vision.modelLoadFailed / vision.formatIncompatible | 会话构建/文件读取失败、元数据与档案不符 | M |
| vision.profileRequired / vision.profileUnsupported | 档案未指定且不可识别（RAW 模式可解，B05）/不在内置表 | M |
| vision.preprocessRequired | RAW 模型图像入口未提供前处理描述（B05） | M |
| vision.externalDataUnsupported | 内存模式 external-data 模型拒绝（路径模式仅同目录解析，B05） | M |
| vision.ioUnsupported / vision.tensorTypeUnsupported | 输入输出结构/dtype 不在冻结集合 | M |
| vision.inputNameUnknown / vision.dtypeMismatch | 输入名/dtype 不匹配 | M |
| vision.inputShapeMismatch / vision.inputBytesMismatch / vision.dynamicRangeExceeded | 形状/字节/动态范围校验失败 | M |
| vision.imageViewInvalid / vision.imageDecodeFailed / vision.imageTooLarge / vision.imageFormatUnknown / vision.orientationInvalid | 像素视图非法（尺寸/stride/magic）与图像入口系列 | M |
| vision.confidenceRange | 阈值越界 | M |
| vision.inferenceFailed / vision.outputShapeMismatch | 执行失败（附底层原因）/输出不符 | R / M |
| vision.keypointMetaMissing / vision.keypointNamesMismatch | 关键点元数据问题 | M |
| vision.modelTooLarge / vision.batchUnsupported | 内存模型超限/native_batch 声明与模型能力不符 | M |
| vision.insufficientBatchRejected | 固定批量轴（native_batch=N≥2）尾项不足完整一批被拒（P6 三轮）：不以 SPLIT 单帧提交（违反模型固定输入形状）；填充组批列后续候选 | M（凑满整批或换单帧模型） |
| vision.backendStrictUnavailable / vision.backendConflict | 严格后端不可用/同进程组合冲突（含 ORT 运行库版本冲突，B14） | M |
| vision.runtimeMissing / vision.runtimeLoadFailed / vision.gpuRuntimeMissing | 运行库缺失/加载失败（可定位：版本/位数/路径） | M |
| vision.resultExpired（兼容别名） / vision.requestStateInvalid | 同 sdk.resultExpired（保留旧名）/ 终态后非法操作（如重复 ack） | N / M |

诊断输出统一脱敏：不含密码、令牌、含凭据完整地址，默认不输出原始图像。未知/旧数据/正常空结果/故障四分：时间与代次元数据 + 空结果正常返回表达，不以"无人"代替故障。同对象并发同步调用失败时，`am?_get_last_error` 返回该句柄最近一次失败（对象级）；并发请求各自失败互不覆盖（请求级快照，B03/B09）。

## 8. 规格对照矩阵（Requirement → 接口映射；按 2026-10-25 整改后三 specs 重建）

缺口判定：**34 个 Requirement**（camera 11 + inference 9 + native-contract 14，含新增 N01/N02 Requirement、SDK-G0 源码交付条目与 SDK-DEC-07 共享帧池三行）设计映射齐全；**三轮修订项（P4 消费方式单选 / P6 尾项拒绝 / P8 概率实证与 OBB 首顶点公式等，对照 §0.3）与四轮收口（P3 快照分库释放 / P4 检测降部署约束（spec 同步修订）/ P8 方向表述与档案定稿补齐，对照 §0.4）落实后待审计方复核确认；SDK-DEC-07 增补（对照 §0.5）同批待回审，映射矩阵未关闭**；本文与 spec 冲突处以 spec 为准（备注列标注收窄/开放点）。

### 8.1 camera-sdk（10）

| Requirement | 映射 | 备注 |
|---|---|---|
| 实时输入与输出模式可查询 | amc_query_capability（H265/独立解码/连续抓图能力位）；amc_source_open；amc_subscribe_encoded/frames；amc_capture_sync/async/continuous | 不支持组合明确拒绝；MJPEG 真实编码不冒充；H.265 与浏览器播放分层（§10.1） |
| 凭据由宿主注入且不持久化 | §2.2；desc 原子凭据（B04）；credentials_set/set_provider/invalidate | 诊断脱敏 §7 尾段；DPAPI/地址生成留宿主 |
| 流探测具有时效和明确失败分类 | amc_probe_streams（候选独立结果/observed_at/valid_until/force_refresh）；am_probe_status_e | 候选=已解析具体地址（B04）；TTL 候选值 §9-O1 |
| 编码输出保持可解码边界与时间元数据 | am_encoded_packet_t（AU/封装/SPS-PPS-**VPS**/keyframe/PTS-DTS/time_base/双代次/source_id）；am_frame_meta_t（clock_mask+时间基） | 首包关键帧+参数集起发；三时钟不混同，未知=INT64_MIN |
| 慢编码消费者按关键帧恢复 | 订阅预算；AM_Q_ENCODED_STREAM；AM_EVENT_ENCODED_RESYNC（scope=受影响订阅）；stats 丢弃 | H.265 随机访问按档案语义判定（N03/B12）；不任意接回 P 帧 |
| 解码节奏与推理采样节拍分离 | sampling_max_fps（保参考帧依赖）；presets+amc_frame_get_scaled；transform | 三速率分离；不宣称节拍=实际检测 FPS |
| 断流恢复与配置变化增加代次 | generation（status/meta/event）；am_reconnect_policy_s；amc_source_close（重试中可关）；fault_class 分型（B13） | AUTH_WAITING 等凭据；旧帧保持原时间/代次 |
| 进程内复用按权限与预算隔离 | am_source_desc_s.reuse（复用键=源配置+凭据权限哈希）；每订阅独立预算/游标；连续抓图共享采集器规则（§2.7）；modeConflict/permissionDenied | 推拉独立句柄；网络分发不属本能力 |
| **独立解码器与网络源解耦（N01）** | §2.8 amc_decoder_create/submit/receive/drain/reset/close + 附录 §2.6 | 提交二分（queueFull 可重试/AM_OK 不可重复）；厂商私有流拒绝；软解/硬矩阵分别验证 |
| **ISAPI 连续取图订阅（N02）** | §2.7 amc_capture_continuous_start/stop/sub_set_callback/sub_pull/sub_get_stats | 错过跳过不补发；实际频率/帧龄可查；过期图不刷新时间；停止后无迟到回调 |
| **共享帧池发布侧（SDK-DEC-07）** | §5.7 amc_shared_pool_create/publish/get_attach/close/get_stats + AMC_CAP_SHARED_FRAME_POOL/三态（§2.1）+ 附录 §1.6 | 统一帧描述（固定宽度+协议版本）；有界池满载丢旧或限时等待；借用中的槽不覆盖；统计可查；池句柄由相机侧入口释放 |

### 8.2 inference-sdk（9）

| Requirement | 映射 | 备注 |
|---|---|---|
| 模型加载与能力声明按导出档案管理 | amv_model_load(_memory)（PROFILE/RAW 双模式 B05）；amv_model_get_info；amv_query_capability（MEMORY_MODEL/RAW_MODEL 位+档案哈希） | 内存加载 P1；external-data 口径 §3.2；失败释放已接管资源 |
| 图像入口内置前处理且张量入口不隐式重复处理 | amv_infer_image_*（PROFILE 按档案/raw_preprocess 显式声明）；amv_infer_tensors_*（无隐式前处理） | 坐标按档案约定映射回原图；非紧密布局按 stride 读 |
| 通用张量覆盖命名多输入输出 | amv_infer_tensors_sync/async（带 am_infer_params_t：槽/关联同图像入口，P6）；am_tensor_in_t（含 assoc B12）；校验码 inputNameUnknown/ShapeMismatch/BytesMismatch/tensorTypeUnsupported | 字符串/Sequence/自定义算子明确拒绝；raw 模型可加载（B05）；张量入口同套槽规则无例外 |
| 动态形状与批次按模型约束验证 | am_io_spec_s（dyn_min/max）；dynamicRangeExceeded；amv_infer_batch（NATIVE/SPLIT/AUTO 三值+逐项回执+逐项槽，P6） | 自动攒批不属首版；固定 batch>1 经 NATIVE 组批，**尾项不足整批明确拒绝（vision.insufficientBatchRejected，P6 三轮；不以 SPLIT 单帧违反模型固定形状，填充组批列后续候选）**（合法批次组成 §3.5） |
| 首版业务适配覆盖 YOLO 全家族五类 | amv_result_s（task 判别+pose/detect/classify/**segment/obb** 完整结构 B02）；§4 档案表 | 每类按「任务+导出档案」冻结验收（四类冻结样本 yolo11n@8.4.163，P8；classify 概率输出经模型图 Softmax 节点实证+SHA 入档，三轮）；Segment 位序/OBB 几何=全局约定定死（§3.6 首顶点公式+数值算例）；RAW 模型正交；表外档案拒绝不虚构结果 |
| 同步异步与同帧多模型具有独立结果关联 | amv_infer_image_sync/async（out_request_id B06）；am_pixel_view_t 同帧 retain 供多模型；result.frame（assoc B12）/model 身份 | 无业务融合/报警联动；共享并发预算 §6 |
| 执行后端显式选择并报告真实状态 | am_backend_policy_s（strict）；am_model_info.execution（actual+**subgraph_cpu_nodes（-1=不可获取）**+fallback_reason，B14/P9）；amv_query_backends 四级+verified；单一 ORT 合同+init 失败 amv_backend_retry（§3.1） | 严格失败/允许回退/组合冲突三分；不只凭硬件存在宣称 GPU；首版无 TRT 枚举（P9 对齐） |
| 结果同时支持结构化与原始张量 | amv_result_s（五类+raw_outputs+元数据）；amv_result_fetch 幂等；amv_result_retain/release（B08）；sdk.resultExpired | JSON 只用于配置诊断；通知仅提示不载数据；ack 后外部持有续计费 |
| **共享帧池消费侧（SDK-DEC-07）** | §5.7 amv_shared_pool_attach/borrow/return/detach/get_stats + AMV_CAP_SHARED_FRAME_POOL/三态（§3.1）+ 附录 §1.6 | 只读借用/归还/多模型消费；消费者与未归还数受上限；超时不证明读者停止；旧代次消息不释放新槽；借用视图只读、不承诺零拷贝；句柄由推理侧入口释放 |

### 8.3 native-sdk-contract（14）

| Requirement | 映射 | 备注 |
|---|---|---|
| 核心独立且宿主与 DLL 行为一致 | 全部函数仅依赖 ctx 与显式 runtime_dirs；无函数要求 AM-RT 服务/业务库；能力归属分层 §10.1（N03） | 行为等价验证归 SDK-G1；接口侧无缺口 |
| ABI 类型与版本可协商 | am?_get_abi_version；init.abi_version；struct_size 全结构；§1.4 演进+**布局策略（B09）**；附录权威蓝本 | 小版本尾部追加+能力位；主版本可区分入口；固定叶结构/指针+count/访问器 |
| 生命周期具有关闭屏障 | **三段式+销毁 §1.1/§5.4（B07/P1）**：对象专属 close + am?_object_query_state + am?_object_request_close + **am?_object_destroy**；四态枚举；父子顺序级联 | CLOSED=屏障达成（句柄在册：可查询/幂等 close/业务调用 handleClosed）；destroy=最终释放（**地址入隔离区：每库上限 4096，未满永不复用、满后最老复用，destroy 后使用=UB，P1 三轮**）；超时保留资源；回调资源屏障后才释放 |
| 缓冲区由分配方释放且跨 DLL 保持所有权 | §5.1 总表（B06 同步超时行）；retain/release 清单+amv_result_retain（B08）；§5.6 视图协议（void 桥接） | 同步借用/异步复制/显式保留三档；超时后不引用输入或已复制+身份；禁止 A 指针交 B 释放 |
| 回调线程与重入约定可执行 | §5.3（线程/串行/白名单/**object_request_close 异步入口 B07**/注销屏障/panic→error）；provider.on_drop | 异常不穿越 C ABI；回调内禁同步等待自身关闭 |
| 已接受请求恰有一个终态 | 五终态枚举；**am_request_snapshot_t 不可覆盖快照（B03/B09；P3 三轮句柄制+四轮分库释放 amc_/amv_request_snapshot_release：DLL 拥有、句柄回分配方库（跨库=handleInvalid），已移交句柄至 release 字符串可读，出账后 query 返回 resultExpired）**；am?_request_query/cancel；背压与已接受区分；AM_REQ_REPLACED | 取消与完成竞态单终态；通知丢失不影响查询 |
| 六类队列分别定义满载与保留语义 | am_queue_class_e（含 ENCODED_STREAM/H.265）+am_queue_policy_s；am?_get/set_queue_policy；§5.5 | 每类策略独立（丢旧/重同步/按槽替换/拒绝/TTL/独立控制） |
| 总资源预算覆盖外部持有与共享调度 | **§6 全表（B10/P4）**：资源域归属+项数/字节双限（解码/缩放/预处理/等待/结果/掩膜/外部持有/控制）；get/set_limits+stats（SDK 硬限与原生库观测分列）；**消费方式单选（P4 四轮：宿主静态/DLL 二选一=部署约束（部署方保证）；SDK 尽力检测（冲突 sdk.domainConflict）不承诺自动互检，并存超限按各自预算拒绝不视为 SDK 缺陷——单选下执行器进程单例即真实共享（唯一入口唯一实例唯一账本）+total_thread_budget/threads 统计；spec「总资源预算」Requirement/Scenario 已同步修订）** | 有界队列≠有界总内存；相机侧多 Context 各自账本；执行器共享=单选约束下的单例合同 |
| 错误和诊断不依赖全局可覆盖消息 | am?_get_last_error（对象级）；**request_query 快照句柄制（请求级，B03/P3 三轮）**；**am?_get_init_error 进程级单槽（P3：原子读、attempt_seq 判覆盖、无 TTL 静默失效；三轮 message 内嵌 char[256] 固定缓冲，读取即自包含无失效窗口）**；§7 三表+脱敏+am_error_info_t 完整定义（B09） | 未知/旧数据/空结果/故障四分；并发请求错误互不覆盖；初始化失败经 init 返回码+init 单槽快照 |
| **源码 SDK 独立交付与构建（SDK-G0）** | §10.2 依赖闭包清单（B15）；runtime_dirs 显式目录；附录 §1 init 结构 | G0 在不含主仓的干净目录验证；sdk/README 现状限制列为待办 |
| 运行库位置和后端环境显式交付 | runtime_dirs（init）；am?_get_version_info（位数/摘要）；amv_query_backends（deps_present）；runtimeMissing 可定位；**单一 ORT 合同（B14 §3.1）** | 实测留证归 SDK-G3；CPU+单加速组合初始化顺序矩阵待实测 |
| 跨语言 ABI 验收独立留证 | §1.3 头文件受控生成；§5.1/5.3/5.6 包装层约定（SafeHandle/上下文管理器/数组视图）；void 桥接消除签名漂移（B08） | 验收场景归 SDK-G2；本文提供合同钩子 |
| 发布包与验收状态真实对应 | am?_get_version_info+capability（未支持位=0 不冒充）；verified 位如实标；五类属首版不在候选目录 | 四门证据与规格校验分开；本文不升级任何验收状态；状态=草案待回审（§9） |
| **共享帧池公共可选能力（SDK-DEC-07）** | §5.7 接口族（两库对称导出+公共内部模块，不加第三个 DLL）；能力位+三态（§2.1/§3.1）；生命周期合同；§6 预算行；sdk.generationMismatch | 「可选」≠首版可缺（至少一种已验证启用配置，裁剪构建明确报不支持）；释放入口按创建方库（跨库=handleInvalid）；不做通用 IPC/断电不丢/跨机器；共享内存≠零拷贝；验收清单见 dual-dll-sdk §9 共享帧池段 |

## 9. 冻结评审开放点——历史回审记录（当前定稿见冻结记录）（2026-10-25 终审 §7.2 复核后修订；原"已全部定稿"作废）

本文档当前状态为**整改稿待审计方回审**，下表为修订后候选口径，回审通过并经冻结评审后才升级为冻结值。

| 编号 | 开放点 | 回审候选口径（修订说明） |
|---|---|---|
| O1 | §6 资源上限默认数值 | **重新核定**：编码订阅队列项数改 **36**（现行 fanout 实际值；旧文档"256"已过时，包数与订阅者数不混同）；补字节上限（8 MiB 候选）、预处理/结果/掩膜等字节项与上下文总账（§6 新表）；压力依据（路数×分辨率×模型组合基准）由团队在 6.2 实施前补证，不沿用"全部冻结" |
| O2 | 首批模型档案范围 | **保留五类范围、补合同**：逐档案固定版本/导出参数/几何/样本与容差（§4.2 表+终审 §5 逐档要求）；分类不套 e2e-NMS 二分；Segment 掩膜口径与 OBB 角度约定进入结构（§3.6） |
| O3 | 内存模型加载 | **保留首版纳入**；补 ONNX external-data 解析基准（同目录/内存模式拒绝，§3.2）、大小上限与失败释放 |
| O4 | 首版像素格式 | **保留 RGBA8/RGB8/BGR8/NV12**；补 plane 有效长度（plane_size_bytes）/stride 语义（§5.2/附录，P5）；色彩矩阵/范围经视图 color_matrix/color_range 字段显式声明（附录枚举，P5）；**H.265 高位深不假装 8 位**：首版 CPU 帧输出为 8 位表示并经能力声明，10bit 专用出口列后续 |
| O5 | 首版 dtype | **保留 F32/F16/I64/I32/U8 子集**（不称全集）；能力枚举与未支持错误明确（tensorTypeUnsupported） |
| O6 | 推/拉并存 | **保留**：同源双订阅独立游标、单订阅内互斥（modeConflict） |
| O7 | 错误码冻结 | **保留方向**：补齐 am_error_info_t 快照结构与请求级不可覆盖语义（B03/B09 已落实）；**数值常量待回审定死**（附录 §6 候选表） |
| O8 | 默认超时/退避 | **修订矛盾统一**：关闭 5s、同步推理 30s、抓图 10s、重连退避 1s→**30s**（原 §2.4 的 5s 上限与 §9 的 30s 冲突，统一为 30s，包络上限 60s，§2.4/§6 已改）；同步超时所有权语义（B06）与子对象关闭查询（B07）已定义 |
| O9 | 后端共存矩阵 | **保留受限方向**：CPU+单加速后端；**单一兼容 ORT 运行库+多 EP 会话**（B14 §3.1）；初始化顺序矩阵与组合逐行实测留证（SDK-G3），未测不标通过 |
| O10 | 入口前缀与文件名 | **保留** amc_/amv_ 前缀与 am_camera.dll/am_vision.dll；其他平台产物规则单列（附录 §5） |
| O11 | 共享帧池数值与启用配置（SDK-DEC-07） | **本轮设计收口**：[专项合同](dual-sdk-shared-frames.md) §1～5 给出feature_flags、版本1、默认预算、有限等待、发布身份、像素视图桥接及进程退出/屏障回收依据；Windows同会话同用户启用配置覆盖O4四种像素。结构语义已明确，尚无运行证据，不代替O1～O10定稿或整个6.1冻结 |

**候选值定稿口径（P 系列同步条，2026-10-25）**：档案表（§4.2 冻结样本及 SHA）、能力位（附录 §6.1）、常量与错误码数值、资源预算默认值四类候选值在**冻结评审时定稿**；定稿属设计冻结——运行时行为与数值合理性另行经 SDK-G1～G3 运行验收验证，不因数值定稿即宣称运行通过（运行验收另行验证）。

## 10. 能力归属分层（N03）与交付依赖闭包（B15）

### 10.1 能力归属分层声明（N03）

| 层 | 能力 | 边界依据 |
|---|---|---|
| **视频核心（本 SDK，am-camera）** | 包序保护、参考链恢复、慢消费者隔离、能力探测、采样节拍、帧身份、资源预算、H.264/H.265 接入与解码、独立解码入口（N01）、单次/连续抓图（N02）——AM-RT 与外部应用复用同一实现 | camera spec「独立解码器解耦」「ISAPI 连续取图」Requirement |
| **浏览器播放器（留前端，不进本 SDK）** | WebCodecs、Canvas、rAF、页面可见性、有界显示缓冲；浏览器播放仍按码流统一 H.264 的既有合同验收——**SDK 支持 H.265 不据此扩大浏览器播放承诺（SDK-DEC-01）**；检测/报警不等待显示缓冲 | camera spec「实时输入与输出模式可查询」H.265 分层 Scenario |
| **宿主（AM-RT 或外部应用）** | 报警图片落盘、证据原字节、ROI/规则/任务语义、业务状态与报警闭环、凭据持久化（DPAPI）、设备三流地址生成；SDK 输出能正确关联的新鲜帧，不自行承担报警闭环 | native spec「核心独立」Requirement；终审 §4 N03 |

补充事实（终审 N03）：当前包序保护与硬解管理部分逻辑仍在 `crates/app-core/src/media/registry.rs`、`pipeline_decode.rs`、`hardware_hwaccel.rs`——**仅导出 am-camera 不能自动获得全部修复**；该下沉属 6.2 实施任务（不在本文范围），本文只声明合同归属。

### 10.2 交付依赖闭包与独立构建清单（B15）

**最小依赖闭包（源码集）**：`crates/am-camera`（视频核心）、`crates/vision`（推理核心）、及其内部**纯函数依赖 crate**（以实际 cargo 依赖图产出为准，含 contracts 类纯类型 crate；不强制重构业务大模块，允许包含必要纯类型依赖或拆出）。**不含** app-core 及业务 crate。

**独立构建材料（SDK-G0 验收对象）**：

| 项 | 内容 |
|---|---|
| 独立 manifest/lockfile | sdk/ 独立 Cargo workspace（自带 Cargo.lock），不继承主仓 workspace、不 `../crates/*` 相对引用 |
| feature 声明 | `credentials-win`（Windows DPAPI 门控；非 Windows 提供明确降级或显式拒绝）、解码/抓图/推理特性开关 |
| 测试资源 | 自带单测与样本（不含 AM-RT 仓内路径依赖的相机测试资源副本） |
| 运行库入口清单 | FFmpeg DLL 列表（avcodec/avutil/avformat/swscale 等，含版本/位数）、onnxruntime.dll（版本/位数与 B14 单一运行库口径一致）；经 init 的 runtime_dirs 显式定位，不依赖 AM-RT vendor 相对路径与环境变量兜底 |
| 构建文档 | 目标三元组、构建命令、feature 矩阵、许可证与摘要生成 |

**与 sdk/README 现状衔接**：README 已记录的限制——仓内 `../crates/*` 引用、workspace 继承、相机测试与独立快照限制——列为 **SDK-G0 前待办**（6.2 实施首项），G0 在不含主仓的干净目录验证，避免主仓路径意外兜底。本文仅登记合同与清单口径，不勾选实施任务。

**线程总预算尾注（§6/终审 §6.3）**：总线程预算同时计入外部工作线程与 FFmpeg/ORT 内部线程（intra/inter-op）；机器能力默认值+调用方可配置上限+预算内调度为首版口径，复杂自动调参列后续。

---

## 附录引用

- [dual-dll-abi-types.md](dual-dll-abi-types.md)（附录 A）：全部公开结构的完整 C 定义（§1 公共/init/错误快照/限额/统计；§2 相机侧；§3 推理侧含五类结果与掩膜/旋转框；§4 am_limits_t/am_stats_t 权威定义；§5 调用约定细则；§6 常量与错误码数值候选表）。**正文与附录冲突时以附录为准**（B09 权威蓝本）。
- 冻结评审通过后本文升级为「已冻结 v1.0」，后续修改走 ABI 版本演进规则（§1.4），不原地改语义。

### R3 独立解码配置/参数集预算补充（兼容行为，本地未发布）

### 本地ABI 1.1媒体工作线程预算扩展

兼容新增`AMC_CAP_WORKER_BUDGET`能力位及下列函数，旧结构/签名/枚举数值不变。调用前检查ABI1.1和能力位；未开位旧库不尝试解析新增导出。

```c
AM_API am_status_t amc_set_worker_budget(amc_context_t* ctx, uint32_t max_workers);
AM_API am_status_t amc_get_worker_budget(amc_context_t* ctx, uint32_t* out_max_workers);
```

`AM_MEDIA_WORKER_LIMIT=64`为默认和硬上限；每Context独立，与CPU核数不同（SDK线程含阻塞I/O/回调）。仅可收紧0..当前值，同值幂等，0禁止新增SDK线程；提高或超过64返回limitOutOfRange，低于当前在用/启动槽数返回resourceExhausted且旧值不变。setter禁止回调重入且只接受Running，getter允许Closing只读、Closed拒绝，失败输出置0。同步宿主线程不占槽。

源取流、像素处理、推送回调、凭据提供器、源事件、异步抓图、连续抓图及探测均在SDK创建线程前共用该额度。spawn失败、实际函数返回或panic展开才归还；取消/超时不释放在读工作。FFmpeg thread_count=1运行在已有调用worker不重复计；驱动及其他原生库自建未知线程不纳入虚假保证。Media stats.threads_active/peak为本Context预留/存活SDK工作槽及历史峰值，不是CPU并行度或进程OS线程数；底层库线程另观测，不填假0。此扩展不改变推理原全局线程域及旧预算语义，不冒充整包耐压/完整硬件验收。

R3媒体抓图索引：请求、合并Flight、最近图频率、消费者及活动订阅表的实际容量计控制字节，增长先保留新旧重叠，空闲容量持续计费至服务销毁。Flight本体另预付；请求仍占原控制项token，但不重复按记录len计本体字节，内部存储不新增外持关闭屏障。受理前记录表/订阅登记超额返回拒绝、无新ID/订阅；受理后Flight/HTTP/缓存失败则以原ID给失败终态，受理不代表图片成功。缓存失败须通知全部本Flight等待者并移除在途登记，不能留下永久Pending。已关闭但仍有句柄的订阅可退出活动索引，句柄本体和图片寿命不因此提前释放；降低允许新增源/订阅限额不撤销已打开源/订阅。Context queue_bytes仍为逻辑数据口径，不含全部控制容量或RSS，C ABI不变。见[验证](../verification/sdk-r3-capture-tables-20261027.md)。

R3推理请求记录表：实际容量及增长新旧重叠先占Context控制字节，空闲容量持续计费到服务销毁；不是外持结果/快照，不增加控制快照项数或Context外持关闭屏障。预算不足返回resourceExhausted，不发布ID、不替换旧请求；既有记录数限制、TTL、取消与C ABI保持。已经保留的容量可以复用，后续降低控制上限不追溯撤销旧存储；新存储不得超额。get_stats(context).queue_bytes仍是逻辑在途/结果口径，不包含全部控制存储或进程RSS。见[验证](../verification/sdk-r3-record-storage-20261027.md)。

§2.8的extradata复制先占Context帧字节，配置许可随不可变拥有者存活；reset(NULL)共享该配置引用。每个新原生上下文的Gate缓存另计，更新先校验并规划，预留/分配成功后原子提交；原生重配置的序列化extra副本独立计费至调用结束。新旧上下文构造期间重叠资源均计费，预算不足明确返回resourceExhausted；create/reset构造失败不改变旧配置/代次，外持旧帧继续有效。该保证不扩展为所有原生执行故障的事务回滚，原有恢复合同保留。Rust同步借用NAL访问入口不分配列表，可能在最终错误前访问部分NAL；不改C签名、结构或枚举。见[定向验证](../verification/sdk-r3-decoder-retained-20261027.md)。

R3独立解码控制存储：输入引用槽按有效queue_max_items、时间关联表按原256项边界固定预留，分配前占Context控制字节，空闲槽不提前出账；新旧reset构造重叠计费，失败保留原状态/帧。默认控制总额64 KiB不提高，max_decoders及其他数量硬限不保证可同时耗满；调用方减少输入槽可降低基础存储，过载有明确resourceExhausted。此项不把原生库/allocator内存并入SDK字节上限，也不改C布局。

R3 RTSP容量补充：像素队列在已预留控制额度内固定实际引用槽；128项接收时间表预留实际容量，先淘汰再插入，防止短暂129项使容量翻倍。既有最大队列、时间关联窗口、采样/像素代次、数据及控制额度均不提高，未改变公开ABI。

R3连续ISAPI抓图补充：引用槽按queue_max_items、FPS时间表按既有120项/5秒窗口固定预留，完整存储先占控制字节，先淘汰再插入；空闲容量计费至订阅销毁。低预算可明确拒绝start，失败不留消费者名额，域登记后失败须关闭并销毁。既有丢旧、停止排空、外持图片寿命和ABI不变。

R3共享执行器存储：现有64 MiB共享总额含执行器本体、固定Job槽、预留线程句柄与Context索引保留容量，在途输入按原分类/Context账独立核算。索引增长先核新旧重叠，失败在请求替换及ID发布前返回ResourceExhausted；空闲索引仍计费至执行器销毁。少量常驻存储使极限载荷可用量小于总额，不增加上限。get_stats(context)仍保留逻辑在途/结果字节口径，不将其说成执行器全部存储或进程RSS；对象scope差异继续按R4核齐。清理用固定栈块、记录锁忙时跳过，API查询仍按TTL核过期；C ABI不变。

### R3模型预检查与文件稳定性（本地未发布）

路径/内存模型的SDK预检查上限仍为256 MiB。路径按已打开文件句柄尺寸先准入并有界读取；读取期间尺寸变化失败，不返回不完整模型。内存输入同步借用至加载返回。ORT路径入口仍会重新打开文件及外部权重，不能保证与预检查文件身份一致或限制其内部读取；调用方应保持输入稳定可信，后续再设计跨平台不可变快照。详见[实测](../verification/sdk-r3-model-read-20261027.md)。C ABI签名与布局不变。

R4 C# `InferenceResult.CopyPose()` returns owned deep copies under the result lock; it rejects closed results, non-Pose results and invalid counts or null pointers. Existing borrowed views and C ABI remain unchanged.

R4 本地语言封装增量：Python `Result.copy_pose()` 在结果锁内复制 Pose 框及关键点到独立 Python 值，关闭后不可再调用；每个结果最多300框、每框最多1024关键点，非 Pose 明确拒绝。原 `Result.view` 仍为底层借用，结果释放后不可读取；复制不意味着 C ABI 或底层结果字段变化。

R4 本地 Python 结构化结果扩展：`copy_detect`、`copy_classify`、`copy_segment`、`copy_obb` 在结果锁内深拷贝原生字段，按任务类型校验，返回 tuple；类别名称可为 None。分类保留 logit、probability 与 has_probability 三项，不能将未知概率当0。分割位掩膜以原样 bytes 复制并保留宽、高、stride 和映射；LSB-first、图像坐标及旋转框语义沿用原ABI。检测/分割/旋转框最多300项、分类最多1000项，分割复制总量最多64 MiB；非法尺寸、步长、空指针拒绝。所有操作不改变 C ABI，旧借用视图仍需调用方保护。

# C# 结构化结果复制（未发布）

C# `CopyDetect`、`CopyClassify`、`CopySegment`、`CopyObb` 在结果存活且持锁期间复制原生结果；返回数据独立，原借用接口语义不变。类型不匹配、数量超限或必需指针为空时拒绝；分割掩膜宽高 1–8192、步长不少于 ceil(width/8)，单次累计复制不超过 64 MiB。此为 C# 包装层新增，不改变 C ABI。类别名称由原生合同提供以 NUL 结尾的 UTF-8 字符串。
