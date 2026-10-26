# R3：RTSP转换前预留、探测预算与通知条目计费

## 范围与目标

评审基线`3cdee3c240ed6d872e27658356844d671a2cac01`，由本包`sdk-source-revisions.json.reviewBaseCommit`登记；以R3子任务1.1e收口上一包定位的两处缺口及同类探测入口。仅SDK main本地实现，不改AM-RT、不发布、不连接现场设备。

MET-2 M-RECOVER/S5：主目标为SDK配置/包转换分配前准入、错误与输出保留期间持续计费、通知实际条目不漏计。保护载荷正确性、既有数量/字节上限、正常流有效交付与慢消费者隔离。此为资源约束修复，不宣称推理提速、CPU卸载或进程RSS下降。

## 改前与改后

| 场景 | 改前事实 | 本包结果 |
| --- | --- | --- |
| HTTP/RTSP探测，仅64字节帧域额度 | 旧Release两类均返回OK；HTTP实际发起1次请求 | 最终Release两类均返回Pending；HTTP请求数0，新建默认预算Context后均OK |
| 推理通知条目 | 固定预留32字节、队列字节除16；本机完整元组实际48字节，临界测试失败 | 按size_of完整条目计费，47字节拒绝且零残账、48字节只容1项，归还后为0 |
| RTSP转换与配置 | C消费层在framing复制完成后才给载荷计费，配置/暂存未通过同一准入 | Rust兼容新增预算回调，配置复制及有界framing工作区先准入；C适配拆分配置/包额度，错误保留许可直到Reader销毁 |

探测对照使用新建Context，不违反只降不升的set_limits合同。第一次测试工具错误尝试恢复已降低的额度，得到limitOutOfRange，未把该工具失败算作SDK缺陷；修正为逐场景独立Context后保存改前、改后全量结果。最终候选Pending是既有资源不足候选状态，不改成网络不可达或认证失败。

## 所有权与测量边界

- Rust新增`Reader::open_with_budget/read_packet_with_budget/retained_buffer_bytes`；旧入口保持调用方管理预算。预留按有界输入长度和保留配置计算保守工作区，不固定为每源16 MiB，也未消除既有包转换复制。
- C `BudgetedReader`字段先销毁Reader再释放许可。成功后将同一预留拆给配置Vec与输出Vec容量，归还暂存余量；失败后不复用该Reader，余量留到其销毁。消费层局部绑定同样保证早退时先释放包、再归还额度。
- 原生FFmpeg、网络库、驱动和allocator元数据另列，不能从SDK计费推导进程RSS硬限。通知此处核完整活条目；VecDeque空闲容量等完整元数据仍随父任务1.1继续审计，不把窄修复宣布成全内存封闭。

真实原生Reader回环25包：初始配置容量68字节；输出容量峰值8,129字节。前一包跨下一次读包保持引用，逐项核Reader配置+前包+当前包计费；Reader关闭后末包仍可读，所有相关对象释放后计费0。低预算拒绝发生在转换前，后续同一失败Reader明确Closed。仅合成H.264、256×144/10 FPS，不覆盖全部编码/硬件/驱动故障。

## 最终Release回归

同源健康/慢消费者压力/恢复各约2秒，有效新鲜帧率分别9.46/9.07/9.18 FPS；压力帧龄最大50 ms、相邻交付最大143.23 ms，峰值8项/463,142载荷字节，无重连，慢编码消费者从关键帧恢复。保持既有≥7.5 FPS、≤750 ms目标；相近历史短测只作保护项对照，不据此宣称速度提高。

真实CPU ONNX Identity结果及共享帧回归通过：未取结果和外持结果压力下另一Context各50次输出逐字节一致；阻塞通知仍执行50次，同Context另一模型50次正确，通知丢弃35次明确记录，注销后无迟到回调。活共享槽持有约5.09秒期间30次发布明确拒绝，另一池30次发布并真实推理，归还后恢复。Identity验证调用与资源合同，不代五类YOLO精度/加速后端验收。

HTTP正常探测回归覆盖Digest、认证隔离、强制刷新、缓存时效、八候选并行峰值4、超时/失败分类和Closing外持。最终core sdk_io 11项、Media FFI 33项通过/11项需显式材料ignored、Vision FFI 12项通过；另单独显式执行上述原生Reader测试通过，不将ignored统一计通过。

本包新通知测试最初污染全局线程配置，导致既有选择测试失败；现改独立Domain，默认并行全组通过。clippy测试模块顺序问题已修正；未以串行测试或lint豁免掩盖。

## 身份与复现

旧Media Release SHA-256=`e17de0b9f276374030e0fddf7b9f1edbb0c12c90689380c253f066a9663101e0`。
最终Media=`238ecc9ef91c273da23bfc53c34c918859362002b31e99becdf24c300a4de645`；Vision=`212f69dc8a5e08494bd84f0c1dc74371a6230aafa2f3e0d8a8260f3c2269633a`。评审基线加本包dirty源码、Windows x64 Release/共享帧构建，非公开制品。

Identity模型=`d42637c177787ea869125e6543b3305442deb3aaadb9f6c6d8262c57bebb0b17`，CPU ORT=`66e62f7bebf65b048ff1a93c40a73df93c36f93ce5bb085076de92b1a1e754fd`。FFmpeg取既有本仓候选运行库，回环发布用MediaMTX v1.21.3与合成编码器；无现场地址/客户图片。

```powershell
python -B scripts/sdk/test-reader-reservations.py --runtime-dir .local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/ffmpeg --mediamtx .local/tools/mediamtx-h2/mediamtx.exe --work-dir .local/tests/r3-reader-reservations-new
python -B scripts/sdk/test-probe-budget.py --dll-dir .local/build/current/release --runtime-dir .local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/ffmpeg --mediamtx .local/tools/mediamtx-h2/mediamtx.exe --work-dir .local/tests/r3-probe-budget-new
```

旧DLL对照加`--observe-baseline`只记录事实，不冒充候选通过。正常压力/真实推理使用现有`test-camera-pressure.py`与`test-inference-pressure.py`；原始结果位于私有`.local/evidence/r3-reader-reservations-20261027/`和`r3-reader-inference-pressure-20261027/`。

本轮7个回环测试目录核绝对路径、零reparse point、所属进程均终止；39文件77,304字节逐文件SHA一致复制归档后独立删除，另行核验已不存在。初次工具失败与中间候选结果一并保留，不筛选成功数据；既有运行库、模型及正式制品未动。

## 审查、文档与剩余

按核心预算、C适配/所有权、测试/文档三组自审；未发现本包未解决的致命问题。关注过的析构顺序、错误后配置保留、配置容量增长、原生AVPacket unref、控制条目拒绝不改变推理终态均核调用上下文。项目无`.Codex/memory/patterns`，不虚构Pattern检查记录。

同步ABI行为说明、Rust入口中英文README/接入、来源覆盖、CHANGELOG、R3设计/spec/tasks、路线与验证状态。C签名/布局/能力位不变，生成物和冻结历史不改；支持平台、第三方依赖、Python/C#调用签名及既有示例无需变化，理由为未新增C入口或支持承诺。

fmt/clippy、C/Rust/Python/C#生成检查、ABI冻结11份、来源边界202文件和OpenSpec严格校验均通过。R3父任务1.1与1.5/1.6保持开放，H2/H5完整硬件矩阵、R4及4h/24h未验收。当前仅完成1.1e的局部修复，不称完整R1～R5优化结束。
