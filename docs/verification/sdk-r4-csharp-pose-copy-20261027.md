# R4 C# Pose 结果复制验证（2026-10-27）

基线 `33d0d4ef0297a24f6af0e691e217abfd830debf6`。`InferenceResult.CopyPose()` 在结果锁内将检测框和关键点复制为托管对象，关闭后调用拒绝；保留原 `BorrowView()` 借用行为。按 Python 现有合同拒绝非 Pose、超过 300 框、超过 1024 关键点及非零数量对应空指针。新增布局由 Python ABI 真相源通过 C# 生成器生成，不改 C ABI。

验证：x64 C# 检查 Pose 检测项大小 32 字节及关键点指针偏移 24、关键点大小 12 字节及置信度偏移 8，与 Python ctypes ABI 实测一致；非空模拟检测框和关键点复制后修改原生值，副本不变；非法任务、数量和关键点空指针被拒绝；控制结果锁证明释放等待读取并在释放后拒绝调用。独立目录 Release DLL 配合 CPU ORT 和 640×640 合成黑图的真实 Pose 推理返回 0 项，复制成功；不代表真实非空人体检测精度。C# 编译警告当错误通过，C# 生成检查、ABI 生成检查和冻结检查通过（冻结 runtimeAcceptance 仍为 pending）。

素材 SHA-256：DLL `bc70f1d936bc93a4c5ad1b0225c4cdc4ed481da9f7dce20efe872330768784a8`；模型 `80fb5f58a3f6ec18909427ecc74fec0910196c2d2e924659eaae6dea5857b957`；ORT CPU `66e62f7bebf65b048ff1a93c40a73df93c36f93ce5bb085076de92b1a1e754fd`。未验证真实非空人体结果、其他结构化任务、复制性能或完整 R4。本记录为定向组件验证，不代表发布验收。
