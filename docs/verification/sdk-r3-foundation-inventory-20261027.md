# R3固定基础对象与剩余容量核查

本表按69624cf源码观察建立，探测动态存储/模型IO次序缺口已由本轮probe记录修复。它不是父任务1.1或同候选压力验收签收，不是进程RSS上限。

| 基础对象 | 数量/单项边界及拥有者 | 尚需量化/验证 |
| --- | --- | --- |
| Context与runtime dirs | 每库8个Context，16目录×最多4095 UTF8字节，随Context最终销毁 | Vec/String实际capacity及初始化失败，非控制队列消息 |
| CameraState/Usage/Metrics | 每Context各一；服务TypeId固定16槽，当前Media6种/Vision3种；close hooks8槽 | 服务本体固定尺寸汇总，frames STATES高水位 |
| 域Source/SharedSource/别名/Hub | 域4096句柄，活源每Context16，订阅每源64；URL/当前凭据均4096扫描限制，死亡Weak在新增清理 | Vec高水位、Closed外持、真实注册失败、全域临时快照 |
| Vision State/Usage/backend表 | 各服务一份，backend固定3行，模型每Context8个；IO每向32/name4096/rank8 | 多份ModelInfo名称/shape容量基础成本；数量前置已修 |
| Shared Ledger/消费者/借用 | 4池/8消费者/32借用及域4096；正常return/detach/reap移除 | verified清理前后重叠、HashMap backing/失败；不假设瞬时len永远8 |
| diagnostics | 每Entry一个static CStr详情、init固定256；选择最多3尝试且只稳定错误键 | 无错误日志累加，不误报无限诊断字符串 |
| 硬件报告 | CUDA最多16设备×固定256原名字、Media最多8适配器×2后端，报告缓存有许可 | 原生/serde临时结构包络及同候选证据，不把存在显卡当执行 |
| stats/event等临时快照 | 受域句柄/64订阅/既有记录上限约束 | 并发宿主调用各自产生临时副本，须列账或改固定栈；宿主自建线程不受SDK硬限 |

已计费的请求记录/Flight/通知/decoder/Reader/RTSP/结果载荷见预算矩阵，不重复修同一容器。不将上述所有配置/固定服务体强塞64KiB控制消息额度，也不提高该默认值来掩盖漏账。固定数量不代替实际capacity和失败准入验证；因此父1.1暂不勾选。

后续1.1u已修共享源别名Table实际容量/增长重叠，见[记录](sdk-r3-alias-storage-20261027.md)；原表中的全域Source/SharedSource登记、Hub和临时快照仍待，不抹去初始观察。

基础包整合已完成pending目录准入、固定8状态/64Hub快照、共享4/32/8表及全域源4096预留，具体边界/取舍/329项及真实DLL见[整合记录](sdk-r3-foundation-integration-20261027.md)。剩余重点为事件/stats临时快照、模型IO多份元数据fallible复制与服务固定本体汇总；不再将已收口项重复列为待实施。

后续模型FFI元数据fallible复制/实际容量和每Run校验克隆、来源事件/stats临时快照已收口，见[模型记录](sdk-r3-model-metadata-20261027.md)及[快照记录](sdk-r3-source-snapshots-20261027.md)。剩余仅为固定服务体/核心与原生元信息、硬件JSON暂存及宿主调用栈的边界汇总，非新增完整硬件或长跑门槛。

固定基础体、Media/Vision报告与原生/宿主边界已补测，见[报告记录](sdk-r3-report-storage-20261027.md)。本机all-features Context1224B、Media基础服务含Arc952B/Context、Vision392B/Context；Media实际7服务仍在16槽内，旧6服务观察已过时。最后新增模型文件读取竞态仍需修复后关闭父项；不恢复已撤销硬件矩阵/固定时长门槛。

后续同候选核查完成：SDK文件预检查采用打开句柄限定读取，文件增长/缩短明确拒绝，内存输入借用；354项、CPU压力及回环/共享帧见[记录](sdk-r3-model-read-20261027.md)。本次R3按SDK受控范围收口，ORT重新按路径打开、外部权重可变和原生/宿主内存不在SDK硬限内，仍须调用方保护文件，后续安全加固未完成。旧文中的长跑要求为历史，已由用户取消。
