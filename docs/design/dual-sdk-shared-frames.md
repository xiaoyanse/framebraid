# 双 SDK 共享帧首版合同（SDK-DEC-07）

状态：2026-10-25 设计回审后分项实施。普通CPU共享帧的双DLL路径已实现并完成Windows x64定向验证，布局及当前故障退化见[协议1实现说明](dual-sdk-shared-layout-v1.md)；媒体与实际异步推理接线尚未验收。1.0实施冻结另见[冻结记录](dual-sdk-abi-freeze-v1.md)，不能用本分项替代6.2c或SDK-G0～G3整包完成。

依据顺序：三份 SDK delta specs > 本文及 dual-dll-sdk.md > ABI 签名/类型附录。本文细化共享帧的行为，类型字段仍以 ABI 附录为准；冲突须同步修正，不能挑选有利表述实施。首版仅 Windows x64 同机同 Windows 会话、相同登录用户；其他权限/服务跨会话场景明确不支持，后续单列。

## 1. 启用与入口闭环

- `amc_init_params_t` / `amv_init_params_t` 尾追加8字节对齐的 `uint64_t feature_flags`：`AM_INIT_SHARED_FRAMES=0x1` 显式启用，0 默认关闭，未知位拒绝；旧 size 未包含完整字段时按 0，不能把旧结构的尾部填充误读为新增字段。构建或平台不支持而请求启用时 init 返回 `sdk.notSupported`，不可静默忽略。
- 能力位表示“当前构建与平台支持”；`shared_pool_state` 四值区分 ENABLED/DISABLED/NOT_COMPILED/PLATFORM_UNSUPPORTED；后两者优先于开关。支持但未启用仍保留能力位，调用共享接口返回 `sdk.notSupported`，错误详情注明未启用。
- 发布者 `create → get_attach → publish/publish_view → close → destroy`；消费者 `attach → borrow → as_pixel_view → infer → return → detach → destroy`。共享池均为 ctx 子对象，父级关闭须等待本地在途操作及持有的租约安全结束；超时只进入 CLOSING。
- create的cfg为NULL时采用第3节默认配置；非NULL时struct_size合法、容量字段显式非零、reserved清零，0不表示无限。borrow或在途推理存在时不得卸载相应SDK，语言包装持有库与租约拥有者。
- `publish` 接收解码帧；`publish_view` 接收普通像素视图，覆盖 ISAPI CPU 图及外部已有图片，无需伪造相机帧句柄。二者在返回前完成到共享槽的复制，不跨返回借用调用方缓冲，不承诺解码直写共享槽。
- 发布失败时不输出有效发布 ID、不残留半发布槽。成功分配池内唯一非零 `publication_id`；原 source/frame/config 身份保留在描述中，禁止用池发布序号覆盖原 `frame_id`。
- `publish_view`要求assoc声明有效的source/frame/source_generation；缺失明确返回invalidArgument，不猜源身份。调用方已有ISAPI CPU图时可用其像素/元数据构造同步发布视图；编码JPEG须先解码，不把JPEG字节当像素。

## 2. 借用、多模型与推理

- `borrow(publication_id>0)` 只选择该次发布：已淘汰返回 `sdk.resultExpired`，尚未发布可限时等待；0 选择调用时最新的可借帧，池空时可等待。0 不保证两次调用取得同一帧，多源池也不按 source 自动过滤。
- `wait_ms=0` 非阻塞，无可用帧返回 `sdk.tryAgain`；1..5000ms 有界等待，到期 `sdk.deadlineExceeded`；大于上限拒绝，无无限等待。关闭唤醒等待者并拒绝新借用。等待者数量受既有域队列上限约束，不为每个等待者新建线程。
- 同帧多模型使用同一非零 `publication_id` 分别借用，各获得独立 `borrow_id`。帧身份隔离键为池身份/代次 + source_id/source_generation + frame_id，不单靠 frame_id。
- `amv_shared_frame_as_pixel_view(pool, view, out_view)` 验证本地池与有效 borrow，生成现有 `am_pixel_view_t`；SDK 内部租约对象提供 owner/retain/release 桥接及稳定的 frame_view/assoc，不指向调用方栈上的 out_view。桥接时完整传递色彩、PTS 时间基、config_generation 和有效位。
- 同步调用或默认复制式异步调用沿用普通像素输入语义。`AM_INPUT_BORROW_RETAINED` 异步接受前持有内部租约引用，调用方随后可 return；只有所有模型实际结束并释放内部引用后，发布侧才能复用槽位。提交失败必须撤销新增引用。
- `return` 消费调用方最初的一份借用引用，不取消已接受的推理；该调用返回后调用方原共享视图及其派生借用视图失效，不得继续读。重复归还或不匹配 borrow 返回 `sdk.handleInvalid`；旧池代次返回 `sdk.generationMismatch`，均不改变其他有效借用。
- 消费者 detach 拒绝新 borrow，但允许 return、内部 release 与状态查询；本地初始借用或内部在途引用未排空时停在 CLOSING。不得把“已调用 return”当成异步推理实际结束。
- 同一borrow的视图构造/提交与首次return由调用方串行；SDK内部retain/release线程安全且不阻塞等待。不同借用可并发，不能把调用方归还与推理接受竞态当作合法缩短缓冲生命周期的方式。

## 3. 容量、丢旧与准入

| 项           | 首版确定的基准配置             | 合同                                                               |
| ------------ | ------------------------------ | ------------------------------------------------------------------ |
| 槽数         | 4                              | 可收紧为 1..4；容量 0 不表示无限                                   |
| 单帧容量     | 16 MiB                         | 超限返回 `sdk.resourceExhausted`，不缩图或截断                     |
| 每池像素容量 | 64 MiB                         | 必须等于 slot_count × max_frame_bytes；溢出先拒绝                  |
| 消费者       | 每池 8                         | 每次 attach 是独立消费者；无动态发现/无限广播                      |
| 未归还租约   | 全池 32                        | 一次 borrow 计一份；内部 retain 延长同一租约，不增加份数但继续占槽 |
| 发布模式     | DROP_OLDEST，publish_wait_ms=0 | WAIT 显式选择，0..5000ms；不存在无限等待                           |
| 共享传输版本 | 1                              | 不同版本拒绝，不猜兼容                                             |

这些是首版可收紧的配置上限，不是性能承诺。每个 ctx 的共享像素映射总预算为 64 MiB；不因多建池、重复 attach 或跨池持帧而放大。单个物理池的本地映射按身份只入账一次，消费者/租约分别入账；发布者与消费者处于不同进程时各自报告本地映射占用，不宣称全系统 RSS 唯一计费。每池控制区、描述及锁元数据上限 64 KiB，另计同一 ctx 的共享控制元数据预算 256 KiB，最多 4 个不同池身份；失联但未安全回收的资源继续计费。

DROP_OLDEST 只替换无任何借用引用的最旧已发布槽。全池被借用时拒绝新帧 `sdk.queueFull`，不偷覆；WAIT 在截止内等空闲槽，超时 `sdk.deadlineExceeded`。发布等待不能放在取流/解码回调中长阻塞，实时适配默认使用非阻塞档。丢旧及满载拒绝分别统计。

publish返回超时/队列满即未接受，不允许返回后继续暗中发布；只有AM_OK输出有效publication_id，调用方据此决定能否重试。borrow超时同样不产生迟到租约。

统计区分：`slots_borrowed` 是被借用的不同槽数，`outstanding_borrows` 是尚有任一引用的租约数，`bytes_in_use` 是已分配像素映射容量，`metadata_bytes` 是控制映射容量；不能用当前帧有效大小掩盖预分配内存。

## 4. 描述、协议及访问边界

- 公共 C 结构是 API 参数，不以 `memcpy(sizeof(struct))` 当跨语言传输格式。attach 描述由宿主逐字段通过已有控制通道传递；字节数组按原始字节保存，uint64 不经 JavaScript 浮点数舍入，若使用 JSON 则十进制字符串编码。
- 创建时生成随机 `identity_tag[32]`，数据/控制/同步对象按 `Local\amrt-sfp-v1-<identity_tag 的64个小写十六进制字符>-data/ctrl/lock` 命名（最终后缀三选一，不包含斜杠）。同会话同用户 DACL，禁止 world 可写；命名已存在则重新生成，不能附着或覆盖旧对象。
- 共享头保存 magic、版本、段长度、池身份/代次、拥有者 PID 与进程创建时间、固定描述长度、槽数/容量及身份标签。固定宽度字段显式按小端序编解码；内部控制布局在版本1实现的 layout 表中锁定并测试，不共享 Rust 对象、指针或原生锁内存布局。双方来自同一协议版本实现；锁/唤醒使用按名打开的 Windows 内核对象。
- 消费端只读映射像素区，读写控制区单独映射。发布/借用/归还的状态转换经共享命名互斥量保护；收到 abandoned 状态时使池故障并拒绝继续复用，不猜测半写状态。阻塞等待有界重试，不持锁等待其他进程完成，也不为每帧创建线程。
- 创建/打开的内核句柄不自动继承给子进程；每个使用共享视图的进程须单独attach登记自身真实进程身份，不允许把本地视图/句柄转交另一进程绕过登记。
- attach 先验证版本、身份、实际段长度及创建进程身份，再暴露像素。每帧验证 plane_count、尺寸、格式、stride、offset、有效字节及所有加乘/范围；NV12 验证两平面与色彩参数，失败不构造越界指针。版本不符 `sdk.abiUnsupported`；身份/池代次不符 `sdk.generationMismatch`；非法布局 `sdk.invalidArgument`。
- 共享帧保留原 PTS 和 time_base_num/den；有效时分子分母非零，未知=PTS INT64_MIN 与 0/0。接收时刻使用同机原始 QPC/frequency 换算毫秒，不扣进程启动点；消费者不得重新标为“刚采集”。QPC 不是 UTC，设备采集时间仅在已有映射可信时填写。
- 创建新池产生新随机身份；不得依赖重启后计数器递增、PID 不复用或32位代次永久不回绕来防串帧。相同身份内序号/代次耗尽时停止该池，要求创建新身份，不原地回绕。

## 5. 异常与关闭：不把逻辑失效当作物理停读

安全回收仅有两类依据：该消费者所有初始及内部引用已通过 detach/return 屏障排空；或已核验消费者的进程创建身份且持有的 Windows 进程对象确实进入退出信号状态。PID 查询不到、访问被拒、心跳超时、代次改变或调用 close 均不能单独证明退出。

活读者失联时不覆盖其槽、不解除其映射、不出账；原池停止准入或保持有界退化。若另建池隔离，必须使用全新共享对象，旧对象仍保留并占用预算；预算不足明确拒绝，不以不断换代逃避上限。消费者重启重新 attach，不继承旧消费者身份；迟到旧归还消息不影响新租约。

发布侧 close 等待消费者与租约排空；超时维持 CLOSING，return/release/查询仍可完成。发布进程已退出时拒绝新借用，存活消费者已有只读映射及在途引用保持至归还，不假装进程退出会立即销毁其他进程仍持有的 Windows 映射。确认某消费者退出后才回收它的记录，不影响其他消费者。

## 6. 实现与回审出口

至少验证普通内存/共享帧同样本等价，H.264/H.265 解码帧与 ISAPI CPU 图的 publish_view 路径、NV12 色彩/PTS 时间基、两个模型同一发布 ID、异步接受后立即 return、满池全借用、重复归还、迟到代次、进程退出与 PID 身份校验、活读者失联不复用、abandoned mutex、非法平面范围以及多池总预算。记录默认配置的实际复制、线程/句柄/映射峰值与回落，不把64 MiB配置当 RSS 承诺。

AIeng 按既定 A（先推理适配）→B（视频与帧池）顺序，实际改造另行授权。首版示例 C/C++、C#、Python 各自可运行，SDK-G0～G3 仍待实施验证。其他讨论备忘不进入本合同。

依据：[Windows 只读映射](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-mapviewoffile)、[进程退出与内核对象生命周期](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process)、[QPC 时基](https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps)。以上是机制依据，不是本 SDK 运行证据。

初始化补充（2026-10-25用户确认）：推理侧显式启用共享帧且未提供runtime_dirs时，可不安装ORT而仅执行共享帧借用/归还；不能加载/执行模型。提供运行库目录则严格初始化及显式失败重试，见ABI冻结r4。
