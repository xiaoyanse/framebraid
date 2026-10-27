# R4 Python Pose 结果安全复制（2026-10-27）

基线 `ddc677d`。Python `Result.copy_pose()` 在结果拥有者锁内深拷贝检测框和关键点，返回不可变 tuple：每个元素为 `(bounds, confidence, keypoints)`，关键点为 `(x, y, confidence)`；返回值不再依赖原生内存。非 Pose、超过300框、超过1024关键点以及相应非零数量配空指针明确拒绝；零项合法。低层 `Result.view` 保留，读取期间与关闭竞争仍由调用方承担，不可在结果释放后使用。此能力不改变C ABI、不提供新原生借用，也不代表其他任务类型或C#已完成。

`scripts/sdk/test-result-pose.py` 以非空合成结构验证深拷贝独立性、非法数量/空指针及非Pose拒绝、显式关闭后拒绝、GC后复制值存活；受控阻塞读取时验证 close 等待锁并在返回后释放。另用真实 CPU ONNX Pose 模型和640×640合成黑图完成推理，模型返回0项；验证空结果及释放，不表示非空画面检测精度或五类模型对拍。

Pose模型 SHA-256 `80fb5f58a3f6ec18909427ecc74fec0910196c2d2e924659eaae6dea5857b957`；DLL SHA-256 `f81acf867bd43a6d1d8c23c595bad814d95097f8d0b33828ce7fb4ad98885e3d`（上阶段工作区构建，非本次干净发布包）；ORT CPU SHA-256 `66e62f7bebf65b048ff1a93c40a73df93c36f93ce5bb085076de92b1a1e754fd`。语法检查通过；既有 Python/C# 像素拥有者及 CPU 双模型同步/异步回归通过（不代表 C# Pose 结果接口已验），实际运行无现场设备/业务数据。未测 C# Pose、四种其他结构结果、真实非空人体样本、跨语言复制成本及完整匹配包；R4 1.1～1.4继续开放，不将本项记为整体完成。
