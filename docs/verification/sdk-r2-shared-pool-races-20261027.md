# R2 共享帧池耗尽/关闭竞争/归还边界定向验证

源码基线为 `main` 提交 `8ccd5da` 加本次新增的 SDK 测试源码，不改任何运行时源码、生成物或公开接口；本验证只覆盖 `am-sdk-support` 共享帧池 Rust 层。Windows x64、合成 4×4 BGR8 帧数据、单进程双 Domain（Media 发布/Vision 消费）、随机回环无外部依赖；不连接相机、推理或现场设备。

新增三个测试（`crates/sdk-support/src/shared/tests.rs`，需 `shared-frames` feature）：

- 多槽与借用记录耗尽有界且可恢复：2 槽/2 记录/2 消费者配置下，占满全部借用记录后新借用返回 `RESOURCE_EXHAUSTED` 而非覆盖他人记录；归还后同帧可被其他消费者再借用。两槽均被活租约占用时发布返回 `QUEUE_FULL`，已发布帧字节保持不变（不覆写活缓冲）。全部归还后槽位复用，新发布正常、旧发布号返回 `RESULT_EXPIRED`。
- 消费者等待中关闭有界：空池 `borrow` 等待（截止 5000ms）期间关闭消费者，`request_close` 置停止标志后等待循环在毫秒级返回 `HANDLE_CLOSED`（本地实测 <100ms 打断，非等满截止），句柄达到 `CLOSED`，本地租约计数归零。
- 发布者等待中关闭有界：`DROP_WAIT` 策略下发布等待空位（截止 5000ms）期间关闭发布者，阻塞发布同样被停止标志打断返回 `HANDLE_CLOSED`。发布者关闭在消费者仍持租约时保持 `CLOSING`，归还租约并关闭消费者后才到 `CLOSED`，验证关闭顺序依赖而非强制接管活租约。

运行 `cargo test -p am-sdk-support --lib --features shared-frames -- shared`，13 通过（含既有共享池测试）；全工作区回归与 clippy 同批执行，结果见本次提交说明。该验证补齐 R2 1.5 所列池耗尽/活帧/取消/关闭竞争/归还后资源边界的 Rust 池层路径；真实推理持帧与取消归还由[前项记录](sdk-r2-shared-retained-pose-20261027.md)覆盖。两者合并不覆盖跨进程崩溃、进程死亡清理、全部资源压力矩阵或长跑，不提供性能结论；R2 1.6 整包量化收口另行记录。
