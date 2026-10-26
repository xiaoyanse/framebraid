# R3：连续抓图固定引用槽与FPS历史存储

## 目标与改前证据

评审基线`3459d8548eb00cc6c93c2487d672d4bc8bc7bf4e`，R3 1.1k，MET-2 M-RECOVER/S5。只补SDK连续ISAPI订阅存储与失败登记回收，不改AM-RT、业务数据或公开制品。主目标为完整控制准入、固定实际容量及空闲持续计费；保护丢旧、120项/5秒FPS窗口、慢回调隔离、外持图片和关闭。

改前先抽出原Queue构造/窗口操作，定向测试真实失败：3槽配置下原预留312字节，滚动10,000次后时间表capacity=128，本体加该时间表已1288字节（引用队列仍为空）。真实旧Release在控制总额512字节下start返回0并创建订阅。保留原失败，不将一次成功启动升级成持续供图验收。

## 实现与结果

完整许可包括Subscription本体、queue_max_items引用槽和120项i64时间表，分配前准入，两块VecDeque固定容量并核容量；先淘汰再记录，最终窗口不变。先构造存储再占消费者名额，注册锁失败归还原子计数；域对象插入后登记失败关闭/destroy，避免无输出句柄对象。清空不归还仍存在的容量，最终析构才退账，外持图继续按原release语义。

| 场景 | 结果 |
| --- | --- |
| 3个引用槽、120项历史 | 本机完整存储1272字节，capacity=3/120，已分配容量与预留一致 |
| 10,000次窗口滚动 | 长度120、首9880末9999；跳到20000后旧5秒窗口清空，仅留20000 |
| 所需控制字节少1/恰好足够 | 前者拒绝且占用0；后者占1项/1272字节，clear后继续占额，销毁后归零 |
| 消费者登记锁被注入poison | 返回internalError，continuous计数回0且无订阅；捕获的测试panic为预设故障，不是未处理SDK崩溃 |
| 真实C控制预算512字节 | 新start返回8、输出NULL、HTTP请求仍0；旧同条件为0/创建成功 |

时间表槽容量从1024降至960字节，仅为本机单表64字节差，不是进程RSS或吞吐收益；完整预留比旧的漏计312字节更多，属于预算完整性修复。未提高默认控制总额，低预算可能更早拒绝。

## 保护与身份

3项新定向测试及Media默认并行全组48通过，11项显式原生材料ignored。clippy首次指出map_err应为inspect_err，修正后严格clippy通过，原失败保留。

真实Release Digest合成服务回归通过：JPEG精确、64×48 RGBA、请求fetch/ack和取消终态、连续抓图丢旧、push/pull互斥、回调屏障、慢回调不阻塞另一路、错开消费者共享请求、帧身份/接收时间以及外持图保持Context Closing。HTTP峰值1。20次不同频率订阅全部成功，停止后的缓存峰值1、健康单次抓图成功；这是原缓存保护，不声称新性能提升。

旧Media SHA=`de01e3581194ab7f9d44e58e5c0f27eb855bd65c4b7585d28553373dee43383d`；候选=`7a19c07993fa4f6c3cd88c7a299923ef9c5226b47deea1e9d3c43e4981c5779b`。Windows x64 Release/共享帧、基线加本包dirty源码，Vision/运行库/原公开包不变。输入JPEG仅242字节合成图，SHA=`e212b0366d978259be0c66486c3c694d3911526b9051d10c4b90c1819c826ad6`。

私有证据`.local/evidence/r3-capture-storage-20261027/`，C改前/后在`r3-capture-storage-baseline-20261027/`和`r3-capture-storage-final-20261027/`，变频raw在`r3-capture-storage-cache-20261027/`。复现：`cargo test -p am-camera-ffi --all-features --lib storage_tests -- --nocapture`；`test-capture-storage.py`传DLL/新证据目录，改前另加`--observe-baseline`；既有`test-camera-capture.py`及`test-camera-capture-cache.py --require-complete`为保护检查。仅随机回环端口，HTTP线程和服务由脚本finally关闭，无现场设备或大型临时副本。

## 审查与剩余

自审预算先于构造、局部/字段析构顺序、失败消费者计数、域登记回滚、时间窗口等价与回调引用，未发现本包剩余致命项。导出文件已超过500行的软检查线，保留现有C导出组织，未达800行；本次仅抽取短私有构造/登记帮助，不为压行数重写既有导出。

同步ABI行为、接入、来源、CHANGELOG、路线/状态、OpenSpec及[完整预算矩阵](sdk-r3-budget-matrix-20261027.md)。没有新公开函数、依赖或平台，README/SUPPORT/示例不变。BP10请求表/执行器常驻元数据、BP12线程统计仍待处理，R3父任务不勾选；4h/24h和完整硬件矩阵未由本包代验。

最终fmt/clippy、C/Rust/Python/C#生成检查、ABI冻结11份、来源边界213文件及OpenSpec严格校验通过；本轮测试进程已结束。来源修订前后快照保留在本阶段证据，原提取指纹与外部.gitignore保持不变。
