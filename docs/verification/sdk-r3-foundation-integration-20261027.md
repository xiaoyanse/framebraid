# R3基础容量整合验证（2026-10-27）

评审基线 `5f32964`。三个隔离包均经主代理审查、统一编译及真实merge，保留原提交：初始化 `af11c82`、共享帧 `face361`、Hub `f516b7f`。另补全域Source/SharedSource基础登记在发布前fallible精确预留各4096槽、满额拒绝；不扩大Control队列预算。

## 主目标与量化

按MET-2核基础容量与失败原子性，不以推理耗时衡量此包，不宣称RSS或整体吞吐下降。

| 改动 | 结果与代价 |
| --- | --- |
| 初始化目录准入 | 在册+pending≤8，复制前RAII；最大目录backing每Context65904B、合计527232B，另计Context/Arc/allocator；饱和拒绝旧目录复制机制17次/65904B→新0/0 |
| 全域Source/SharedSource登记 | 各4096×8=32768B，按需首次精确预留，保留到卸载；满额不发布新句柄。冷启动明显大于旧小Vec，不隐藏基础开销；不把此64KiB总和算成Context载荷 |
| 共享源别名 | 16B首槽，扩2槽需48B重叠、最终32B；旧源/claim不因失败受损，关闭不退空闲容量 |
| CameraState登记 | 固定8槽128B（含Mutex136B），登记初始化/重复lookup无索引堆分配；活State使Context destroy返回tryAgain，不提前腾第9Context |
| Hub登记/快照 | 固定64槽520B/栈快照520B；同64订阅旧collect5次alloc/realloc→0，重复1000次仍0；已停止但回调未退出仍占槽，快照释放锁后投递 |
| 共享帧三表 | Ledger4槽160B、initial32槽768B、verified8槽320B；先清旧缓存再装新8，不以进程无法打开证明退出；buffer登记后不再有可失败分配 |

固定结构相对空HashMap/Vec增加基础体积，实际Map/Process/Arc和共享区另计；没有消除所有临时分配。全域事件/stats快照、模型元数据等剩余见[清单](sdk-r3-foundation-inventory-20261027.md)，不关闭父1.1。

## 同候选整合验证

私有原始数据 `.local/evidence/r3-alias-storage-20261027/`；隔离包首轮编译错误与非法合成fixture失败均保留，修正未放宽断言。

- `cargo test --workspace --all-features`：329通过、0失败、21显式忽略；未把忽略项当通过。
- support/camera-ffi/vision-ffi 全特性全target严格clippy通过；两库Release构建通过。
- 最终Media DLL SHA256 `a10656c25a5cf4db7c5d9e14defb137e95a033a5c7d48aedcb1380f3571b74da`；Vision DLL `b141d9d9ea233861c1ac14fe64d1f8275d1ff5f1006452f53b34abdd59cc0a17`。源码为整合提交加本次Source全域登记差异，构建时dirty标记不冒充tag/发布身份。
- 真实两DLL400次并发生命周期、76结构布局、79+53导出；真实别名15/47/48字节临界与257次同源开关通过。
- 共享帧跨进程只读映射、活慢消费者保留、消费者退出回收、owner退出仍可读旧view、废弃锁/故障关闭退化通过。
- Python/C#像素拥有者、GC/关闭竞争、双模型同步+异步真实CPU Identity推理/归还保护通过；不是五类模型精度验收。
- 实际HTTP/RTSP回环worker额度、阻塞Closing与Context隔离通过（main-media-workers.json），不接现场设备。

## 审查与同步

致命/阻断项0：目录借用只到调用返回，准入计数同锁移交；pending保护卸载；源登记先预留、claim失败归还；Hub安全Option栈快照无新增unsafe；共享初始借用槽在buffer转移前准备，lease登记失败仍RAII归还。C ABI/生成布局及默认数值不变，公开调用流程/模型档案/语言示例无需改写；CHANGELOG、状态、路线、OpenSpec与原始来源修订同步。

这些组件证据不代表完整R3或R1～R5完成。完整硬件矩阵与固定4h/24h长跑按用户最新决定另行移出强制验收，未执行不记为通过；实测范围继续如实描述。没有改AM-RT、推送、标签或发布。
