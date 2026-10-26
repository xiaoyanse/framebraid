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
