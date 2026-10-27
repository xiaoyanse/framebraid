# R4 C# 结构化复制局部验证

基线：688bf762c26e75e5400979934954c379c5348aa1；本次新增 Detect、Classify、Segment、OBB 的 C# 深复制；保持 C ABI 和 BorrowView 不变。Segment 总掩膜预算 64 MiB，结构化数量和尺寸边界与 Python 一致。

Windows x64 .NET Framework 编译器 /warnaserror 编译所有 C# 绑定并运行 scripts/sdk/csharp-structured-check.cs：四类非空合成结构、复制后原生内存修改、关闭后复制值、布局、错误输入、掩膜累计预算及复制和关闭互斥通过。test-language-bindings.py 检查 44 种 C# 结构、297 字段和 Python 绑定及 10 项所有权测试通过，未请求真实模型运行。生成检查和 ABI 冻结检查通过；后者仍报 runtimeAcceptance pending。

测试是模拟原生合法内存和本机布局，不证明不受信任的裸指针有效性、真实非空模型结果或所有硬件；四类真实模型对拍仍待验证。

主线合入复核：主线全部 C# 绑定以 `/warnaserror` 重编译并运行同一合成结构/释放竞争测试通过；C# 类型生成器、ABI 生成/冻结和来源边界检查通过。接入说明合并冲突已保留主线 Rust 显式硬件探测迁移和四类复制说明。

隔离工作树已在确认无链接、无活动进程并保留测试摘要后回收；语言布局结果归档 `.local/evidence/r4-csharp-structured-main-20261027/language-results.json`，SHA-256 `e8ecccc57aff17e40bad98ffb9e3d0a99e29f1807917bad3825d95ddcc2be1ce`，生成测试源一并保留。
