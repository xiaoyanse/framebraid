# R3 Hub与CameraState基础容量收口

评审基线：`5f32964`。本包为内部容量/分配优化，不修改C ABI、默认额度、公开订阅上限或宿主版本；R3父任务与整包持续运行验收保持开放。

## 测前目标与边界（MET-2）

- 主目标：每源Hub订阅登记使用固定64个Weak槽；高频`list()`返回固定栈上Arc快照，不再按帧构造Vec。只量化引用容器的分配机制，不把它宣称为CPU、RTSP吞吐或端到端延迟同比改善。
- 次目标：每库CameraState登记使用固定8槽，移除HashMap动态backing；保留每Context唯一Usage/Metrics，保留关闭后外持State阻止最终destroy的生命周期。
- 保护：订阅顺序、已有配置可调上限、停止但回调未退仍占槽、锁外投递/reset、快照持有者安全、控制额度先准入再构造与最终释放。订阅队列`try_reserve_exact`后增加实际capacity核查，拒绝超过已计费项数的backing。

## 设计及计费口径

CameraState登记8槽与Hub登记64槽属于固定基础结构，不强塞64KiB控制消息额度。Hub随SharedSource销毁；快照随本次调用栈释放，多个宿主并发调用有各自快照，不代表SDK限制了调用方线程总数或整个进程RSS。

快照使用64个Option<Arc>槽，只遍历已填充前缀，标准数组Drop逐项释放；登记表只留Weak，不延长订阅对象寿命。`list()`在释放登记锁后才投递、reset或锁队列。关闭但回调线程仍存活的订阅留在登记表，快照排除已停止项，避免提前腾出实际仍被线程占用的订阅名额。

现有订阅控制计费公式保持`size_of<Subscription> + queue slots + presets.capacity()`，本包不冒充它已经覆盖Arc分配头或原生运行库堆。CameraState本体与Usage/Metrics的Arc仍有必要分配；此次消除的是索引容器和临时列表分配，不是所有分配。固定Hub登记相对空Vec会增加基础结构常驻空间，须同时报告实际size；pixels::fanout内部due集合的Vec仍保留，本包不宣称像素分发全链路零分配。

## 定向证据入口

主代理已在本隔离树统一执行Media全features单测：**69通过、11显式ignored、0失败**；严格全targets全features clippy通过。我已核对最终日志与5项新测试的实际输出。首轮编译发现安全快照改造后的旧imports与native失败分支into_iter残留，修正后重跑通过；首轮失败日志保存在主树`.local/evidence/r3-alias-storage-20261027/hub-worker-{tests,clippy}.log`，最终日志为同目录`hub-worker-{tests,clippy}-final.log`，保留前后证据。

| 用例 | 核对内容 | 结果 |
| --- | --- | --- |
| `frames::registry_tests::state_registry_is_fixed_and_reuses_context_services` | 初始化索引零分配、命中零分配、重新创建轻量State仍复用原Usage/Metrics；打印实际结构size | 通过（隔离包） |
| `frames::registry_tests::retained_state_prevents_destroy_and_does_not_free_a_ninth_context_slot` | 8Context满额，旧State外持导致destroy返回tryAgain，第9个拒绝；释放后destroy重试与新Context成功 | 通过（隔离包） |
| `subscriptions::hub::registry::tests::subscriber_registry_and_snapshots_have_fixed_storage_and_no_heap_allocations` | 64槽/第65项拒绝；旧collect同路径分配与新list对照；1000次快照零分配；Arc持有与最终预算释放 | 通过（隔离包） |
| `subscriptions::hub::registry::tests::snapshot_prunes_dead_and_drained_slots_and_resets_outside_registry_lock` | dead/关闭已排空项清理、顺序与reset采样状态；锁已退出 | 通过（隔离包） |
| `subscriptions::queue::tests::hub_slot_survives_stopped_subscription_until_callback_thread_drains` | 状态单测设置alive，确认stopped且alive仍占槽；alive清零后释放；不是实机回调测试 | 通过（隔离包） |

分配计数器仅编译进camera-ffi测试二进制，按当前线程TLS开关统计alloc/alloc_zeroed/realloc次数；初始化对象与Arc拥有者放在计时/计数区外，其他测试线程不混入。旧路径对照保留原先`filter_map(upgrade).filter(!stopped).collect<Vec<_>>()`，报告实际次数，不预设一次或声称内存请求次数等于进程峰值。

## 实测量化（Windows x64，单测机制对照）

| 项目 | 结果与口径 |
| --- | --- |
| State登记 | 固定8槽128B，包含Mutex共136B；登记结构初始化0次分配、已有State命中0次分配 |
| Hub登记 | 固定64槽520B；初始化0次分配，登记64个Weak为0次分配，第65项明确拒绝 |
| Hub快照 | 固定栈对象520B；同一64订阅旧collect为5次alloc/realloc，新快照0次；重复1000次新快照累计0次 |
| 生命周期与额度 | 快照持Arc期间对象保留，全部释放后控制用量回到(0,0)；旧State外持时destroy为tryAgain且第9Context被拒，释放后重试通过 |

固定登记相对空Vec基础体更大，满载时替代动态backing，不能由上述机制数据推导RSS下降或端到端提速。新测试的关闭回调占槽检查是状态机制验证，已有真实线程回调注销/关闭回归在本次全包单测中通过；11项ignored不冒充已运行。尚未重编最终DLL或完成main集成验证。

## 静态审查

已检查变更与调用上下文：登记锁后仍按原顺序锁订阅Data；快照离锁；Option槽只在填入Some后递增有效len，迭代器只读；不引入unsafe/Deref或手工析构。生产登记在同锁容量预检后push_reserved，不在域句柄登记之后新增可失败返回。Context登记容量依据域最多8个仍注册Context；外持CameraState保留Entry使destroy失败回插，不能先释放域Context名额却遗留活State。

测试辅助fixture仅扩大cfg(test)可见性，未扩大生产公开面。未改AM-RT、sources目录、SDK公共支撑层、原始来源清单或.gitignore；最终核心修订清单与OpenSpec由主代理集成同步。没有`.Codex/memory/patterns`目录，无额外项目Pattern可加载。

后续待验：ABI门禁、主线合并后测试、最终DLL、实际同候选压力及4h/24h；固定小容器改造不能替代这些验收。

## 主线整合补充

上述隔离待验状态已由主线整合复核更新，最终双DLL、329项工作区及跨进程/真实CPU推理见[整合记录](sdk-r3-foundation-integration-20261027.md)。原隔离日志与失败轮不覆盖。
