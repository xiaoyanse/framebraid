# R4 Pose 非空结果复制验证

基线为 main 的 `c90d452`；验证在 Windows x64、本机 CPU 和开发构建 DLL 上进行。使用 Ultralytics 安装包附带的 bus.jpg（SHA-256 `c02019c4979c191eb739ddd944445ef408dad5679acab6fd520ef9d434bfbc63`）和 YOLO26n Pose 模型（SHA-256 `80fb5f58a3f6ec18909427ecc74fec0910196c2d2e924659eaae6dea5857b957`）。DLL SHA-256 `49ebd1bfe28f9ee1c4e92a417b7f15f72d11a5d62fefcbda67c286106c6c53be`，并非正式发布产物。

Python 和 C# 分别运行真实图片推理，均得到 4 个 Pose 检测；两边分别将复制后的框、置信度和关键点与各自原生结果逐项比较，并验证应答、释放及垃圾回收后副本仍可读取。原有合成数据、无效输入和关闭竞争测试通过；Python 空白图 CPU 结果为 0。

这是复制一致性和生命周期验证，不是两语言输出的直接逐项对拍，也不验证独立推理参考、检测准确率或其他模型类型。R4 的真实模型及参考对拍要求仍待完成，不以本次结果关闭。
