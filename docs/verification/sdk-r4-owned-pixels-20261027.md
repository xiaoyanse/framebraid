# R4 像素输入拥有者门面验证（2026-10-27）

## 范围与依据

代理工作树基线 `69624cfbc2c294996f84233f3f821b9a6c8f175e`，分支 `codex/r4-owned-pixels`。仅手写 Python/C# 封装、语言测试及本记录；不改 C ABI、生成类型、核心 crate、AM-RT、公共合同或 OpenSpec 勾选。主代理另行完成合同、指南、来源及集成验证。

目标采用 MET-2 的 M-LANG/S6：消除常见输入借用路径的提前归还风险，保持旧入口兼容；不是吞吐优化，不要求不同语言同速。原始裸视图可先取出后关闭拥有者，语言对象本身不能证明指针有效；测试不解引用已经释放的裸指针去冒险制造崩溃，而验证新入口在相同关闭顺序下明确拒绝。

## 改动与生命周期

- Python `Frame.pixels()`、`BorrowedFrame.pixels()`、新 `RgbBuffer.pixels()`，C# `CameraFrame.Pixels()`、`SharedBorrow.Pixels()`、`RgbBuffer.Pixels()` 返回 `OwnedPixels` 操作门面，不公开原生指针。支持同步推理、默认复制异步提交、共享帧发布和独立平面复制。
- 门面强持原拥有者。操作统一锁定拥有者，再进入模型或发布器；原拥有者的 close/Dispose 使用同一锁，必须等原生同步调用或异步提交返回。没有反向 model→owner 加锁路径。显式关闭后门面拒绝，门面不延长已经显式归还的租约。
- 异步默认复制输入；拥有者保护到提交返回即可，随后允许归还/关闭。每个请求的结果另行 fetch/ack/release，不把拥有者门面当异步完成句柄。未提供零拷贝异步选项，不宣称全链零拷贝。
- Python 新 `RgbBuffer` 复制构造输入，复制关联结构；调用方随后修改原 bytearray 不影响像素。C# 原构造仍 pin 用户数组，保持可更新数组的旧行为，调用方必须避免读取期间改写；新 `RgbBuffer.Copy` 才隔离源数组。门面的“只读”指没有写入/裸指针出口，并非能禁止低层代码或外部原数组写入。
- `CopyPlanes`/`copy_planes` 返回独立字节（C# 数组可改但不影响原帧），核维度、格式、平面数量、指针、步长、末行有效字节和长度范围。支持当前 RGBA8/RGB8/BGR8/NV12；包含原平面声明的 padding 字节。
- 旧 `pixel_view/BorrowPixel`、`infer_pixel/InferPixel/SubmitPixel` 保留；文内注释明确由调用者保护拥有者直到调用返回。Python ctypes 私有字段和 C# IntPtr 低层逃逸仍存在，不能宣称在恶意或不当低层访问下也内存安全。
- C# `RgbBuffer.Dispose` 幂等并与读取同锁；补终结器回收遗漏的 pin/非托管描述符。Python 原生资源仍要求 with/close，不依赖 GC 自动关闭 DLL 资源。

## 测试与证据

代理证据（忽略目录）：`.local/evidence/owned-pixels-final/results.json`；旧语言回归 `.local/tests/language-regression/language-results.json`。均在此隔离工作树下，无 Cargo 编译、无相机/业务服务或现场设备操作。

```powershell
python scripts/sdk/test-owned-pixels.py --output <本树.local/evidence/owned-pixels-final> --dll-dir <SDK主仓.local/build/current/release> --runtime-dir <现有onnxruntime-cpu目录> --model <现有image.onnx>
python scripts/sdk/test-language-bindings.py --output <本树.local/tests/language-regression> --run --dll-dir <同目录> --runtime-dir <同目录> --model <同模型>
node scripts/sdk/check-boundary.mjs
git diff --check
```

| 检查 | 代理结果 |
| --- | --- |
| Python 拷贝输入隔离、GC 根、读取期间并发 close 等待、提前关闭拒绝、异常解锁、Frame 门面失效、平面越界拒绝 | 通过 |
| C# 原构造引用数组保持、新复制工厂隔离、重复 Dispose、并发 Dispose 等待、GC 根、异常解锁、关闭拒绝 | 通过 |
| Python/C# CPU 原生推理 | 每语言 2 个模型各同步调用；共享借用提交给 2 个模型异步执行，归还并 GC 后结果保持 17，Identity 输出 `[1,3,2,2]` |
| 共享消费者关闭 | 活借用时关闭拒绝；归还后消费者 detach，再 publisher close，最终 Context 关闭正常 |
| 原有语言回归 | Python 10 项拥有者测试；C# 37 结构/259 字段；旧 Python/C# CPU 推理、共享帧、C# 选择示例均通过 |
| SDK 来源隔离检查 | 224 个核心源文件、0 业务依赖、0 外部本地 path；本包不改变核心源 |

实际加载既有 Release DLL（本包不编译 DLL）：

| 材料 | SHA-256 |
| --- | --- |
| `am_camera.dll` | `881134f3a5e4d72189f52ae0aa0bf13c2e92eb7827dc1a400e9a09952433ddbf` |
| `am_vision.dll` | `211e7a295a4edbe44a13d45d694447b71eb1014c4b45b02c65acdc2acb0d844c` |
| CPU `onnxruntime.dll` | `66e62f7bebf65b048ff1a93c40a73df93c36f93ce5bb085076de92b1a1e754fd` |
| `image.onnx` | `d42637c177787ea869125e6543b3305442deb3aaadb9f6c6d8262c57bebb0b17` |

## 复制成本与结论边界

12 字节 RGB8 样例：安全门面本身不增加语言像素复制；Python 新 RGB 拥有者或 C# Copy 工厂显式复制 12 字节，C# 原构造仍为 pin、无新增像素复制。每次 CopyPlanes 另外复制 12 字节；异步 SDK 默认复制及前处理/ORT 转换仍保留。这是实现路径和内容验证，不是分配采样或时延收益实测，未报告速度改善。

自审：无新增队列/全局登记、无 DLL 卸载或 ABI 漂移；返回异常后锁正常释放；双模型提交各自原生复制，源关闭不影响已接受请求。Python safe_sdk 现约 660 行，后续继续增量时宜按拥有者职责拆模块，本包未为压行改写现有代码。

本结论是**代理工作包验证**，待主代理 merge 后重跑相应门并补公开文档才能作为主线阶段成果。未测真实 RTSP CameraFrame、五类 YOLO 精度、GPU、干净交付包、结果 raw view 的拥有者门面与长跑；不关闭 R4 1.1/1.2 父任务或整个 R4。
