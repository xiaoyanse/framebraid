# R4 语言通知缓冲与拥有者验证（2026-10-27）

范围：R4 1.1/1.2 的独立小包；不将 R4 父任务或五类模型验证标为完成。源码基线 `75cb7b19edf765306f847787295e8b36133948df`。主代理负责公共文档、来源清单和 OpenSpec 集成同步。

## 目标卡与现状缺陷

MET-2 / M-LANG、M-RECOVER；主目标是通知未消费时语言层积压有固定容量，不宣称推理或 CPU 提速。保护指标为最新通知顺序、并发计数对账、异常不越过回调、注销前固定拥有者，以及实际 DLL 调用仍可用。

原 Python `queue.SimpleQueue` 和 C# `ConcurrentQueue` 都无容量上限，原生队列有界不能限制已转移到语言封装的积压。相同 10,000 条输入的重建 Python 基线保留 10,000 条；此重建只验证容器机制，不是历史 DLL 性能对照。

修复：每个模型的 Python/C# 通知缓冲最多 **256 条**，满时丢弃最旧提示，保留新提示。Python 保留常用 `get/get_nowait/empty/qsize` 消费面；新增 `notifications_dropped`；C# 保留 `TryTakeNotification` 和 `NotificationsDropped`，新增容量常量。丢弃数只统计语言缓冲的淘汰或入队异常，不重复计入原生队列丢弃。队列不承载图片、张量或结果引用；请求查询、fetch/ack 和唯一终态不变。使用者须保留提交返回的请求 ID，不可把通知当完整事件账本。

生产者不等待缓冲腾出空间，不执行业务回调；只有固定长度队列的短临界区。注销仍使用原生注销屏障，屏障完成前保留委托和拥有者，不主动卸载 DLL。已排入语言缓冲的提示不在注销时静默删除。

## 定向实测

- `python scripts/sdk/test-safe-bindings.py`：**10 项通过**。保留原 6 项所有权验证，新增 10,000 条积压、4 个生产者共 4,000 条并发消费对账、GC 固定及注销释放根、注入入队异常隔离。
- 10,000 条无消费：候选两语言均保留最新 **256 条**，淘汰 **9,744 条**；FIFO 顺序核对通过。Python 并发测试与 C# 并发测试核对消费数 + 丢弃数 = 4,000，无重复交付。
- `test-notification-buffers.py`：真实 DLL CPU Identity 推理及原生通知接线通过；C# 注销/重新注册/GC 和缓冲验证通过。10,000/4,000 条压力通过显式调用已注册的语言回调完成，**不是声称原生推理持续负载 10,000 请求通过**。
- `test-language-bindings.py --run`：Python/C# CPU 同步异步、fetch/ack、共享帧租约通过；C# 路径/内存有序选择、strict 拒绝、诊断通过；37 个 C# 结构 / 259 个字段与 Python 布局一致；C# `/warnaserror` 编译通过。
- `git diff --check` 通过；未修改生成类型、C ABI、核心 crate，也未运行 Cargo 或重新构建 DLL。

私有证据位于本工作树 `.local/tests/notifications-20261027/notification-results.json` 与 `.local/tests/language-bindings-20261027/language-results.json`；主代理集成前归档到主仓 `.local/evidence`。所有测试只在独立本地进程，未启动相机/RTSP/业务服务，未接现场设备。

## 实测身份

使用主仓 `.local/build/current/release` 的只读 DLL，不把本次语言封装修复冒充新的 DLL 构建：

| 输入 | SHA-256 |
| --- | --- |
| am_camera.dll | `881134f3a5e4d72189f52ae0aa0bf13c2e92eb7827dc1a400e9a09952433ddbf` |
| am_vision.dll | `211e7a295a4edbe44a13d45d694447b71eb1014c4b45b02c65acdc2acb0d844c` |
| image.onnx（合成 Identity） | `d42637c177787ea869125e6543b3305442deb3aaadb9f6c6d8262c57bebb0b17` |
| onnxruntime.dll（CPU） | `66e62f7bebf65b048ff1a93c40a73df93c36f93ce5bb085076de92b1a1e754fd` |

## 审查与未验范围

定向代码审查覆盖锁顺序、异常、FIFO、并发计数、回调固定与注销屏障：本包无阻断项。语言通知锁不获取模型 Gate，因此模型注销等待原生回调时不会反向等待模型锁。`.Codex/memory/patterns` 在本工作树不存在，没有虚构 Pattern 检查通过。

这是缓冲有界机制和语言接线通过，不是 RSS、持续运行或吞吐收益结论。裸 `BorrowPixel`/`pixel_view`/结果结构视图仍要求显式拥有者存活，完整拥有者封装和 read-only 视图、对象级统计、缺依赖全矩阵、五类真实模型、干净包和 4h/24h 测试仍待后包。源码兼容面为新增语言常量/统计，以及通知超过 256 条时不再无限保留；C ABI/布局/SDK DLL 完全不变，公共说明由主代理同包同步。
