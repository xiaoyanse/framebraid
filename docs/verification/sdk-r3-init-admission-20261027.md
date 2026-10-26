# R3 初始化目录与并发准入验证（2026-10-27）

状态：隔离工作包经主代理审查、统一编译及定向测试通过；真实合并、main 集成验证与最终 DLL 仍待。测试基于 `5f329641e3a307db540c2d8a20ed517684acdad9` 加本包源码差异；以下不冒充完整 R3 验收。

## 范围与机制

- 修改公共支撑层 Context 初始化，不改 C ABI、运行库选择、线程预算默认值或 AM-RT。
- 原路径先复制运行库目录、构造 Context，最后才受 `SDK_MAX_CONTEXTS=8` 限制。大量同时初始化可在拒绝前各自持有目录副本；域索引项数上限不能证明这部分总字节受限。
- 新路径先对不超过 16 项、每项最多 4095 UTF-8 字节的输入做零分配借用校验，然后以域绑定 RAII 申请 pending 名额。`registered_contexts + pending_contexts <= 8`；普通句柄插入同样为 pending 预留句柄容量，不允许抢占已准入的登记位置。
- 准入后才执行 `try_reserve_exact` 目录 Vec 和 String 复制，并检查实际容量。分配失败、容量偏差、线程预算错误及登记失败都释放目录和 pending 名额；登记成功在同一域临界区把 pending 原子转成在册 Context，不重复计数。
- pending 期间不持域锁，不跨 FFmpeg/ORT 调用持锁；卸载空闲检查包含 pending。Vision 的既有线程配置 singleton 与默认继承不变。

## 量化边界（MET-2）

主目标是初始化峰值容量可证明，不以加速解码或推理作为指标。保护指标是错误返回、配置一致性、失败可重试和关闭/卸载安全。

- x64 每个 Context 的目录 backing 上限：`16 * sizeof(String) + 16 * 4095 = 65,904` 请求字节；域内在册与 pending 合计不超过 `527,232` 请求字节。Vec 本体、Context/Arc 固定结构、分配器元数据、运行库自身内存另外计，不将此值冒充 RSS。
- 借用校验的固定栈对象为 16 个 `&str` 和 count（x64 实测 264 字节）；每个准入 guard 为域引用及 bool（x64 实测 16 字节）；Registry 新增一个 usize pending 计数（x64 8 字节）。宿主自建的任意数量调用线程及其栈不归 SDK 池上限约束。
- 目录配置属于有固定项数的基础配置，不挤占 64 KiB 控制消息队列预算，不提高预算默认值。
- 定向测试比较相同 16×4095 字节目录的旧复制操作与新饱和拒绝路径：旧目录复制机制复现实测 17 次分配、65,904 请求字节；新饱和 init 实测 0 次分配、0 请求字节，三次分配故障恢复轮均一致。前者是原目录复制机制的局部对照，不包含原路径 Context/Arc 分配，不是整条 init 或端到端吞吐比较。

## 已执行的隔离工作包验证

- 主代理执行 `cargo test -p am-sdk-support --all-features -- --nocapture`：29 个单测、services 5 个、version 2 个，共 **36 通过、0 失败、0 忽略**。覆盖：8 个并发 pending、域锁可继续操作、4095 普通句柄与最后一个保留 Context 槽、7 在册与 1 pending 成功移交、失败登记归还名额、既有 Vision 默认/冲突配置回归。
- 同包 `tests/services.rs`：目录 Vec、第一项和第二项 String 分配失败后均能重试至 8 Context；最大长度与实际容量核对；相对目录错误不分配不占位；Vision 目录复制失败不污染线程配置。
- 主代理执行受影响包 `cargo clippy -p am-sdk-support --all-targets --all-features -- -D warnings` 通过；本包指定文件 rustfmt 和 `git diff --check` 通过。ABI/边界与 main 集成回归、最终 DLL 验证仍待主代理统一运行。
- 精简原始日志保存在主仓 `.local/evidence/r3-alias-storage-20261027/init-worker-tests.log`（SHA-256 `acd0d68b1290b5477e298c18565a7cc58a295e71fb7e0e9639c01b24559fc8fa`）及 `init-worker-clippy.log`（`f1a5e770ec2d78b85501cdf58e0bb3fdd366c4139250630a876dd11378d0c8b0`）。本记录只引用，不搬入公开材料。
- 未运行真实运行库、实机硬件、4h/24h 持续运行；本包不关闭这些验收项。
