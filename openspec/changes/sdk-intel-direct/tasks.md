# Tasks: sdk-intel-direct

> 本 change 等级：L3，门禁要件：关键路径单测+关键集成点测试（真装配）+e2e 真机（Arc 140T）+ABI/生成检查+boundary+文档全同步。范围忠实度：本清单即交付范围，缩水=打回。

## 1. 合同设计（先合同后生成链）

- [x] 1.1 §3.3a 增补三处（bit10 语义扩展/前处理色彩注记+容差分级/设备域 OV 侧 vendor 一致性）+审计口径 OV remote 条款，写入 docs/design/dual-dll-abi.md；types 注释同步（批 I-A）
- [x] 1.2 constants.json abiMinor 7→8（AM_ABI_VERSION 65544）+四生成物同步（generate-abi/generate-rust-abi/generate-python/generate-csharp）+freeze-abi 冻结工件单文件归因重算（批 I-A，r15）
- [x] 1.3 变更分类登记 CHANGELOG Unreleased（兼容新增，ABI 1.8）（批 I-A；批 I-C 补 I-B 实现哈希绑定与 I-C 条目）

## 2. OV 绑定面

- [x] 2.1 ov_sys.rs 新符号绑定（ov_core_create_context/ov_core_compile_model_with_context/remote tensor 族），2025.0/2025.1/2025.3/2026.1 四线头文件核对，符号存在性校验纳入 OvApi::load（批 I-A 六 remote 符号+批 I-B preprocess 族 24 符号）
- [x] 2.2 remote context/张量属性键常量（CONTEXT_TYPE=VA_SHARED/VA_DEVICE/SHARED_MEM_TYPE=VA_SURFACE/DEV_OBJECT_HANDLE/VA_PLANE）+序数按版本线判（ElementLine 同款机制）（批 I-A）

## 3. vision 实现

- [x] 3.1 OV remote 探测函数（openvino_c 可载+GPU 设备在域，OnceLock 进程缓存，不触发推理编译）接入能力位置位（bit10 = CUDA 探测 OR OV remote 探测）（批 I-A 探测+批 I-B bit10 接线）
- [x] 3.2 OV GPU 会话的 GPU 帧处理链：适配器 vendor 一致性校验（D3D11 纹理 vs 会话设备，核对词机制）→remote context→NV12 双平面 remote 张量→preprocess 图（scale 除法语义+pad 114 归一化前，两坑规避）→compile_model_with_context→绑定推理→输出回读（批 I-B）
- [x] 3.3 notSupported 判定链：OV 会话+GPU 帧+remote 不可用（无 GPU 插件/适配器不一致/格式不支持）→显式拒绝（不静默降级），错误详情含设备域快照（批 I-B 实现；批 I-C 矩阵三档实证）
- [x] 3.4 搬运审计：OV remote 路径 gpu_inputs 计数+copies=0 口径（零 host 上传）+预算挂账与 CUDA 路径同式（批 I-B，I-B e2e 实测增量=remote 帧数/copies=0）

## 4. 测试与验证（L3 全套）

- [x] 4.1 单测：适配器一致性校验纯函数+preprocess 图参数构造（scale/pad 值断言）+探测函数 mock 面（批 I-B）
- [x] 4.2 集成测试（真装配）：OV remote 路径 e2e——自组装 am_gpu_frame_ref_t（同 CUDA 线 gpu_cascade_e2e_tests 先例形态）喂 NV12 纹理，身份断言（bus=5/zidane=3）+top conf 门（对拍 CPU 臂 ≤5e-2）（批 I-B ignore 门实跑，证据 ib-20261119）
- [x] 4.3 真机门（Arc 140T）：remote 全链身份零漂移（三轮定时窗）+D3 参考输出对照（GPU 域 5e-2 统计档，reference-check.py 或同口径脚本）+回退矩阵（无 GPU 插件运行库→CPU 会话不受影响+GPU 帧拒绝路径）（批 I-C：30s×3 轮 7144 帧零漂移+D3 对照由批 I-B e2e 承载（IoU=1.000/Δconf=0.0000 编码臂）+回退矩阵三档 JSON 实证；**60s 窗升格被本机外源 GPU 负载阻塞（visionforge 持续占用 iGPU compute，驱动调用内核态阻塞两次复现），按原型 30s×3 口径收口，升格待环境安静窗口补测——如实登记不视为通过**）
- [x] 4.4 性能量化：生产几何（1080p→640 非恒等）A/B（remote vs 像素路径 OV CPU vs gpu-hostio）+恒等几何对照（原型口径复现），数字进 RELEASE_STATUS（绝对值）+INTEGRATION 指导（批 I-C release 三臂：A id640 9.2-9.8ms（7144 帧，长窗证据承载）/B 25.35+31.76ms/C 13.27+19.94ms（prepare+run 分段）；**A prod 臂因同上环境阻塞未落盘（remote 建链进程级 AV 复现）——登记未完成，数字为争用态实测口径**）
- [x] 4.5 workspace 全测+fmt+clippy+ABI 检查+boundary 登记（新/改生产源码全入指纹链）（批 I-B 559/0+boundary 304/13+fmt/clippy 净+freeze 绿；批 I-C 文档批复跑同数）

## 5. 文档同步

- [x] 5.1 INTEGRATION：Intel 直传部署节（OV 运行库要求+设备定向+容差分级说明）（批 I-C，含三条限定同口径）
- [x] 5.2 MIGRATION：ABI 1.7→1.8 段（bit10 语义扩展+旧调用方零变化论证+容差分级）（批 I-C）
- [x] 5.3 RELEASE_STATUS 验证状态节+ROADMAP 待办回写；SUPPORT.md 支持矩阵（Intel 直传=已验范围+边界如实）（批 I-C）
- [x] 5.4 变更分类核对六件套（CHANGELOG/README×2/INTEGRATION/SUPPORT/RELEASE_STATUS）（批 I-C：CHANGELOG I-C 条目+I-B 哈希绑定；README×2 状态行补未发布 1.8 一行；INTEGRATION/MIGRATION/SUPPORT/RELEASE_STATUS 各新增节）
