# R3 Context服务登记与关闭基础存储

2026-10-27；源码评审基线`26058585542cd4ca553e2e241d757feaeec035cf`。R3 1.1p/BP10、MET-2 M-RECOVER/M-OBS；SDK main串行、本地未发布，未改AM-RT、不连接现场设备。

## 改动与边界

旧Context服务索引为HashMap，关闭钩子为Vec，通知关闭时再collect强引用Vec。服务类型由SDK源码决定，但容器和临时快照没有明确固定存储说明。

现在服务索引固定16个内联槽，关闭钩子固定8个弱引用槽；本版Media最多6/1种，Vision为3/0种，不是让调用方登记任意业务插件。满额先拒绝，服务构造失败不发布；ISAPI服务先登记关闭责任，再发布服务。同类型保留同一Arc，重复钩子不增槽，失效弱引用可复用。关闭在栈上保留最多8个强引用，释放登记锁后通知。

固定基础开销与原控制通道载荷预算分列：Context创建即拥有内联槽，直到Context销毁回收，每库最多8个Context仍由原域限制保证。未改变64 KiB控制载荷预算或提高公开资源上限。服务体、句柄总表、Buffer注册、allocator及原生库内存不能由索引大小推导全部计费，BP10父任务继续。

C ABI结构、函数、能力位、绑定及冻结身份不变。Rust支撑层Context.services具体类型由HashMap改为Services，CloseHooks改用register；直接操作这些内部字段的源码调用方须按同提交调整并重新编译。不承诺内部结构的Rust二进制布局稳定；仓内所有使用点已接线。

## 量化与代价

`crates/sdk-support/tests/services.rs`采用线程局部分配计数，预建拥有者后对同样5种TypeId登记/关闭计数。旧容器按评审基线的HashMap/Vec/collect形式重建，**不是旧DLL端到端基准**。Windows x64 Release/all-features：

| 操作 | 旧容器 | 新实现 |
| --- | --- | --- |
| 5服务索引登记分配 | 2次，共请求428字节，最终容量7 | 0次，内联512字节 |
| 1钩子列表登记分配 | 1次 | 0次 |
| 真实Context关闭强引用快照分配 | 1次 | 0次 |
| 基础本体 | HashMap48字节，桶/钩子另分配 | Context1112字节（含本平台共享帧字段），钩子136字节另有Arc拥有者 |

新索引空闲也占512字节，比旧HashMap本体多464字节；8个Context的索引本体合计4096字节。钩子和服务拥有者仍按需分配，不称全链零分配、RSS下降、CPU卸载或吞吐提升。固定空槽是代价，收益是可计算容量和关闭时不临时申请列表。

## 验证

- 新增3单测：16槽满后先拒绝/同类型复用；构造失败不发布且可重试；8钩子满额、去重、弱引用回收及通知时锁可重入。Release分配对照额外验证真实Context关闭0次堆分配及5项内容一致。
- 全工作区all-features：288 passed、19 ignored，无失败；支撑层无默认特性15单测及3集成测试通过。忽略项未跑，不冒充完整硬件验收。
- 两DLL Release/shared-frames构建及受影响三包严格clippy通过。真实Digest、CPU RGBA、外持图片关闭屏障、慢回调/错峰共享、72/80字节临界、缓存失败重试和20个留存Closed句柄通过。
- 三轮真实H.264回环每轮5条媒体线程，关闭后归零；4槽时压缩继续、像素等待，回调排空。自有MediaMTX/FFmpeg已退出，认证配置自动回收。
- 实际CPU张量推理低预算、拒绝不替换/不消耗ID、输入拥有者归还与关闭通过。有序选择、缺EP strict/CPU回退、100次诊断、Python包装、设备探测和CPU loaded/executed状态分离通过；不是加速EP正向验收。

旧选择测试曾在硬件报告查询返回resourceExhausted：两份8 KiB选择缓存+16 KiB探测缓存+32 KiB查询预留已占满64 KiB，另有上一阶段纳入计费的请求表。原失败日志保留；测试现断言重叠拒绝、释放模型后同一探测结果查询成功，同时保留100次模型查询与strict/回退断言。未放大SDK预算或改产品代码绕过拒绝。

## 身份、审查与后续

Windows x64 Release/shared-frames候选为评审基线加本包dirty源码：Media SHA-256 `10930be9294ab3b94540680a436728d6a39a69a87b585eb1a3737c4276ad83a8`；Vision `e96ae962e15fef98bd9bb698c200cf54a26657c6cfe2ce4f7b9eb3de092685f8`。FFmpeg/ORT CPU使用本仓隔离候选，正式制品未替换。

主证据`.local/evidence/r3-context-services-20261027/`保留编译、原失败、最终回归及allocation-release.txt；同前缀workers/capture/selection/hardware目录存结果，回环退出记录在`.local/tests/r3-context-services-rtsp-20261027/owned-processes.json`。本包未创建旧DLL/模型大副本，无新增待删除大型测试目录。

按支撑容器、两库接线、测试和文档分组审查：构造失败、弱引用/锁序、固定上限、关闭持有、C ABI不变和Rust源码迁移均核对，无阻断项。README与语言绑定无需变更：不新增调用步骤、导出、能力位或支持承诺；接入说明补基础开销边界，OpenSpec和来源清单同步，原提取指纹不覆盖。

最终来源检查222文件、零业务依赖/零外部本地路径；C头132函数和冻结11项检查通过，runtimeAcceptance仍pending。R3 OpenSpec严格校验、fmt及diff检查通过；受保护.gitignore和原始提取清单SHA未变。自有回环进程按PID另查已不存在。

BP10域句柄/Buffer、服务体及其他索引，BP12直接Rust会话/原生依赖，完整硬件矩阵及4h/24h继续开放。本包只本地提交，不推送、打标签或发布。
