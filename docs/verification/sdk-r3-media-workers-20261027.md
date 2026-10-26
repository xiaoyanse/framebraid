# R3 Media Context工作线程预算与观测

2026-10-27；源码评审基线`2883a2b`，完整SHA绑定本阶段sdk-source-revisions.json。SDK main串行、本地未发布；未改AM-RT、不连接现场设备。R3 1.1o，MET-2 M-OBS/M-RECOVER，父任务1.1、完整硬件与4h/24h不由本次关闭。

## 旧问题与实现

旧Media已有单种资源数量限制，但各线程创建点没有共用Context额度，get_stats的threads字段一直为0。真实旧DLL在Digest抓图工作被屏障阻塞时，Windows已能枚举amc-capture，统计仍active/peak=0/0。

本地ABI1.1 r10兼容新增`amc_set_worker_budget`/`amc_get_worker_budget`、`AMC_CAP_WORKER_BUDGET`及常量`AM_MEDIA_WORKER_LIMIT=64`。无新结构/旧偏移变化。调用前核ABI/能力位，使用同提交的绑定；v0.1.0公开ABI1.0制品不替换。

- 每Context默认/硬上限64个SDK工作槽，含阻塞I/O和回调，不是CPU核数或自动调参。只允许收紧，同值幂等；0禁新增SDK线程。高于当前额度返回limitOutOfRange，低于当前在用/启动槽返回resourceExhausted，旧值不变。
- 所有CABI八类创建点共用同一账：源读包、懒像素worker、订阅推送、凭据、源事件、异步抓图、连续抓图及探测。预留在spawn前，失败释放所捕获资源；实际函数结束或panic展开才归还，不为取消/超时抢回活槽。
- 像素worker先拿既有decoder许可及工作槽，再分配其队列/时间表；资源不足不在每包上重复做未准入分配。探测分组按min(4,已配额)执行，0槽仍可读已缓存结果；其他活工作抢占时明确背压，不无限增线程。
- Media stats按本Context报告预留/存活SDK工作槽和历史峰值；收紧后历史峰值可高于新上限，不是新超额。FFmpeg thread_count=1使用已有调用worker，不重复计一次；同步宿主线程不占槽。驱动/原生库未知线程不能伪造为0或说受该上限控制。
- getter在Closing只读、Closed拒绝且失败清空输出；setter禁回调重入且仅Running可变。懒像素线程可能在订阅受理后才因额度失败进入资源等待，压缩交付继续；不能把订阅受理等同已交付像素帧。

直接Rust SessionPool没有CABI Context，仍按其自己的配置核查；该路径与不可管原生线程保留在BP12。不是将SDK工作槽宣称为整个进程OS线程硬限，也不改变推理原进程共享执行器/ORT预算。

## 验证与量化

3项新增单测：统一额度/在用收紧失败不改旧值/0槽拒绝、拒绝时捕获拥有者归还/线程panic归槽、两条受控scoped工作与第三条拒绝。Media全特性及无默认特性均56 passed、11 ignored；忽略项为显式原生/硬件用例，本阶段不冒充已跑。

真实Windows x64 Release检查：

| 场景 | 结果 |
| --- | --- |
| 旧DLL阻塞抓图 | OS有amc-capture，但stats=0/0，缺口实证 |
| 新DLL阻塞抓图 | stats=1/1；同Context再建拒绝且out ID=0 |
| 两个Context各配1槽 | 各自可持1条真实抓图线程，一域耗尽不侵占另一域 |
| 在用期间配0槽 | resourceExhausted，旧上限1保持，工作结束active=0/peak=1 |
| 空闲后配0 | 异步新线程拒绝，宿主同步抓图仍可正常返回 |
| 慢连续回调 | Closing期间active仍1；释放回调后归0，回调内setter按callbackContractViolation拒绝 |
| 单槽探测3条HTTP候选 | 串行完成，peak=1；随后0槽读取同缓存无新HTTP |
| Context Closing/Closed | Closing getter可读、setter拒绝；Closed getter拒绝且输出0 |
| Python/C#高层 | 实调默认64→2→0、非法/增额拒绝；C#实际P/Invoke成功 |

真实回环H.264 256×144/10FPS、SOFTWARE_ONLY：三轮每轮编码/像素均有回调，stats活槽/峰值5/5，Windows线程名正好为源读包、源事件、像素worker、两个订阅推送。每轮关闭后OS已无Media线程，stats=0/5。收紧为4后测试实走懒像素资源等待，编码持续回调、像素帧0、active=4而历史peak仍5，随后完整排空。MediaMTX/FFmpeg仅为本轮回环替身，已按拥有者退出，临时认证配置自动回收。

原Digest精确JPEG、CPU RGBA、持图寿命、取消终态、慢回调隔离、推拉互斥和错峰共享通过；抓图索引72/80字节受理/执行拒绝及缓存失败重试、保留20个Closed句柄通过。C#布局37结构/259字段、6项Python拥有权、示例编译及独立C#预算实调通过；这些不是R4五类档案验收。

## 性能观察

测前目标是额度/观测正确性，同负载延迟为保护项。各三轮本机Digest基准，8次预热、16次突发，计时Python异步提交至fetch，含HTTP/轮询/慢消费，不是纯CPU/模型性能。每轮单路16、预算内32、超额64有效/192拒绝，旧新分母及物理HTTP峰值1相同。

| 场景 | 旧三轮P95 ms | 新三轮P95 ms |
| --- | --- | --- |
| 单路 | 2.9479 / 38.8589 / 28.2603 | 18.2251 / 38.7135 / 2.0808 |
| 预算内双路 | 27.9620 / 42.8123 / 29.0040 | 27.5581 / 30.6551 / 2.7333 |
| 超额慢消费 | 89.1389 / 38.5512 / 30.7276 | 31.4122 / 30.7307 / 32.6412 |

原始尾值不删；小样本P99代表性false，没有额外复测到绿。不能从这些波动声称整体提速、CPU卸载或线程调度最优。可确认的改善为占用不再误报0，以及多类SDK线程有统一拒绝/归还边界。原native工作/宿主线程和OS整体资源仍分别测量。

## 合同、证据与审查

旧Media SHA=`6c256def81a417fbb788767d42dfb393e2efb58090f207fd0fec0869f5fa1610`，最终新=`77f6c8608d1eb02e86d13bfbf47f5b560ae18bb4d81c0ccf0d9d6b2a1aeab198`，均Windows x64 Release/shared-frames；新库为评审基线加本包dirty源码。Vision未改，沿用`155d1a798642806b36d6f70422974ebbe70bda05cfd35347368bb685167b84c5`。只冻结本地r10合同，不将runtimeAcceptance pending改通过。

私有主证据`.local/evidence/r3-media-workers-20261027/`保留目标、构建/门禁、全部慢轮和脚本初稿错误；同前缀baseline/candidate/final3/closing目录各存真实结果，回环进程退出记录在`.local/tests/r3-media-workers-rtsp-final/`。初稿误读probe返回顺序、把懒创建当同步拒绝、测试路径参数错误均保留并修正，不计为SDK回归；失败流程的自有服务也确认退出，不据失败覆盖原证据。

复现：`test-media-workers.py`显式DLL/输出，选回环时再传FFmpeg目录/MediaMTX/独立rtsp-work；`--observe-baseline`只允许旧库恰缺两个新增导出。`benchmark-capture.py --compatibility-baseline`仅同负载旧库对照，正常模式仍严格核全部导出。新Python/C#封装先核ABI/能力位；生成器同步头、常量、ctypes和P/Invoke，C#仅追加既有capability的托管布局，不改变native结构。r9冻结原字节保存，r10引用其SHA。

按代码、生成/绑定、文档三组审查：预留与失败回滚、析构/关闭、额度与锁序、回调重入、Ctx隔离、延迟像素拒绝与压缩优先、旧布局/导出不变。没有未处理致命项。入口函数仍有较长存量段落，不在本包夹带大重构；没有新增持续观测线程或每帧落盘。只在SDK main本地提交，不升级宿主、不推送/打标签/发布。

## 最终门禁与回收

全工作区首轮未配置`AM_RT_FFMPEG_DIR`，6项am-camera测试报告`media.ffmpegDirMissing`；原日志`workspace-tests.txt`保留。显式指向本仓已验证的FFmpeg候选后，`cargo test --workspace --all-features`通过：284 passed、19 ignored，无失败（含文档测试）；见`workspace-tests-configured.txt`。忽略项仍未运行，不升级为完整硬件验收。

Media严格clippy、fmt、diff空白检查通过；C头生成132函数、C#生成检查、来源220文件/零业务依赖/零外部本地路径、r10冻结11项及R3 OpenSpec严格校验通过。冻结工具的runtimeAcceptance仍为pending，整候选与长跑未因此关闭。

已核绝对路径、零reparse point及无在用进程，回收本包旧DLL对照副本与两个语言编译目录，共5,134,892字节；`cleanup-manifest.json`保留文件身份，`Workers.cs`与`language-results.json`先归档到主证据目录。删除后逐目录确认不存在；当前制品、运行库、原始对照结果和回环进程退出记录保留。只读清理检查初稿的PowerShell数组拼接错误已修正并重新核验，不把错误输出的零链接数作为删除依据。
