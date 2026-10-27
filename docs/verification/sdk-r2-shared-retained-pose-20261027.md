# R2 共享帧池耗尽与推理归还定向验证

基线源码提交 `30af6ce2b89b17a8ef0b836db8e623a6d54e9588`；使用已有本机开发构建，Media DLL SHA-256 `0887f856fcc6b77bc2e2d54184aa1e7f9760b05306327ff326c71ba99f927fa5`，Vision DLL SHA-256 `49ebd1bfe28f9ee1c4e92a417b7f15f72d11a5d62fefcbda67c286106c6c53be`。构建身份沿用此前记录，不代表干净制品。本机 Windows x64、CPU ORT、合成 RGB、单槽共享池，两个模型实例使用同一 Pose 权重。

运行 `python -B scripts/sdk/test-inference.py --dll-dir .local/build/current/release --runtime-dir .local/releases/candidates/v0.1.0/windows-x64/runtime/onnxruntime-cpu --model-dir .local/tests/met2-s3 --shared-pose-only`。两个真实推理请求接受后归还初始借用，释放回调受屏障控制；屏障未打开时更改生产者输入并尝试再次发布，得到队列满，持有帧字节未变化。正常完成及关闭取消两轮各收到两次释放回调，归还后池的 outstanding 借用为零且可再次发布；正常轮的两个推理输出与同步基线一致。

此测试覆盖一槽、两个同权重模型实例及人为阻挡释放回调；不代表不同模型、跨进程崩溃恢复、所有资源压力和所有关闭竞争已覆盖，也不提供性能结论。R2 1.5 父项仍需综合其他资源路径证据，不因本次通过单独关闭。
