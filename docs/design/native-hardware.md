# 原生硬件发现与解码接线

本地R2/H2实施合同；不覆盖[1.0冻结](dual-sdk-abi-freeze-v1.md)，不代表公开v0.1.0已具备硬解。本页涉及Rust兼容新增，C ABI及自动选择接线仍按H2后续任务推进。

## 分层与入口

- `am_camera::media::hardware::adapters()`：枚举Windows x64 DXGI适配器，最多8个，返回快照索引、名称、LUID、设备标识与内存。索引不可跨拓扑变化持久化；同名不去重。DXVA2使用D3D9序号，必须按LUID映射，不把DXGI序号直接代入。
- `hardware::probe(Arc<FfmpegApi>, Duration)`：显式操作，软预算大于0且不超过5秒，同进程最多一轮，忙时立即返回Busy。最多16次D3D11VA/DXVA2设备创建；编译支持、设备初始化与失败原因分别记录。调用间检查预算，不强杀驱动调用。报告的`stream_verified`恒为false。普通状态查询不自动执行本操作。
- `CpuDecoder::create`保持CPU默认；名称保留以维持Rust源码兼容。
- `CpuDecoder::create_on_device(api, codec, extradata, max_pixels, backend, adapter)`：显式H.264/H.265设备解码构造，未提供自动回退；拒绝无目标HW_DEVICE_CTX配置的解码器。初始化成功不等于目标码流可用。当前错误沿用DecodeError，完整诊断的跨语言入口尚未交付。
- `hardware_backend()`仅在接收真实硬件像素格式后返回后端；无帧返回None。输出仍为CPU拥有的RGBA/RGB/BGR，硬件表面通过`av_hwframe_transfer_data`回读，不能把GPU地址直接当CPU平面。该路径不承诺GPU零拷贝或性能必优于CPU。

D3D11创建后从真实设备查询IDXGIAdapter LUID，与请求快照核对；发现FFmpeg默认设备回退或拓扑变化导致身份不符时拒绝。DXVA2的映射缺失明确失败，不静默初始化另一块显卡。系统DXGI/D3D9库从System32按需加载，不增加CPU路径隐式加载依赖。非Windows x64枚举返回UnsupportedPlatform，不冒充跨平台验收。

## 生命周期与资源

### 共用选择层（H2.0b 实施合同）

`CpuDecoder::create_with_policy(api, codec, extra, max_pixels, policy, budget)`返回解码器和拥有的选择报告。`SoftwareOnly`保留CPU语义且不枚举硬件；`PreferHardware`在共享的单轮探测准入下，以D3D11VA优先、适配器快照顺序、随后DXVA2选择。最多16项尝试，预算大于0且不超过5秒，只在原生调用间检查。先核目标解码器HW_DEVICE_CTX配置，不因设备类型可枚举而选择不支持的解码器。无硬件、缺系统依赖、初始化失败、探测忙或耗尽预算时返回CPU与明确原因，不永久缓存失败。

报告区分目标策略、初始化选中的设备、逐项失败与实际硬件帧；`selected`只代表初始化选择，不能作为实际执行证明。真实后端仍须读取解码器`hardware_backend()`。这个工厂不解析完整视频未来的profile/位深，也不承诺初始化能证明全部码流；目标码流错误仍由send/receive返回。上层须据此停止本代次的硬件尝试、按CPU预算准入后等待随机访问点恢复；选择层不私自重放已接受的视频包。显式reset/新代次可以重新选择，普通查询不探测。

该层为直接解码和RTSP像素订阅提供同一策略，不改变现有C ABI枚举、默认值或布局；调用方仍须先取得解码器许可，并在输出/回读前取得字节预算。本阶段未完成DLL/RTSP接线，H2.1～H2.4不因此关闭。

设备探测的本地引用由同一FFmpeg运行库归还，包括原生失败但留下非空引用的情况。解码器持有独立`av_buffer_ref`；get_format回调opaque指向稳定Box，其寿命覆盖CodecContext释放。回调仅选择目标格式，不隐式切换其他后端。

解码器最多持有一张待输出硬件帧及一个CPU回读帧；输出字节预算检查先于回读。错误保留待输出帧供上层明确重试/关闭，不覆盖调用方已经拿走的Vec。所有帧、包、设备上下文在Drop中归还。后续C ABI接线必须将CPU回读暂存纳入全局预算，原生驱动/DPB显存另作可观测限制；不能因Rust原语测试通过而宣称已满足全局耐压合同。

## 运行库与验收

### 参数重建的设备复用（H2.0d 实施合同）

直接解码器和RTSP像素订阅在参数集变化时需要重建codec上下文。`CpuDecoder::reconfigure(codec, extra, max_pixels)`构造独立新上下文，硬件路径引用已有设备，不重新发现或初始化设备；原上下文与待输出帧不受构造失败影响。新上下文尚无真实输出时不得继承旧上下文的硬解验证状态，调用方负责随机访问点与代次管理，不搬运旧队列或重放旧输入。`software_reconfigure`明确构造CPU上下文供上层恢复，不私自决定重试和回退策略。设备引用和运行库均覆盖上下文全寿命，允许新旧上下文分别销毁。CPU路径同样保留该语义。本原语不代表DLL自动回退已经完成。

### 回读预算接线（H2.0c 实施合同）

`hardware_readback_bytes()`在取得待输出帧后返回CPU回读所需保守字节数；CPU帧为0。`copy_pixels_with_readback_budget(max_output_bytes, format, max_readback_bytes)`先核两项预算，再分配和回读，所有退出路径释放CPU暂存；`copy_pixels_reusing_readback`最多保留一张CPU目标帧供复用，调用方许可必须覆盖缓存全寿命，直至`release_readback_cache`/丢弃/关闭，不能在copy返回时提前归还。预算不足或回读失败保持原硬件帧/PTS供重试。

默认Rust copy保持原有“会话内最多一张回读缓存”的有界使用方式（旧实现对应原生transfer pool），资源域管理则由内部`BudgetedDecoder`透明持有Charge。尺寸/格式变化先释放旧缓冲和旧许可，再取得新许可并创建目标帧；失败归还许可且不覆写已经输出的Vec。字段析构保证原生缓存先销毁，Charge随后归还。直接解码与RTSP像素复制共用该预算适配器，不让调用方自己管理C ABI暂存。

回读不再让`av_hwframe_transfer_data`自动分配到原生transfer pool，而是以`av_frame_get_buffer(...,64)`显式创建单张CPU目标帧，再传输到已有缓冲；可按上述作用域释放或在预算覆盖下复用。首步只接受运行库报告可传输的NV12/P010LE，其他格式明确拒绝，供上层回退。所需字节按32行对齐、64字节步长的`av_image_get_buffer_size`加512字节平面/地址填充计算，并通过`av_frame_get_plane_buffer`核实际大小，依据锁定libavutil61 `ff_frame_get_video_buffer`；不读取GPU地址或猜显存布局。驱动/解码DPB内存仍单列原生观测，不冒充SDK托管硬限。

这一步不改变C ABI或默认CPU路径；两个入口已共用实际Charge适配器，真实硬件组件测试覆盖预算拒绝/恢复/缓存持有/释放，但自动选择尚未接入，不冒充两入口整链硬解完成。Rust `HardwarePolicy`不能直接强转C ABI数值：既有C合同PREFER_HW=0、SOFTWARE_ONLY=1，装配层须显式映射。

转换上下文按输入格式、输出格式及尺寸键控，每个解码器最多一个，参数变化重建；转换失败则失效重建。保持原有缩放标志与色彩算法，不为提速改变像素。测试覆盖同尺寸不同图、RGB/BGR、尺寸变化、8/10-bit格式变化、构造/转换失败及关闭，并对拍新旧校验值。

FFmpeg的`h264_d3d11va`/`hevc_d3d11va`为旧AD_HOC接口，不等于现代AVHWDeviceContext解码。新构建必须启用`h264_d3d11va2`/`hevc_d3d11va2`，并对每个编码调用`avcodec_get_hw_config`确认D3D11VA和DXVA2均具有HW_DEVICE_CTX方法。设备类型可枚举不替代此检查。旧公开运行库及冻结哈希保留原字节，更新另建候选。

`hardware_probe`示例区分原生与注入失败；创建/归还计数配对。`hardware_decode`只读取本地合成8-bit 4:2:0单AU，重复12次验证时间戳与CPU/HW像素。YUV首帧对拍用于区分解码差异与不同swscale输入格式的插值差异；RGBA逐帧记录平均和最大差异，不以宽松整图平均掩盖解码内容错误。该示例含观测拷贝，不作为性能基准；单AU不证明P/B帧、格式切换、断流、长跑或生产画面通过。

当前C ABI直接解码、RTSP像素订阅尚未自动接入本原语。接下来须按H2.1～H2.4完成策略选择、实际设备/后端及失败原因查询、回读预算、关键帧重同步、有界重试与关闭竞争，再做三轮同输入端到端性能比较。推理设备选择按R5/H5单独推进；推理CUDA与视频D3D11不可混为一项。
