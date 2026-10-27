# R4 C# 结构化复制局部验证

基线：688bf762c26e75e5400979934954c379c5348aa1；本次新增 Detect、Classify、Segment、OBB 的 C# 深复制；保持 C ABI 和 BorrowView 不变。Segment 总掩膜预算 64 MiB，结构化数量和尺寸边界与 Python 一致。

Windows x64 .NET Framework 编译器 /warnaserror 编译所有 C# 绑定并运行 scripts/sdk/csharp-structured-check.cs：四类非空合成结构、复制后原生内存修改、关闭后复制值、布局、错误输入、掩膜累计预算及复制和关闭互斥通过。test-language-bindings.py 检查 44 种 C# 结构、297 字段和 Python 绑定及 10 项所有权测试通过，未请求真实模型运行。生成检查和 ABI 冻结检查通过；后者仍报 runtimeAcceptance pending。

测试是模拟原生合法内存和本机布局，不证明不受信任的裸指针有效性、真实非空模型结果或所有硬件；四类真实模型对拍仍待验证。
