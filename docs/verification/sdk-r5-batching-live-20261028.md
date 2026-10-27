# R5 攒批真实 ORT 实测与两项死锁修复（2026-10-28）

基线 `main` 合并提交 `df8b850` 之后的工作区修复（本文所属提交从 Git 历史查）。DLL 为本仓 Release 构建（am_vision，`--features shared-frames`），ORT CPU 运行库 `.local/releases/candidates/v0.1.0/windows-x64/runtime/onnxruntime-cpu`，模型 `.local/tests/r5-batch/dynamic.onnx`（动态批量轴，允许跨样本计算）与 `fixed4.onnx`（固定 batch=4）；合成张量输入，无现场设备。脚本 `scripts/sdk/test-batching-live.py`，结果 `.local/evidence/r5-batching-live-20261027/`。

## 实测矩阵（全部通过）

| 用例 | 结果 |
| --- | --- |
| 固定轴模型 configure | 返回 `vision.batchUnsupported`；get 返回 flags=0 |
| 默认关闭 | AUTO 维持逐次执行，get 全零计数 |
| 启用 + 8 项凑满 | 一次提交立即成批（formed=1/batched=8），逐项输出 shape [1,3] 拆分正确、帧关联 1000-1007、ctx 干净关闭 |
| 5 项不满批截止 | 窗口 250ms 截止成批（timeout_batches=1），5 项交付 |
| 3 项 + 取消其一 | 取消项 CANCELED 终态收口；剩余 2 项窗口截止成批执行 DONE |
| 运行期关闭 | configure(NULL) 后新提交逐项交付、formed 计数不再增长，查询返回默认选项 |

对拍策略说明：`dynamic.onnx` 允许跨样本计算（批组成不同输出不同），逐项数值断言改为形状/帧关联/有限性 + 成批统计断言，不与逐项执行比固定值。

## 实测发现并修复的两项真实缺陷

合成单测（16 项）未覆盖以下真实调度交互，实测捕获：

1. **prune 收口的同线程锁重入死锁**：worker 在 tick 内 `records.try_lock` 成功后在 `Ok(records)` 作用域内调用 `finish_pending → execution::finish → records.lock()`——对不可重入 Mutex 同线程重入，worker 永久死锁，此后一切 records 操作（query/cancel）阻塞。首修（drop 后调用）不彻底。
2. **域锁内收口的跨锁成环**：`finish` 内部经 `state.context() → queues.lock()` 取 TTL，而 worker 此刻持有 `engine.data` 域锁——与等待域锁的队列持有者成环，实测表现为 cancel 后剩余窗口项永停 QUEUED（诊断日志实证 worker 在 finish_pending 内停摆）。

**修复**（结构性，非补丁）：`tick` 改返回 `TickOutcome{jobs, pruned}`——窗口触发照旧在域锁内入队；**剪除项收口整体移到域锁外**（worker 循环 `finish_pruned`），与正常执行完成路径的锁序完全同构。新增回归单测 `prune_keeps_remaining_items_flushable_by_deadline_tick`（部分取消后剩余项仍须被截止成批）。

修复后全套实测矩阵通过；`am-vision-ffi` 37 项单测、workspace 回归见下。

## 未验边界

吞吐/逐帧 P95 对照与新旧调度量化（R5 3.1）另行执行；多模型多窗口并发、真实模型（非合成张量）攒批、GPU 后端攒批未验。本记录不构成性能结论。
