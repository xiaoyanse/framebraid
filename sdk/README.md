开发者接入从 [INTEGRATION.md](INTEGRATION.md) 开始，英文入口见 [Getting started](../docs/GETTING_STARTED.md)。当前源码仓库保留 sdk/ 层级，Windows 开发者预览包见 [v0.1.0](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0)。

# FrameBraid 双源码 SDK 技术说明

交付主体是独立 Rust 源码 SDK，Windows x64 目标为 am_camera.dll、am_vision.dll，C ABI 1.0，121 个入口（相机 74、推理 47）。本文保留原宿主阶段的技术合同；原宿主的五类模型、加速后端等验证不能直接当作 FrameBraid 提取验收。当前具名候选验证为 Windows x64 Release、CPU，见[最新状态](../docs/RELEASE_STATUS.md)；提取过程记录内部归档。不以入口数代替功能、制品或现场验收。

## 独立源码构建

当前仓库已经是独立源码，不需要原宿主导出脚本，在仓库根目录执行：

```powershell
$env:CARGO_TARGET_DIR='D:/FrameBraid-build'
cargo build -p am-camera-ffi -p am-vision-ffi --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames --locked
node scripts/sdk/check-boundary.mjs
```

源码闭包包括相机/推理及纯内部依赖、固定lockfile、资源、公开头/绑定/样例和测试脚本；不依赖AM-RT服务、数据库或桌面壳，不携带vendor运行库/模型权重。sdk-source-manifest.json逐文件记录来源提交、未提交来源变化、SHA/size。运行库和模型由调用方按验证矩阵显式提供，不能回到主仓兜底。

## 能力与调用顺序

相机：init → source_open（原子凭据或稍后注入）→ 选择ISAPI抓图/连续抓图，或RTSP编码/帧订阅 → 推/拉输出 → release → 逐个订阅close/destroy → 源close/destroy → ctx shutdown/destroy。source_open异步连接，元数据未就绪的编码订阅返回probePending，调用方有界重试。无凭据不发网络请求；认证最终拒绝停AUTH_WAITING，显式更新后恢复。

- RTSP接入H264/H265/MJPEG，编码不转码，H264/H265以完整Annex B AU输出。独立解码器首版接完整H264/H265 AU；长度前缀/任意分片需先在调用方装配，未支持能力位为0。MJPEG编码包是原JPEG，bitstream_format字段对此编码不适用。
- ISAPI单次/异步/连续输出原JPEG或RGBA帧；同共享源连续节拍去重，不补发错过节拍，不把重复原图重打时间。原图JPEG不重新编码；CPU解码可处理PNG，但JPEG出口拒绝PNG伪装。
- 四像素格式RGBA8/RGB8/BGR8/NV12。NV12经计预算的RGBA中间帧规范化为BT.601 limited；ANY取RGBA，实际软件后端明确回填，不声称已硬解。get_scaled的method=0最近邻/1双线性，像素中心对齐，保留帧身份/时钟和可证明变换；NV12目标宽高须偶数。preset仅声明档位上限，按需get_scaled，不预分配每档。
- 编码慢订阅丢链后等新关键帧；其他订阅继续。读流与参考帧解码连续，sampling_max_fps仅限制输出。源重连/权限轮换递增代次，旧包/旧图不可贴新身份。
- 双预算按ctx累计：字节、项数、外持、控制拥有者。入队帧保守预占外持额度；多个外发图片/包别名保守重复计额度，不能把预算数当独立物理RSS。原生FFmpeg/ORT内存另列；native_rss_estimate_bytes=0表示未提供独立RSS观测。

推理：init（显式运行库目录）→ load路径/内存模型 → 图像或命名张量同步/异步/批量 → fetch → ack缓存 → release外持 → model close/destroy → ctx shutdown/destroy。

- 五个固定导出档案Pose/Detect/Classify/Segment/OBB；档案ID、SHA/前后处理以crates/vision/resources/profiles.json及设计为准，不承诺任意YOLO版本通用。RAW支持五dtype、多命名IO和显式图像前处理。固定batch不能成完整批次明确拒绝；NATIVE/SPLIT/AUTO回执逐项表达接受/失败。未发布本地扩展：动态批量轴模型可经 amv_model_configure_batching 显式启用有界自适应攒批（AUTO语义内凑满/截止/排空三触发，逐项身份不变），默认关闭、固定轴拒绝、尾填充仍不支持，合同见 dual-dll-abi §3.5 末；合成数据单测已验，真实模型组批实测待验。
- 图像JPEG/PNG或RGBA/RGB/BGR/NV12；异步默认复制，BORROW_RETAINED保留拥有者至真实执行结束。同步超时返回后原输入可释放，终态仍可按request_id查询。
- CPU可与单一种OpenVINO或CUDA共存，同进程单一ORT路径；另一加速类型明确冲突。OpenVINO固定CPU设备且provider/ORT线程均计配额；strict失败不偷回退，允许回退返回实际后端。query_backends不偷偷建探测会话，硬件/依赖/实际加载/实际执行分开，未观察不假称已验。
- 提供目录的init严格加载，失败不给ctx且保持故障；所有ctx销毁后显式backend_retry恢复。显式AM_INIT_SHARED_FRAMES且无runtime_dirs可仅共享帧，不能据此执行模型。错误初始化不污染已正常运行库。
- 完成通知为独立有界线程，不阻塞推理；丢通知后终态仍可查。ack/TTL只清缓存，外部结果继续有效并计费。线程配额不是OS忙碌线程实测。

## 共享帧

shared-frames编入与init.feature_flags=AM_INIT_SHARED_FRAMES运行启用分开；未启用/未编入/不支持平台明确拒绝。发布普通帧或外部pixel_view，消费者只读borrow转普通推理入口。publication_id与原帧关联分开；return不撤回在途推理引用；活读者不被超时强回收。元数据必须给真实或显式合成关联，未知时间INT64_MIN，不伪造设备同步。

## 生命周期与凭据

close发起排空，CLOSING可查询，CLOSED后destroy。所有外持、回调、工作线程退出后才卸载SDK；取消不能强杀原生调用。buffer必须由分配库配对release，跨库释放/双释放拒绝。危险回调重入明确callbackContractViolation，普通retain/release/fetch/ack按合同白名单执行。

reuse=1为共享权限域，关闭一个源不误杀其他消费者；轮换影响该域并发事件。共享provider的user_data须等on_drop后释放，不能以单个源CLOSED代替共享provider注销。凭据仅注入，不持久化，不出日志/导出；传入URL不得嵌入用户名密码。

FFmpeg原生日志可能带敏感URL；SDK Reader存续期间同一FFmpeg实例日志引用计数静默，最后Reader退出恢复原level。SDK结构化故障仍可查询。不会安装卸载后失效的原生日志回调。

2026-10-26用户确认：ORT/FFmpeg底层运行库允许常驻宿主进程，退出时统一回收；SDK自有线程/句柄/回调/外持排空后，双SDK模块仍支持卸载和重载。依赖不得保留已卸载SDK回调，不在DllMain阻塞回收或强卸载不拥有的依赖。主机已有ORT路径不匹配明确拒绝；要切换不兼容运行库须重启进程。有限轮资源走势与7×24、加速后端验收分别记录，不承诺未测平台。

## 多语言示例

[examples/README.md](examples/README.md)提供C/C++17/C#/Python真实推理、共享帧和相机入口。Python生成的amrt_sdk.py为完整原始ABI，safe_sdk.py为主要流程拥有者封装；C#用SafeHandle保持回调/内存寿命。Java后续。示例用合成身份/数据；不能把示例本机通过当现场硬件通过。

## 验证与打包

```powershell
node scripts/sdk/generate-abi.mjs --check
node scripts/sdk/generate-rust-abi.mjs --check
node scripts/sdk/generate-python-abi.mjs --check
node scripts/sdk/freeze-abi.mjs --check
python scripts/sdk/test-ffi-lifecycle.py --dll-dir D:/FrameBraid-build/release
python scripts/sdk/test-camera-capture.py --dll-dir D:/FrameBraid-build/release
python scripts/sdk/test-camera-probe.py --dll-dir D:/FrameBraid-build/release --runtime-dir D:/FrameBraid-runtime/ffmpeg
python scripts/sdk/test-camera-transport.py --dll-dir D:/FrameBraid-build/release --runtime-dir D:/FrameBraid-runtime/ffmpeg
```

传输脚本只用随机端口MediaMTX/FFmpeg自产流，路径通过参数给出，不触现场相机。没有FFmpeg命令行工具的测试机可用AM_SDK_TEST_JPEG指定预生成64×48红色JPEG，解码脚本的--reference-dir指定对应样本的ffprobe JSON和FFmpeg RGBA参考，模块寿命脚本用--codec-reference-dir传入；这些仅替代测试参考生成，SDK仍实际解码。其余test-inference/test-tensors/test-batch/test-notify-stats/test-backends/test-accelerated-backends/test-module-lifecycle分别验证图像、张量、批量、通知、后端恢复和模块寿命，需要对应固定样本/运行库，详见脚本--help与主仓验证记录。

运行库登记及历史打包工具见 [runtime/README.md](runtime/README.md)，未来 FrameBraid 包须按[发布清单](../docs/RELEASE_CHECKLIST.md)重建并验证。工具核完整性不等于批准分发；当前仓库本身就是独立源码，不依赖原宿主 export-source。正式制品逐文件 SHA 与来源提交另行记录。

公开头文件共 121 函数、74 结构、547 字段，自然对齐/UTF-8；布局、数值与预算单源生成，freeze-v1.json 为发布前 r5，r1～r4 历史保留。G0 独立构建、G1 核心、G2 ABI、G3 环境分别留证，FrameBraid 当前边界见[最新验证状态](../docs/RELEASE_STATUS.md)。原宿主的完整内部验收报告未随 SDK 导出，不将其缺席冒充新增验证。

依赖提示分 SDK 未加载与已加载两层，分别用语言装载异常或既有 SDK 错误详情；不把系统 126 直接断言为 VC++ 缺失。新受控精简运行库档案的严格材料检查通过，原 BtbN 档案仍有静态依赖材料缺口，两者不混用；均不自动批准发布。
