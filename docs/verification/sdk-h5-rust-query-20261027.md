# H5 Rust 旧查询显式探测迁移（2026-10-27）

基线 `688bf76`。Rust `runtime::query` 改为只读运行库文件状态和进程已有观察，不加载/初始化 CUDA 驱动；没有传入 probe 结果时设备标记未知，note 提示先显式探测。调用方可 `devices::probe(budget)`，然后将 `CudaSnapshot::from_probe(&report)` 传入 `query_snapshot`；只有拿到设备数才判有或无，驱动未加载或初始化失败、数量未知为 Failed，调用方自行决定何时刷新。删去旧的内部无预算 CUDA 查询路径，C 旧矩阵与 Rust 共用结果转换；C 结构及 ABI 不变。

这是 Rust API **行为变化**：依赖旧 `query` 自动探测的调用方须按上述顺序迁移；不会自动持续刷新，也不应把 false/unknown 当作已确认不存在。显式 probe 的预算在原生调用之间检查，不能强杀卡住的驱动；只读查询仍检查本地运行库路径，不承诺磁盘操作零阻塞。

验证：Vision 核心和 Vision FFI 定向测试及 all-targets 严格 clippy、Release DLL 构建通过；单测覆盖无探测、失败、0和非零的快照转换和 Rust 查询默认未知。相同候选 DLL 的 Python ctypes 验证显式探测、缓存矩阵、刷新失败撤销、关闭并发和真实CPU模型推理通过；查询三调用+JSON P50 0.012 ms/P95 0.0136 ms，仅供证据定位。运行库与 DLL 为工作区构建，非干净发布包；本机 CUDA 实际模型、驱动永久阻塞及 C# 旧矩阵专项仍未验。R5 H5.1/3/4与后续组批继续开放。记录位于 `.local/evidence/h5-rust-query-final-20261027`。

首次收紧驱动初始化条件后，旧 FFI 单元夹具未同步设置已初始化状态导致1项失败；修正夹具并重跑相关包测试、clippy、Release DLL和实调通过。
