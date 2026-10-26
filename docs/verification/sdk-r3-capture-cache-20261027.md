# R3 ISAPI 连续取图历史频率缓存回收

## 缺陷与测前目标

基线源码`b09fd82`，其中Media DLL生产源码仍对应`d64eadd`。本地Digest HTTP替身返回242字节合成JPEG，保持一个60s慢订阅；另外依次创建、取首帧并排空20个不同频率订阅（900→140ms），不连接现场。每轮停止后只应留下仍活跃的慢频率缓存，最后全部排空应为0。

MET-2 M-RECOVER/S5：同一帧域预算收紧32 MiB、同一图片/请求参数，主目标是消除已停止频率占用及恢复健康抓图准入，不以微型JPEG耗时或RSS下降判收益。控制/外持预算保持原值。观察器baseline模式保存已观察失败而不要求20轮成功；candidate的`--require-complete`明确要求20/20、停止后缓存峰值≤1、单次健康抓图成功，不把观察脚本退出0当修复通过。

代码追踪发现：缓存键包含源×频率，但引用计数只按源，停止一个频率后需等其他消费者全部退出才清理。更早的leader先插缓存、外层才检查容量，使外层容量检查不能约束新键。慢频率刷新还可能写回历史快频率键。每个Raw保留既有4 MiB HTTP预留，因此即使小JPEG也会持续挤占域预算。

## 修复与量化

| 项目 | 修改前 | 修改后 |
| --- | --- | --- |
| 默认预算20轮切换 | 20轮后遗留21个缓存 | 归属机制由下述同预算三轮验证，不借默认预算扩大证明 |
| 相同32 MiB、20轮目标 | 只完成6轮，下一轮返回ResourceExhausted；停止临时订阅后仍遗留8个缓存 | 三轮均20/20，停止每个临时订阅后只保留1个活缓存 |
| 同时存活的慢订阅旁新增单次抓图 | ResourceExhausted（错误码8） | 三轮均成功，不扩大32 MiB预算 |
| 所有订阅排空 | 缓存可清零，但需要关闭健康消费者 | 不必先停止健康消费者即可回收历史项；最后排空仍为0 |

内部引用计数改为源×频率，最后同频订阅排空立即回收该频率缓存，其他频率/外部读者仍有效。leader/follower缓存提交共用短锁顺序（归属→缓存），提交前核活归属和Context状态，晚到旧帧不能覆盖更高frame_id；重新采集写本订阅频率，兼容且新鲜的其他频率图片仍可共享。保守HTTP预留、共享采集器、公开签名/默认值、外部release语义都未更改。不通过盲目缩小记账或自动提高预算掩盖故障。

缓存项8→1为同预算受控场景的逻辑占用；不等于RSS减少28 MiB，也不证明所有硬件或真实相机容量增加。三轮JPEG SHA-256相同=`e212b0366d978259be0c66486c3c694d3911526b9051d10c4b90c1819c826ad6`。

## 验证及身份

- 4项Rust定向回归通过：100种频率回收、同频两消费者最后一个才回收且读者仍可读、旧帧/关闭后缓存不复活、100次并发提交与最后消费者退出竞争；隔离域每项最终占用归零。
- Release真实C路径：同预算三轮20/20全部满足目标卡；完整`test-camera-capture.py`通过JPEG字节、RGBA、异步fetch/ack、取消唯一终态、推拉互斥、回调屏障/慢订阅隔离、兼容消费者共享、ContextClosing等回归。错开消费者仅5次物理请求，接收时间/身份保留；不是每个消费者独立抓一次。
- `cargo fmt --check`、受影响camera-ffi全部feature的clippy、C/Rust/Python/C#生成、冻结、源码边界和OpenSpec严格校验通过。无新增外部依赖或ABI演进；未做重复全仓编译。

基线DLL SHA-256=`a674f4f7a9c03d17b06dd33806f07c832cfa08012357349e93d4711fefcd3eab`；候选=`a916f7fa2f7d2d34204ceaa776b74cc68a5f3b52b7ff7de4791c1ce4a97e2b9a`。候选由`b09fd82`之上的本轮dirty源码Release/共享帧构建；源码修订清单及私有`source-review.json`绑定前后摘要，不把HEAD短号当作构建已包含提交。

原始证据位于`.local/evidence/r3-capture-cache-{baseline,baseline-tight,candidate-1,candidate-2,candidate-3}-20261027/`；公共目录`.local/evidence/r3-capture-cache-20261027/`保存源码前后指纹、基线DLL、回归stdout及门禁。基线DLL为本轮受控对照保留，后续归档按身份核对，不修改已发布制品。HTTP替身、CLI造图和测试工作均结束，没有启动AM-RT服务、业务库或现场设备。

```powershell
python -B scripts/sdk/test-camera-capture-cache.py --dll-dir .local/build/current/release --output .local/evidence/r3-capture-cache-rerun --require-complete
python -B scripts/sdk/test-camera-capture.py --dll-dir .local/build/current/release
cargo test -p am-camera-ffi --all-features --lib captures::io::tests -- --test-threads=2 --nocapture
```

## 审查、同步与未完成范围

检查归属/缓存锁与flights、subscriptions、回调队列的调用顺序；HTTP请求、解码和用户回调均不在新短锁内执行。回收仅撤销内部缓存引用，不强制归还活外部图；共享频率统计仍按同源所有活消费者求和，不把频率键数误当消费者数。测试脚本保留失败输出、只在回环造图，输出路径拒绝已存在目录和链接。

同步ABI正文的回收约束、R3设计/规格及子任务1.1c、来源修订、路线、状态、接入和CHANGELOG。历史提取指纹与ABI冻结不重写；README支持范围/语言绑定/第三方材料无变化依据为纯内部回收修复，没有新增公开能力或依赖。

本项是完整预算审计中发现的一处实际缺口，不将父任务1.1勾选；其他中转暂存、全链路预算/统计覆盖、同候选4h/24h、硬件和整包收口继续开放。
