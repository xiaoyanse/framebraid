# Tasks

> 用户已授权恢复SDK main串行开发，先补R1量化前置再继续本包；不改AM-RT、不发布。测量/验收采用[MET-2](../../../docs/PERFORMANCE_ACCEPTANCE.md)，指标组M-LANG/M-TENSOR；语言复制/借用成本透明、拥有者及诊断可用、档案容差正确；不要求不同语言同速，不用微型图代五类代表模型。 旧勾选保留其原证据范围，历史5%警戒不等于已证回归或自动通过。

## 1. 优先实施与定向验收

- [x] 1.0b 增加两语言Frame/共享Borrow/RGB安全像素操作门面，owner→model锁、显式关闭拒绝、GC/异常/并发关闭、两模型sync/默认复制async及copy语义真实验证，见[记录](../../../docs/verification/sdk-r4-owned-pixels-20261027.md)。不关闭结果raw view/真实相机/五类模型等父项。

- [x] 1.0a 修复Python/C#无界通知积压：256条、丢旧及语言计数，10000条/9744丢弃及4生产者4000对账、10项拥有者和真实CPU通知/共享租约、37结构259字段通过，见[记录](../../../docs/verification/r4-language-ownership-20261027.md)。未关闭以下父任务和裸视图/五类模型/干净包。

- [x] 1.0c Python Pose 结构化结果持锁深拷贝与释放/竞争验证，见 `docs/verification/sdk-r4-python-pose-copy-20261027.md`；其他类型、C#和1.1～1.4继续。

- [x] 1.0d Python 检测、分类、分割、旋转框结构化结果有界深复制与合成生命周期验证，见 `docs/verification/sdk-r4-python-structured-copy-20261027.md`；C#与真实模型对拍仍开放。

- [x] 1.0e C# Pose 结果持锁深复制和限定生命周期验证，见 `docs/verification/sdk-r4-csharp-pose-copy-20261027.md`；其他 C# 结构及真实模型对拍仍开放。

- [x] 1.0f C# 检测、分类、分割、旋转框安全深复制及合成生命周期验证，见 `docs/verification/sdk-r4-csharp-structured-20261027.md`；真实模型对拍与完整 R4 保持开放。

- [x] 1.1 审计 C/C++、C#、Python 常用采集/推理流程和所有权，补足高层帮助入口；真实 DLL 示例验证成功、缺依赖、背压与关闭，不增加无界队列。
  - 收口核对：10 个示例所有权模式逐项审计（C 显式配对/C++ RAII/Python with/C# using）；成功/缺依赖/背压/关闭四场景映射既有真实 DLL 证据；装载诊断与帮助指引在案。见[记录](../../../docs/verification/sdk-r4-ownership-closeout-20261027.md)。
- [x] 1.2 提供有拥有者/只读/有效期的借用视图与复制接口说明，验证 GC/提前归还/异常退出、多模型异步不会悬空或二次释放。
  - 收口核对：INTEGRATION.md 借用/复制说明完备（门面锁/GC 根、异步默认复制、Copy 家族语义、64MiB 上限）；GC/提前归还/异常/多模型异步/二次释放全部映射既有验证。见[记录](../../../docs/verification/sdk-r4-ownership-closeout-20261027.md)。
- [x] 1.3 增强模型档案版本、摘要、输入输出校验及兼容错误；五类合法代表模型逐类绑定导出身份并真实推理对拍，缺权重明列未验。
  - 收口核对：五类档案（版本后缀/样本摘要/输入输出规格/加载校验/兼容错误键）在案；Pose 绑定真实权重完成人物图对拍（独立 ORT 对拍+原始张量一致）；Detect/Classify/Segment/Obb 本机无合法权重，按原文缺权重明列未验，不冒充。见[记录](../../../docs/verification/sdk-r4-ownership-closeout-20261027.md)。
- [x] 1.4 从干净解压的匹配本地 SDK 包运行语言示例，头/绑定/能力/文档一致，中英文指南及接入/支持说明同步后提交。
  - 收口核对：干净检出构建→锁定指纹打包→独立目录解压，Python/C#/C/C++17 四类示例全部真实编译运行通过（含修复 inference.cpp NOMINMAX 保护与 lifecycle.c 能力位掩码同步 16 位全集；编译环境问题查明为会话 PATH 解析顺序、MSYS2 工具链完好），camera.py RTSP 本地链路未过如实登记待查；打包暴露的修订登记/许可清单漂移已修正提交。见[记录](../../../docs/verification/sdk-r4-clean-bundle-20261027.md)。

## Workflow follow-up

- R3审计登记的旧接口差异：ABI§2.11列出对象级统计scope，当前两库只接受NULL/`context`。在1.1/1.4核对所需对象身份、接口接线、语言例子与正式合同；未补前不宣称对象统计支持，不擅自缩减合同。证据见[审计](../../../docs/verification/sdk-r3-budget-audit-20261027.md)。

- 本地提交不代表发布；不自动归档。未完成实现及范围内定向必验项保持开放，不用旧候选数据替代；全硬件矩阵/固定长跑不再是未完成验收项，未测范围仅如实说明。
