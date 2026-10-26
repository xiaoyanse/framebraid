# R3 C源恢复、真实启停与请求归还验证

## 对象与测前目标

源码基线`d64eadd`；本阶段只新增验证工具和记录，不改SDK生产实现、依赖、公共ABI或AM-RT。相机DLL由本基线Release/共享帧配置重新构建；推理DLL复用已验R3预算候选，其对应核心源码未变。精确DLL/FFmpeg/ORT/模型/工具指纹见私有证据，不用Git短号替代二进制身份。

MET-2 M-RECOVER / S5：随机127.0.0.1端口、自产H.264 256×144/10 FPS、软件解码策略、一个C源及编码/像素订阅。测前卡约定恢复首个新鲜帧≤10s，首帧后5帧≤2s，单次关闭≤8s；排空后SDK具名worker、迟到回调及队列/外部持有均为0。这是本地故障场景预算，不是生产SLA或最大路数承诺。

退避另按失败结束→下一次连接开始计时，排除原生打开耗时：initial100ms/max400ms/jitter0/max_attempts5；最多五次各5s原生打开，观察总预算30s。观察器间隔下界允许15ms测量误差，上界保留1s调度余量；本轮实测相对设定值偏差不超过6.6ms，不将观察器宽容差当生产时限。恢复及关闭预算不因重试标定调整。

## 实测结果

| 项目 | 结果 |
| --- | --- |
| 实际故障循环 | 20/20：10次终止自产发布者、10次更换错误凭据后显式恢复；不是仅操作内存状态 |
| 恢复首帧 | 最小919.78ms、中位1111.86ms、最大1509.45ms；从启动恢复操作前计时 |
| 恢复窗口 | 各轮新代次5帧，首帧之后最长510.17ms；source_id不变，frame_id唯一递增，received_at为恢复操作之后同机QPC时间 |
| 压缩参考链 | 各轮首个恢复压缩包为新代次关键帧，含H.264 IDR/SPS/PPS；故障期间旧像素队列已清空 |
| 认证等待 | AUTH_WAITING与AUTH故障明确返回，等待期重试计数不增加；服务日志共10次拒绝，对应10次错误凭据，没有重试风暴；显式更新才恢复 |
| 退避/停止 | 最后三次实际等待203.54/406.52/403.76ms，对应200/400/400ms；第5次失败后观察期没有新尝试 |
| 真实源生命周期 | 100/100源启动、真实编码/解码回调、关闭/销毁，另5轮预热；不是100次空Context初始化 |
| 关闭 | 中位65.60ms、P95 109.48ms（100样本排序第95项）、最大112.52ms |
| 排空 | 每轮30ms静默观察回调计数不变；逐轮SDK具名媒体worker=0、队列项/字节与外部持有项/字节=0 |
| 回调竞争 | 每10轮阻塞第一次帧回调；回调内禁止的close明确拒绝，外部unsubscribe(0)保持Closing，释放回调后才完成 |
| 基础误用/并发 | 400次并行Context生命周期、重复关闭与外库句柄拒绝；76结构布局与77/53导出检查通过，不作为全部接口功能验收 |

10次队列/像素工作定向回归通过：包括阻塞像素执行不挡压缩队列、溢出后旧epoch不迟到、真实读取归还前保留decoder许可、注销等待回调及保留包、关闭唤醒pull。原语合成结果和实际C/RTSP结果分开保存。

推理补充为真实CPU通用模型，模型族精度不在本项范围：400次四线程fetch/release，结果TTL收紧至40ms后请求明确Expired，外部指针仍可读；模型Closed后新提交拒绝且请求ID清零，重复close安全，持有结果使Context保持Closing，最后release后排空。原生两项批次在输入归还屏障前取消其中一项，取消不会提前释放仍在读取的输入，另一项正常完成，retain/release=2/2，终态在实际归还之后。推理具名worker排空后为0。没有调用合同明确为UB的destroy后句柄。

## 资源及证据边界

100轮逐次记录私有提交字节、进程句柄和具名worker。预热后PrivateBytes=20,766,720，结束21,078,016（增加311,296字节）；逐轮范围20,402,176～22,687,744。句柄预热216、结束214，逐轮214～216。**不把首尾差或短序列当作无泄漏证明**：Python观察器保留逐轮记录、分配器缓存及SDK隔离区均有寿命，归因和4h/24h趋势仍需长跑。SDK命名线程观测不冒充FFmpeg/驱动所有内部线程覆盖；公共Media线程统计完整性仍在1.1审计。

本轮验证现有恢复机制，未制造“优化前后提速”数字；原始故障路径只与测前预算对照，不拿失败期间低吞吐比较正常性能。CPU卸载、其他编码/硬件矩阵、多源容量和长期稳定性均未由此验证。

## 复现及原始材料

```powershell
python -B scripts/sdk/test-camera-recovery.py --dll-dir .local/build/current/release --runtime-dir .local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/ffmpeg --mediamtx .local/tools/mediamtx-h2/mediamtx.exe --work-dir .local/tests/r3-recovery-rerun --fault-rounds 20 --cycles 100 --warmup 5
python -B scripts/sdk/test-inference-recovery-ownership.py --dll-dir .local/build/current/release --runtime-dir .local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/onnxruntime-cpu --tensor-dir .local/tests/r1-baseline-inputs/tensors --batch-dir .local/tests/r5-batch --output .local/evidence/r3-request-recovery-rerun
python -B scripts/sdk/test-ffi-lifecycle.py --dll-dir .local/build/current/release
```

测试专用工具在本机Python3.13.14、MediaMTX1.21.3执行，需显式提供本机私有运行库和合成模型；新输出目录必须不存在。小冒烟参数会如实写入结果，不能冒充20/100完成。

私有归档`.local/evidence/r3-c-recovery-20261027/`包含full、前置冒烟、review、目标卡、原始逐次数据、退避事件/状态、最初观察器源码及摘要、400次基础生命周期、10项Rust定向回归和门禁。`.local/evidence/r3-request-recovery-reviewed-20261027/result.json`保存最终推理过期/取消/排空证据。相机DLL SHA-256=`a674f4f7a9c03d17b06dd33806f07c832cfa08012357349e93d4711fefcd3eab`；推理DLL=`aab058431dd4c485ea2d2f3d0953e79dc4189deed5ef8b0860a2538d7484d537`。

保留一次工具标定失败：最初把五次原生连接也塞进10s退避观察窗口，未完成；后按原生5s/次合同独立观察30s，并测真正的失败后等待，不放宽恢复10s预算。全循环后复核工具，将五帧窗口起点严格放到首帧实际返回处；全20条原始差值最大510.17ms已满足同一2s目标，另跑2故障/2启停复核。完整原观察器和后验检查保留，不以小冒烟替代全量结果。

拥有的12个MediaMTX/发布者进程均已退出；没有结束其他项目进程。合成认证配置自动移除，28个文件171,299字节逐文件SHA一致归档后，回收本轮5个测试子目录并验证不存在；保留输入模型、运行库、正式制品和原始证据。

## 审查、同步及仍开放项

审查新工具的回调所有权、异常路径、子进程回收、输出路径/链接约束与测量边界；回调只复制借用期元数据或通过允许的getter读取，不从回调阻塞调用close；有意测试禁止重入仅验证明确拒绝。拥有者回调/SDK工作真实结束前不销毁句柄。测试读取过程中没有现场地址、业务库或用户图片，没有新增依赖。

同步本记录、路线、状态、CHANGELOG、接入测试入口和OpenSpec 1.2/1.4；CLI参数解析/AST、生成、ABI冻结、源码来源边界及OpenSpec严格检查通过。无SDK生产源码变更，历史提取指纹和源码修订清单不重算，C ABI/绑定/README能力/支持矩阵/第三方材料无需修改。没有发布或更新宿主固定依赖。

1.2公共恢复与1.4生命周期验证通过；1.1完整预算/统计覆盖、1.3慢消费者/共享帧组合、1.5同候选4h/24h与1.6整包收尾继续开放。H2/H5硬件专项目标仍独立，不将本机软件结果当作其完成证明。
