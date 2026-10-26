# R3 推理分类字节准入隔离

## 目标与边界

源码基线 `a4f9c12`，候选为本记录同提交。按 MET-2 M-RECOVER / S4-S5，主目标是慢输入归还期间，不把其他通道或 Context 的用量套用当前通道的较小字节上限，误拒绝仍有额度的健康工作。不是推理提速，也不增加总预算。

测前目标卡与原始证据在 `.local/evidence/r3-budget-isolation-20261027/`。仅合成 CPU ONNX Identity 模型与受控输入归还屏障，不连接现场设备。保持真实输入有效直至归还，不用强行取消或提前释放制造空闲额度。

## 问题与修复

原准入以共享执行器 `data.bytes` 对比当前提交类的 `max_bytes`；离线规划元数据同样如此。其他类或其他 Context 已占用较大额度时，会阻断较小上限内本可接收的健康工作。

修复为三级字节校验：本 Context 实时/离线分类暂存、本 Context 两类合计、共享执行器 64 MiB。离线规划元数据与输入一起占离线额度；输入归还回调未返回时仍占原额度。项数仍使用既有模型×槽、单次批量和执行器域限制，不改默认值、C ABI 或能力位。拒绝不更新任一本账；同一锁序短锁更新，复制、推理与拥有者回调均在锁外。

## 对照结果

| 场景 | 旧 DLL | 候选 | 保护结果 |
| --- | --- | --- | --- |
| 实时输入归还停滞，健康离线通道上限8 KiB | 0/3接受 | 3/3接受 | 输入/规划同受离线字节上限 |
| 离线输入归还停滞，健康实时通道上限512字节 | 0/3接受 | 3/3接受 | 无自动重放或强制归还 |
| 一 Context 占用较大输入，另一 Context 实时上限512字节 | 0/3接受 | 3/3接受 | 共享执行器总额保持 |
| 同一实时类300字节上限，已有208字节仍未归还 | 明确拒绝第二项 | 明确拒绝第二项 | `queueFull`、无请求ID；释放后恢复 |

每次健康提交的接受/拒绝计数增量：旧 DLL 为 `[0,1]`，候选为 `[1,0]`；受控拥有者 retain/release 均 `[1,1]`。所有成功结果48字节逐位相同，输出 SHA-256=`0ecfd15b7ce32c6ba65cfe073467e6fac7d86bc1b2dba33d800d08587b54b493`。旧候选解除压力后也能恢复，表明比较的是错误隔离而不是永久死锁。

基线 DLL SHA-256=`6c4bd8b1adfdba3ff381a3f022a8c66db591ca42b62f5fa9e638a33e41f1c4ac`（上一已验证阶段构建）；最终候选=`aab058431dd4c485ea2d2f3d0953e79dc4189deed5ef8b0860a2538d7484d537`，对应原始 `after/results.json`、`batch-regression.json` 与 `identity.json`。模型 SHA-256=`d42637c177787ea869125e6543b3305442deb3aaadb9f6c6d8262c57bebb0b17`。相同显式CPU ORT路径，运行库摘要保存在身份记录。候选运行时notes诚实标注基线提交与dirty输入，不冒充提交后重新构建。

工具记录提交耗时仅供定位，含Python调用与离线回执工作；旧路径拒绝、新路径实际接受，不能据此算速度提升，也没有足量尾部样本支持P99或生产容量结论。

## 验证与复现

```powershell
python scripts/sdk/test-inference-budget-isolation.py --dll-dir .local/build/current/release --runtime-dir .local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/onnxruntime-cpu --sample-dir .local/tests/r1-baseline-inputs/tensors --output .local/evidence/r3-budget-isolation-20261027/after
cargo test -p am-vision-ffi --all-features --lib
cargo clippy -p am-vision-ffi --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

基线使用同一脚本加 `--baseline`，DLL目录为原始证据 `before/`。脚本只接受仓内 `.local` 输出，模型与运行库按本机私有材料提供。

- 11项Rust测试通过，含4项新预算测试：独立分类/Context、分类/合计/64 MiB总额拒绝、溢出与零部分扣账、元数据共占及收紧后不强收、32并发下Context字节或域16槽先到者限制、释放后归零。
- 真实CPU批量回归：动态/固定/未证明批次、部分回执、原生64项、取消后真实输入归还、离线等待与恢复、捕获超时不迟到准入、控制回执容量及关闭屏障通过；原始结果 `batch-regression.json`。不借此宣称五类业务模型重验。
- 受影响包严格clippy/fmt；生成、ABI冻结、来源边界及OpenSpec校验见同目录门禁记录。

## 审查、同步及未完成项

自审基线为上述源码提交，检查所有Admission构造/销毁、正常/失败/取消/批次退出、锁序和统计。新账本不持Context强引用，不引入循环；引用继续随真实工作排空。执行器锁→Context暂存锁同序，所有失败检查先于修改；无原生调用或拥有者回调在这两把锁内。没有未修的本阶段阻断项。新增模块承载一处准入，避免输入和规划各写一套算法。

同步ABI行为说明、OpenSpec与路线/状态/CHANGELOG、来源修订。README中英文、接入说明/示例、语言绑定、支持矩阵及第三方清单本阶段无需改：没有新接口、参数、依赖、语言或平台范围，也不替换公开制品。原提取清单和ABI冻结历史不改写；项目无 `.Codex/memory/patterns`，不伪称检查不存在的Pattern。

R3 1.1a局部修复通过；1.1全路径预算审计仍开放，尤其媒体全部工作线程观测、各类中转/控制分配及组合压力。20次故障、100次真实源关闭和4h/24h长跑未完成，不能由本轮短测或历史数据代替。其他R包与H2/H5未验范围继续按原任务推进，不把本记录当整包交付。
