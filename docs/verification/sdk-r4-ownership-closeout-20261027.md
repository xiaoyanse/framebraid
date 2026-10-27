# R4 1.1/1.2/1.3 收口核对：所有权审计、借用说明与模型档案（2026-10-27）

本记录整合既有实现与证据并补充本轮示例所有权审计；核对基于 `main` 提交 `9ee4e6d` 时点，不新增运行验收（引用记录均已提交）。

## 1.1 常用流程与所有权审计 + 真实 DLL 示例四场景

**审计范围**：`sdk/examples/` 全部 10 个示例（C 2、C++ 2、Python 2、C# 3、共享帧 1）。所有权模式核对：

| 语言 | 模式 | 审计结论 |
| --- | --- | --- |
| C（inference/lifecycle/shared_frame.c） | 显式 ack/release、cancel 在途请求、close 状态非 CLOSED 提示"不得 destroy/unload"、destroy 仅在 CLOSED 后 | 配对完整，无泄漏路径示范 |
| C++（inference.cpp） | `unique_ptr` + `ObjectDeleter<close,destroy>` 模板、Borrow 析构归还共享帧、ResultDeleter、close 未完保留不强毁 | RAII 边界正确 |
| Python（inference/camera.py） | `with` 全套（context/subscription/frame/result/pool/borrow）、fetch/ack 配对、借用作用域化 | 无跨期持有 |
| C#（Inference/Lifecycle/Selection.cs） | `using`/SafeHandle、SharedBorrow using 内 SubmitPixel | 同上 |

**四场景证据映射**（真实 DLL，既有记录）：

- 成功：C/C++ 真实调用（[ABI 1.1](sdk-r2-abi11-hardware-20261026.md) GCC C11/-Werror 与 C++17）；Python/C# 高层门面实调（[owned-pixels](sdk-r4-owned-pixels-20261027.md)、[H5.0b](sdk-h5-inference-report-20261026.md)）。
- 缺依赖：装载诊断分层——缺文件/缺导出/运行库缺失（`test-sdk-load-diagnostics.py`，含 C# `DllNotFoundException` 分支）；VC++ Redist 官方指引在 `sdk/INTEGRATION.md` §依赖。
- 背压：有界通知 256 条+丢旧（[1.0a](r4-language-ownership-20261027.md)，10000 条/9744 丢弃与 4 生产者对账）；有界队列与预算拒绝沿用 R3 压力矩阵；无新增无界队列（审计确认示例与门面无排队放大）。
- 关闭：lifecycle 示例模式 + 门面关闭竞争（owned-pixels Dispose 竞争、共享池 [1.5b](sdk-r2-shared-pool-races-20261027.md) 等待中关闭毫秒级打断）。

**高层帮助入口**：绑定错误携带 code/message/source（复制后再释放快照），装载失败给"deploy matching SDK and bindings"类非臆测指引（loader hint 测试在案）。**结论：1.1 收口。**

## 1.2 借用视图与复制接口说明及验证

说明落点 `sdk/INTEGRATION.md`：安全像素门面（owner→model 锁 + GC 根、异步默认复制、`copy_planes` 额外复制、`RgbBuffer.Copy` 隔离原数组）；C# `CopyPose/CopyDetect/CopyClassify/CopySegment/CopyObb` 与 Python `copy_pose` 等结构化复制语义（结果存活期内才可复制、掩膜 64MiB 上限、`BorrowView` 保持借用）；共享租约 attach/borrow/return 与无 ORT 例外。

验证要件映射：GC 根（weakref 存活、owned-pixels 双语）；提前归还/关闭后拒绝（`ObjectDisposedException`/RuntimeError、二次 return `handleInvalid`）；异常退出（注入 `infer_pixel` 抛错不隐藏、结果不悬空）；多模型异步不悬空（`asyncDefaultCopySurvivesReturnAndGc`：异步默认复制在归还+GC 后完成交付）；二次释放防护（`amv_shared_pool_return` 幂等拒绝）。**结论：1.2 收口。**

## 1.3 模型档案：版本/摘要/校验/兼容错误 + 五类绑定状态

档案现状（`crates/vision/resources/profiles.json`，5 个档案五类各一）：id 携带 `-v1` 版本后缀；`sample_sha256` 摘要；`input`/`outputs` TensorSpec 与 `classes`；加载时（`sdk/model.rs`）按档案比对实际输入输出（名称/形状），失败返回 `vision.profileUnsupported`/`vision.inputNameUnknown` 等具体错误键，借元数据校验不做每运行克隆（validation_tests 在案）。

五类绑定与真实推理对拍状态：

| 类别 | 档案 | 权重 | 状态 |
| --- | --- | --- | --- |
| Pose | pose-yolo26-e2e-coco17-v1 | yolo26n-pose.onnx（本机） | **已验**：真实人物图 4 项有效检测与 Python 独立 ORT 对拍、原始张量对拍（[记录](sdk-r4-pose-real-image-20261027.md)及 18a0820/be95ad4/30af6ce 提交链） |
| Detect / Classify / Segment / Obb | 各自档案已定义 | **本机无合法权重** | 档案与解析路径在案（结构化复制 1.0d/1.0f 合成验证）；**真实权重推理对拍未验**，按任务原文缺权重明列，不以 Pose 或合成图冒充 |

**结论：1.3 按可得范围收口**——档案版本/摘要/输入输出校验/兼容错误在案，Pose 完成绑定与真实对拍；四类缺权重如实未验，取得权重后须按同一对拍方法补验，不重复冒充。

## 边界

本收口不新增五类真实模型验收，不改变档案合同；1.4 干净解压包语言示例验证保持开放（下一批）。对象级统计合同缺口（§2.11）另按任务书在 1.1/1.4 范围内核对，未补前不宣称对象统计支持。
