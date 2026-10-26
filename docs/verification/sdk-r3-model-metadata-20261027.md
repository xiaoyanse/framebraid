# R3模型元数据与输入校验验证（2026-10-27）

评审基线 `eec8251`；仅SDK内部，C ABI/公开Rust签名/默认预算不变，不修改AM-RT、不推送或发布。主目标是有界元数据复制、失败原子性、消除每次Run的重复元数据克隆；保护IO名字/类型/动态范围/实际字节长度和所有权。

## 实现与量化

- 模型IO在复制前核非空、每向≤32、name≤4096字节、不含NUL、rank≤8；Ffi拥有者、C名字、三份shape/min/max、视图Vec和input_specs副本都fallible精确预留并核实际capacity，不再用不可恢复clone/collect构造这些副本。
- 32项最大元数据的同路径旧构造机制165次alloc/realloc→新130次。另input_specs构造65个分配点保留必要拥有权；逐一注入全部195点失败，均返回resourceExhausted、净保留字节0，之后可重试。不是将外部C语言malloc失败自动转换，也不宣称全部ORT/allocator内部OOM均可恢复。
- 每向IO拥有者+视图实际backing141856B，独立input_specs134912B；双向+输入副本合计418624B/模型。核心TensorInfo、Context/ModelHandle/Arc固定体、profile/device与原生ORT会话另外列，不冒充总模型内存或RSS。基础元数据受每Context8个模型（包括构造中、Closed但未destroy）与每库8Context包络约束，不强塞64KiB控制消息。
- 元数据构造失败先drop原生Model，再减模型计数/归还线程permit，避免额度先可复用而旧session尚未销毁。
- 核心validate_input_data改借用迭代元数据，public validate_input_infos签名不变；32输入单次成功校验旧65次元数据分配→0，像素/张量载荷复制未变。不满个数先拒绝，不先克隆调用方任意长度列表；重复名字/类型/形状/动态范围/实际数据长度错误保持原键。

## 实测

原始证据 `.local/evidence/r3-model-metadata-20261027/`；初次测试中的视图测试拥有者检查已在审查中改成同时保留owner并核指针，不采用丢owner后读指针的断言；语言脚本第一次输出到evidence被其tests目录约束拒绝，改用合法tests路径后通过，未改工具约束。

- metadata4项、核心validation2项通过；195点失败注入及边界零分配拒绝在其中。
- source快照包真实merge后 `cargo test --workspace --all-features`：339通过、0失败、21显式忽略。相关vision/vision-ffi/camera-ffi全特性全target严格clippy、最终两库Release构建通过。
- Python10拥有权测试、C#37结构/259字段、两语言真实CPU Identity同步/异步、fetch/ack、共享租约与C#有序选择/strict失败报告通过。
- 实际CPU未取结果/慢通知/健康模型与共享活租约压力回归通过（inference-pressure/result.json）。
- 实际H.264本机RTSP压缩/像素/事件慢消费者回归通过，三段各约2秒，健康画面baseline9.10/pressure8.99/recovery8.61FPS，max_age≤51ms，最大间隔≤156ms；用于定向功能保护，不据有限单轮短窗宣称吞吐改善，原工具预算不变。关闭后队列/外持出账。
- 最终DLL：Media `39295c3ef4b7d7777985867bf81f1a57980ff7004eeb18dcf8d553bb6f1a0a30`，Vision `fdc3beeea70cfca59ec9ccc564da070cfb382d8507dced897e13ee46d2b0ca80`；详见私有identity.json，构建时dirty源码指纹保留，不冒充已发布tag。

## 审查与未完成范围

正确性/安全/维护/成本审查无致命或阻断：拥有者完成后才创建视图；构造失败不发布悬空指针；IO数据借用仅到同步校验返回；读取实际len继续验证，未绕过字节/shape错误。测试分配故障局限于线程本地，native运行时不被人为注入故障。没有新增依赖或C ABI/冻结变更，调用方式/语言示例/模型档案不受影响，故不重写。

来源事件/stats的固定快照及成本见[相邻记录](sdk-r3-source-snapshots-20261027.md)；该包整合已纳入上述339项和真实RTSP。R3父预算仍需最后汇总固定服务体与原生/宿主边界，不能用此子项代替。完整硬件矩阵和固定长跑已取消强制验收，不再列为欠账。
