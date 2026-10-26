# R3 慢消费者、结果积压与共享槽隔离验证

## 对象、目标与边界

源码检查点`f6c5846`；本阶段只增加验证工具、统计读取字段和记录，无SDK生产源码、依赖、公共ABI或宿主变更。实测Media DLL仍为`d64eadd`构建，Vision为R3分类预算候选；相关生产源码未变，不把当前测试提交当作二进制重新构建身份。两个DLL摘要与[上一阶段](sdk-r3-c-recovery-20261027.md)相同，目标卡及私有记录保存完整身份。

本包按MET-2 M-RECOVER验证S5受控压力与局部组合，不宣称完整S4采集→推理整链承载。视频使用随机回环端口、自产256×144/10 FPS H.264、软件解码；同一源有健康压缩/像素订阅、停读的压缩订阅和阻塞的像素回调，事件回调同时阻塞。没有复制相机连接来制造隔离。

测前预算：三轮各正常/压力/恢复4s；健康新鲜画面≥7.5 FPS、接收帧龄及相邻交付间隔≤750ms，聚合队列≤50项/12 MiB。50项来自四条订阅8+2+2+2项和内部像素队列36项，字节为场景包络，非RSS。各窗口排除开始前接收的像素帧，不把清理积压算新增吞吐。推理为CPU Identity、每次健康调用≤2s；结果缓存收紧2项、控制通知16项；一槽共享池活借用保持≥5s、拒绝覆写，另一个池和真实推理继续。短持有验证租约不因等待超时被强制回收，不代替4h/24h趋势验证。

## 实测

| 场景 | 结果 |
| --- | --- |
| 视频正常/压力/恢复 | 三轮分别9.29～9.55 / 9.35～9.60 / 9.60～9.85新鲜FPS，满足各自测前预算；不从短窗口波动推导提速 |
| 同源慢消费 | 压力期健康帧最大接收龄51ms、最大交付间隔157.96ms；压缩仍交付，重连数保持0。慢帧队列丢旧累计41～42，压缩重同步累计9次；不把主动丢弃当健康丢帧 |
| 队列及恢复 | 压力聚合峰值7项/602,773字节；慢压缩消费者重新拉取首包为IDR/SPS/PPS，解除回调阻塞后收到新鲜画面。注销后回调不再增加，排空后队列/外部持有/具名媒体worker归零 |
| 结果不取 | 2个完成结果占3,900字节，第三个请求得到唯一Failed/resourceExhausted终态；另一Context完成50次正确推理。不是无限接受，也不把执行期失败写成准入拒绝 |
| 结果已ack仍被外部持有 | 持有结果继续占1项/1,950字节；把结果配额收紧1项后新请求仍Failed，另一Context另完成50次。最终release后原Context恢复10次正确推理；14个接受=14个执行，其中2个失败，缓存和外部占用归零 |
| 慢结果通知 | 一个通知回调停住，原模型仍完成后续50次，另一模型在同Context完成50次；结果字节正确，控制通知超额丢弃35次可查，101次执行无失败。注销丢弃未消费通知不撤销已完成请求；不把通知丢弃当推理失败 |
| 活共享槽 | 实际持有5.09s，30次同池发布明确QueueFull、未覆写；另池30次发布/读取、共享视图30次真实异步推理成功，与普通内存复制输入结果逐字节相同，帧ID/源代次/配置代次一致；归还后可再次发布 |
| 跨进程补查 | 当前DLL四种像素格式字节一致、只读页、20次池生命周期句柄150→150（保存stdout复测151→151）；活慢消费者不抢回，核实退出才回收，拥有者退出后既有视图仍可读；遗弃互斥量明确拒绝复用并保持安全Closing |

最终推理复核健康普通调用最大0.175ms、通知阻塞期最大0.137ms；共享输入调用最大3.55ms（包含异步查询/取回），均为微型模型、Python边界局部耗时，**不是YOLO耗时、吞吐上限或加速收益**。最终结果/通知/共享场景检查各一次；之前三轮5s共享持有及重复压力原始记录全部保留，没有删掉慢轮。模型SHA-256=`d42637c177787ea869125e6543b3305442deb3aaadb9f6c6d8262c57bebb0b17`。

## 复现与证据

```powershell
python -B scripts/sdk/test-camera-pressure.py --dll-dir .local/build/current/release --runtime-dir .local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/ffmpeg --mediamtx .local/tools/mediamtx-h2/mediamtx.exe --work-dir .local/tests/r3-pressure-rerun --seconds 4
python -B scripts/sdk/test-inference-pressure.py --dll-dir .local/build/current/release --runtime-dir .local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/onnxruntime-cpu --tensor-dir .local/tests/r1-baseline-inputs/tensors --output .local/evidence/r3-pressure-inference-rerun
python -B scripts/sdk/test-shared-frames.py --dll-dir .local/build/current/release
```

视频三轮和工具冒烟原字节归档于`.local/evidence/r3-pressure-isolation-20261027/`，38文件478,737字节逐文件SHA一致，`transfer.json`映射原路径。5个本轮测试目录已回收并核验不存在，工具拥有的MediaMTX/FFmpeg全部退出。推理重复记录在`.local/evidence/r3-pressure-inference-reviewed-{1,2,3}-20261027/`，最终带普通内存结果对照/身份检查在`r3-pressure-inference-final-review-20261027/`；运行库、模型和正式制品保留。

保留两次工具错误：第一次媒体观察器未读取frames_dropped字段导致KeyError；首次共享测试误把仅异步允许的BORROW_RETAINED标志传入同步入口，SDK正确拒绝InvalidArgument。补全观察字段并使用真实异步入口后通过；不计为SDK缺陷或隐藏失败重试。

## 审查与同步

审查关注：每个回调函数指针活到注销完成，释放屏障先解除再关闭；只复制借用期数据；共享视图直到推理实际归还后才return，未对只读映射做写入。超额请求核对唯一终态，不对destroy后对象做UB测试。测试函数按完整场景组织，较长但不引入生产抽象。输出路径拒绝既有目录和链接，故障日志只含合成回环参数。AST/CLI、C/Rust/Python/C#生成、ABI冻结、来源边界、OpenSpec严格检查通过。

更新任务1.3、路线、状态、接入和CHANGELOG；无生产接口/默认行为或能力声明变化，源码修订指纹、冻结、README支持范围、第三方材料无需变化。结果配额隔离边界是Context；同Context仍共享总配额，一槽池被活借用时仍应拒绝覆写，不承诺同池无限继续发布。

功能/安全与受控隔离预算通过；无代码优化前后收益声明。完整预算审计1.1、同候选4h/24h的1.5、整包1.6及H2/H5实际硬件矩阵继续开放，不将本机短测或具名线程计数当长期稳定/原生驱动全覆盖。
