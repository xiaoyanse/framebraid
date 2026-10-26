# R3硬件报告暂存与固定服务体（2026-10-27）

评审基线 `9079b0d264b83cfd0e04c4af5c3ed978a09a14c4`。仅SDK main，本地阶段保存，不升级AM-RT、不推送或发布。指标M-RECOVER：报告临时存储分配前有额度、无增长/JSON树副本，失败明确；保护JSON字段、生命周期、真实后端与回退诊断。不是视频帧率或CPU卸载收益测试。

## 修复与量化

此前Media报告先构造serde Value树，再to_vec增长，最后仅检查len，查询虽预留32KiB却没有约束两份存储的实际容量。现以类型化借用报告直接写入fallible精确16KiB输出，写入上限16383B，为C终止NUL保留空间；超额返回resourceExhausted、不扩容、不返回截断JSON。真实字段/枚举/隐藏关闭拥有者状态保持，测试按解析内容比较，JSON对象键序不作合同。

- 最大原生字段构造（8适配器×2后端共16尝试、128个控制字符设备名、最大数值）：JSON3386B；旧Value+to_vec分配208次，新1次。序列化本身不克隆原生选择记录。不是所有报告恒定208次，也不据此宣称CPU同比下降。
- 输出固定16384B，Cached固定56B+Arc头16B，共16456B持续计control。探测先保留32KiB，释放临时选择记录后再缩到16456B；原生解码器已先销毁。查询保守预留32KiB，含同一16KiB输出；默认64KiB下缓存+单查询可同时容纳。相比旧小报告紧缩Vec，缓存常驻量增加，这是有界固定容量的明确取舍。
- 查询借用source/decoder已拥有的选择报告，不额外复制JSON树；并发查询/旧缓存引用未释放时继续持有原许可，不通过提前退账提高并发。输出16383B边界通过，16384B明确拒绝；满writer再次写入不增长且不产生堆错误字符串，之后重试成功。
- 使用已有serde依赖增加当前crate直接声明，只改变本包锁内依赖边，不升级第三方版本或C ABI。

## 固定基础体包络（Windows x64、all-features实测）

尺寸来自编译期类型的运行测量，不是heap/RSS测量；Arc头16B为当前x64实现开销，allocator额外头/对齐另列。不把嵌入成员和整体重复相加。

| 对象 | size_of字节 / 数量边界 |
| --- | --- |
| Context | 1224，含内联Services512及启用特性时共享Ledger；每库在册+pending≤8，目录backing另列每Context≤65904 |
| Media CameraState / Usage / Metrics | 32 / 40 / 96，每Context各一，Arc分配头另各16 |
| Media抓图State / CloseHooks / 探测Cache / 线程Budget / 硬件Slot | 368 / 136 / 112 / 16 / 24，每Context各一，Arc头另各16；连同Usage/Metrics实际7种服务，低于16槽 |
| Vision State / Usage / 硬件Slot / 选择Slot | 208 / 80 / 24 / 16，每Context各一；State含固定3后端行，模型通知另按模型数和既有额度 |
| Vision硬件Cached / 选择Cached | 144 / 200；各自已有16KiB / 8KiB缓存许可，查询预留32KiB / 16KiB，不另无限累积JSON树 |
| ModelHandle / Info / 核心Model / TensorInfo | 552 / 200 / 248 / 56；Info与Model内嵌在Handle，不能重复计基础体。模型每Context≤8，含构造中和Closed未destroy |

Media固定服务体+CameraState及上述Arc头共952B/Context；Vision对应392B/Context。与Context体、域Entry/token/两4096索引、动态载荷和原生库分别核算，不将它们全部强塞64KiB控制载荷。记录/Flight/缓存引用槽、Hub和请求/结果等动态拥有者的实际容量仍沿既有预算矩阵计费；共享强引用不重复收费同一像素实体，引用槽另收费。

核心两向TensorInfo已有32/name4096/rank8边界，按当前56B元素及名字/shape计算≤269824B；FFI两向+输入副本≤418624B，见前记录。原生ORT会话自己的IO/initializer/优化图/执行缓存不属于这些副本，不能用上述数值宣传总模型内存硬限。模型档案/设备/错误原因使用有界参数和稳定键；诊断不积累任意错误日志。

## 同候选验证

原始记录 `.local/evidence/r3-report-storage-20261027`；identity.json保存最终两DLL SHA-256与dirty构建身份。

- 新增容量/等值测试2项，固定布局观测10项；工作区all-features **351通过、0失败、21显式忽略**。
- 受影响support/camera-ffi/vision-ffi全target严格clippy、两库Release、fmt通过。
- 实际DLL：旧ABI初始化、未来ABI拒绝、长度查询/短缓冲/NUL/canary及关闭报告通过。本机Intel Arc H.264实际D3D11VA输出；H.265样本硬件执行失败后CPU恢复，保留Runtime错误和代次，未把初始化成功当实际执行。
- 相同最终两DLL下真实CPU推理未取结果/慢通知/共享活租约压力通过，关闭worker检查通过。没有触碰现场设备或业务库。
- C ABI132函数及11冻结文件未变，冻结全局runtimeAcceptance仍为pending，不借本项改成通过；源码边界更新后238核心文件通过。

首轮修改遗漏内部测试从Value转bytes的解析适配，编译拒绝后补齐；首次新测试错误使用不可被旧Value表示的u128极值及低估标准库错误对象分配，修正为实际可达数值范围，并消除writer错误字符串分配。保留失败日志，不把它们归因于硬件或忽略。

## 审查与剩余

按正确性/安全/维护/成本审查：只读借用不逃逸锁和拥有者，输出不在输入报告存储上写入；缓存和在途引用保留许可到真实释放，序列化失败不发布部分JSON。JSON schema/头文件/绑定/快速接入方式不变，故不重写这些接口材料。固定大小替代小Vec的常驻代价已公开。

R3父项暂保持开放：最终核查还发现核心模型文件metadata检查后使用fs::read，文件并发增长可能使读取临时量越界；下一阶段补读取限制和失败回归后再作父项总结。原生FFmpeg/ORT/驱动、系统网络缓冲、外部调用线程数/栈和直接Rust辅助Hub实例数由各自边界管理，SDK局部额度不是整个进程RSS或线程硬限。完整硬件矩阵/固定4h/24h已取消强制验收，不恢复该门槛。
