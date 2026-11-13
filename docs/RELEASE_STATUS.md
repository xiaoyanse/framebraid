# Release status / 交付状态

当前公开版本：**v0.7.1**（2026-11-13，C ABI **1.5**（兼容新增 65541）：**opt-in GPU NV12 级联输入**——源帧一次搬运、ROI 批 4 显式可选、会话级 CUDA FP32 对拍选项（ABI 1.5），默认逐 ROI；**Python/C# 推理池封装**（owned 像素提交+分片清理+旧版绑定兼容验证）；质量线（am-camera 测试 clippy 清零+SDK 检查自动化）。研究线提交 `65b450c`..`ed51905`（独立研究线产出，PM 独立复验后定版）。历史版本 [v0.7.0](https://github.com/xiaoyanse/framebraid/releases/tag/v0.7.0)（推理池编排 API，ABI 1.4）、[v0.6.5](https://github.com/xiaoyanse/framebraid/releases/tag/v0.6.5)、[v0.6.4](https://github.com/xiaoyanse/framebraid/releases/tag/v0.6.4)、[v0.6.3](https://github.com/xiaoyanse/framebraid/releases/tag/v0.6.3)、[v0.6.2](https://github.com/xiaoyanse/framebraid/releases/tag/v0.6.2)、[v0.6.1](https://github.com/xiaoyanse/framebraid/releases/tag/v0.6.1)、[v0.6.0](https://github.com/xiaoyanse/framebraid/releases/tag/v0.6.0)、[v0.4.0](https://github.com/xiaoyanse/framebraid/releases/tag/v0.4.0)、[v0.3.2](https://github.com/xiaoyanse/framebraid/releases/tag/v0.3.2)、[v0.3.1](https://github.com/xiaoyanse/framebraid/releases/tag/v0.3.1)、[v0.1.0](https://github.com/xiaoyanse/framebraid/releases/tag/v0.1.0)。
当前本地源码与发布版一致时以 Git 提交核对；领先时以本页登记。发布流程见 [发布清单](RELEASE_CHECKLIST.md)，逐项验证证据见 [verification 目录](verification/)（按主题命名，含过程记录）。

## v0.7.1 验证状态快照

研究线（GPU ROI 级联）验证（`b230ecd`..`ed51905`，证据 `.local/evidence/roi-production/`）：四张合成测试图案回放、每张 8 ROI 对拍，CUDA CPU 像素前处理和 GPU 纹理输入的最大框差为 0.123 像素，CUDA 逐个和批 4 最大框差为 0.947 像素。CPU 与 CUDA 后端的低置信度首选框存在 166.05 像素差异，在相同 CPU 像素输入下复现（阈值边缘首选框切换，与 GPU ROI 坐标回映无关）。隔离进程禁用 TF32 后，四张画面 CPU/CUDA 最大框差为 0.00142 像素，CUDA 逐个与批 4 最大框差为 0.00277 像素。ABI 1.5 严格 CUDA FP32 会话选项复测：同一批四帧 CPU/CUDA 最大框差 0.00130 像素，CUDA 逐个与批 4 最大框差 0.00142 像素。20,275,200 字节准入边界、拒绝、CPU 像素显式回退、取消及双模型在途排空已在本机验证。随后在 bus 与 zidane 两张公开实景照片上，以真实 YOLO11n 权重、0.25 置信度阈值验证——两张照片各 8 个固定 ROI 的首框一致，CPU/CUDA 最大差 0.000077 像素，逐个/批 4 最大差 0.000115 像素，动态 ROI 回映最大差 0.00008 像素。三路共用会话的 15 请求均完成，热态账本峰值 60,825,600 字节，排空归零，设备显存采样增量 2 MiB 后回到空闲读数。批 4 在照片实测中较慢，保持显式可选。冷启动峰值、多独立会话和现场负载未测。

语言封装验证（`0ad06fa`）：Python 四路结果逐项对拍，C# 四路异步请求提交后释放 RGB 拥有者，随后等待、取回并确认结果；两种封装均在同一 Context 连续创建、关闭三个池；旧 v0.6.5 Python 绑定以 ABI 1.3 加载 ABI 1.4 双库，完成初始化、单帧结果对拍、确认和关闭销毁（前向兼容实证）。

**PM 独立复验（定版前）**：GPU 级联门 `gpu_cascade_public_entry_real_replay` 以 FP32 严格对拍（FB_ROI_CUDA_FP32=1+FB_ROI_REQUIRE_CPU_PARITY=1）复跑绿——回放臂 33.53s+照片臂（真实 YOLO11n）14.65s；workspace **18 套件 528/0 零失败**；四检绿（bindings 0/doc-coverage 197 符号 0 漏/zero-semantics 0/freeze 11 工件过=ABI 1.5 基线已重算）；boundary 296 源文件 0 违规；**vision 侧真机门 16 门同进程串行全绿 39.03s**（15 门+gpu_cascade 新门入列）。发布门环境债如实登记：am-camera-ffi 19 个 ignore 门 12 绿 8 阻塞（回环 RTSP 服务器搭建+样本变量映射的历史 harness 配置散佚；8 门全在本版改动半径外，所涉文件的 workspace 单测全绿）——登记 v0.7.x 待办不阻塞本版。构建注意：本机 Release 并行 codegen 出现 rustc 原生异常（研究线发现），`CARGO_BUILD_JOBS=1`+`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1` 绕过，发布构建采用。

GPU ROI 已实现 ABI 1.5 显式可选级联输入，处于未发布状态。四张合成测试图案回放、每张 8 ROI 对拍，CUDA CPU 像素前处理和 GPU 纹理输入的最大框差为 0.123 像素，CUDA 逐个和批 4 最大框差为 0.947 像素。CPU 与 CUDA 后端的低置信度首选框存在 166.05 像素差异，在相同 CPU 像素输入下复现。隔离进程禁用 TF32 后，四张画面 CPU/CUDA 最大框差为 0.00142 像素，CUDA 逐个与批 4 最大框差为 0.00277 像素。候选新增 ABI 1.5 严格 CUDA FP32 会话选项。采用该选项复测，同一批四帧 CPU/CUDA 最大框差 0.00130 像素，CUDA 逐个与批 4 最大框差 0.00142 像素。20,275,200 字节准入边界、拒绝、CPU 像素显式回退、取消及双模型在途排空已在本机验证。随后在 bus 与 zidane 两张公开实景照片上，以真实 YOLO11n 权重、0.25 置信度阈值验证。两张照片各 8 个固定 ROI 的首框一致，CPU/CUDA 最大差 0.000077 像素，逐个/批 4 最大差 0.000115 像素，动态 ROI 回映最大差 0.00008 像素。三路共用会话的 15 请求均完成，热态账本峰值 60,825,600 字节，排空归零，设备显存采样增量 2 MiB 后回到空闲读数。批 4 在照片实测中较慢，保持显式可选。冷启动峰值、多独立会话和现场负载未测。

v0.7.0 后的语言封装已用本地 Release DLL 和合成 ONNX 模型验证。Python 四路结果逐项对拍，C# 四路异步请求提交后释放 RGB 拥有者，随后等待、取回并确认结果。两种封装均在同一 Context 连续创建、关闭三个池。旧 v0.6.5 Python 绑定以 ABI 1.3 加载 ABI 1.4 双库，完成初始化、单帧结果对拍、确认和关闭销毁。

## v0.7.0 验证状态快照

推理池编排 API（sdk-p1，六批提交 `95ada4e`/`0dea86b`/`f6da676`/`edc3faf`/`1a6a971`/`592a170`，ABI 1.4 兼容新增 65540）：合同冻结（§3.5a/§3.4a——三导出校验顺序/原子回滚/借用语义/两阶段关闭/线程；错误码零新增）→生成链全量同步（freeze 基线升 r11）→池实现（含 batching.rs TOCTOU 既有生产竞态修复：两段锁索引跨锁复用并发缩表越界，改单锁 upsert）→关键路径单测 11（desc 域全分支/槽位记账/回滚/lane 映射/借用关闭语义/非池红线回归）→真装配 e2e 两门（GPU 直传 8 路双分片全流程+CPU 像素批 2 臂）。**验证**：**15 门真机门同进程串行全绿 25.74s**（cb72418 串行口径；门禁执行坑两条登记：真机门须 `--test-threads=1`，rtk 代理清洗 PATH 须直跑 cargo）；workspace **18 套件 528 测试零失败**；fmt 干净/clippy 零新增（3 条 am-camera 预存）/bindings 0 issues/doc-coverage 0/zero-semantics 0/freeze-abi 11 工件过（ABI 1.4 r11）/boundary **291** 源文件 0 业务依赖 0 外部路径。**长稳**：通道层 8h（内存零爬升+GPU 事件零异常）；池 API 形态按组合论证收口——归因复跑进程级采样**池进程稳态 Priv 净增 <8MB/11min≪500MB 判据线（净健康实锤）**，凌晨 8.6GB 系统级增长定性复合外部负载（复跑窗口 top5 进程级抓证当场复现，归因报告存内部证据目录）。**诚实边界**：①194.8 items/s 为发布机单卡研究口径（RTX 5060 Laptop/CUDA ORT 1.31.1/驱动 621.14），跨机需复验；②已知问题（仅退出期，运行期零影响）：宿主退出时池分片会话 DLL detach 路径可能挂死——退出前显式 `amv_pool_close` 排空即可规避，强杀不损数据，列 v0.7.x 首个修复候选；③完整 4h 长窗定量未补跑（组合论证收口，列可选补强）；④批 3 形态固有低吞吐维持 INTEGRATION 分通道指导（GPU 直传批 4 主推）。

## v0.6.5 验证状态快照

v0.6.4 热修（`314d2f8`，内部修复无 C ABI 变更）：①单帧非攒批回归——B1.2 执行槽入口（exec_stage::enter）长度一致性检查排在零份额旁路之前，单帧一次性全额入账路径（admissions 非空+charges 空）len 1≠0 误报 `sdk.internalError`(28) 整笔收口；vocab/world 真实权重 4 门暴露（rc=28 无 detail）；**归因链=v0.6.3 worktree 同环境对照绿（排除驱动/ORT 漂移）→input.rs 还原仍红→scheduler 家族还原绿→锁定 B1.2→diff 复核定案**（全程 `.local/evidence/sdk-b1-20261112/p1-attribution.md`）；修复=旁路前置（两块对调）。②GpuPipeline 冷启动顺序——establish_context（cuInit+primary）前置到 PTX JIT 前，业务线程冷启动首触 GPU 帧不再必败伪 JIT(3)（探针可复现→RED→GREEN 回归测试 coldstart_tests）。**验证**：4 门真实权重门修复后单跑/跨进程全绿；**ignore 真机门 13 门分进程全绿**（同进程串行末门 ctx 初始化挂=预存测试隔离债另登记——12 门资源累积，与本次修复无关）；am-vision 112/0+am-vision-ffi 69/0；m2b 四门/批 C 组装/B1.5 双 Model 门回归全绿；workspace 门禁绿；boundary 287。**影响面如实登记**：v0.6.4 制品单帧非攒批路径全坏（rc=28 误报），AM-RT 在 0.6.3 无恙未受影响；**发布清单已补"ignore 真机门全列"检查项**（B1 验证盲区制度修复：守恒族全攒批路径，单帧覆盖仅 ignore 门）。

## v0.6.4 验证状态快照

多 Model ctx 预算分配（sdk-b1，`5388a78`，内部调度无 C ABI 变更）：执行错峰槽（exec_stage.rs——补账前取槽至执行完成释放，等槽批仅持守恒挂起份额；GPU 直传零执行份额批整槽旁路）+编码路径瞬态源面出账+包络混记注释；护栏总量/三层/整笔拒绝/守恒全保持。**验证**：am-vision-ffi **69 单测**（新增守恒族 8 测：双 Model 叠加压力/交替周期归零/等槽保持归还/取消 TTL 排空）+真机 ORT 门 **4/4**（新增 `batch_e2e_dual_model_staggered_execution`：双窗成批/逐项身份/在途关闭互不干扰/守恒/零拒绝）；workspace **9 套件两轮全绿**；boundary **287**（源码修订链式登记）；freeze-abi 过（ABI 工件零触碰）。**量化四臂（SDK 真路径 8 路 25fps w40，ORT 1.31/CUDA 13.4.92 驱动 621.14，`.local/evidence/sdk-b1-20261112/`）**：CPU 双 Model 批 3=20.1/批 4=56.5（拒绝 75%→0 解锁达成；错峰买可用性不买速度——持槽含预处理串行化；CPU 通道最优仍批 2=97.7）；**GPU 直传双 Model 批 4=194.8 items/s 零拒绝（200 需求 97.4% 实质满额）——120-150 承诺超额兑现，"8 路不可达"旧论（CPU 像素口径）在 GPU 直传通道推翻**；单 Model 回归锚 56.3（vs 57.9 差 2.8% 噪声内）；GPU 事件日志四臂零新增。**诚实边界**：批 3 反常（全凑满零截止却 355ms/批 vs 批 4 131ms）归因未拆分登记后续；GpuPipeline 冷启动顺序（PTX JIT 先于 cuInit，首触线程无上下文必败，探针可复现）登记下轮立项候选；GPU 直传 194.8 为开发机单卡口径（RTX 5060 Laptop/621.14），跨机需复验。
当前本地源码与发布版一致时以 Git 提交核对；领先时以本页登记。发布流程见 [发布清单](RELEASE_CHECKLIST.md)，逐项验证证据见 [verification 目录](verification/)（按主题命名，含过程记录）。

## v0.6.3 验证状态快照

**文档指导版（零代码语义变更；变更=文档+公开头文件注释+版本元数据 0.6.2→0.6.3，DLL 功能与 v0.6.2 一致）**：攒批吞吐优先配置指导（批 I，`sdk/INTEGRATION.md` 攒批段）——max_wait_ms 取凑满时间尺度（非帧周期）、非整除路数陷阱（6 路×批 4×wait=40 比全凑满低约 45% 且可低于逐项；wait=120 恢复 99.9% 凑满）、wait=0 即入即发语义（代码取证：零宽窗口零挂起零死等）；CHANGELOG 定版段含 131 复测锚点（621.14 驱动+ORT 1.31.1/CUDA 13.4.92：8 路 w40=57.9 items/s 1868 批零截止 meanN=4.00；6 路陷阱=34.8 items/s 截止/凑满交替同构 meanN=3.0——陷阱形态跨驱动复现、幅度驱动敏感）与 CUDA 口径勘误（126 口径实为 12.9.79，按包内 DLL ProductName 订正）。**门禁债清偿（v0.6.0 起积压，本版复跑清单检查暴露）**：dual-dll-abi-types.md 为 0.6.0 新增级联/跟踪/帧导出结构体补 20 个零值语义标注（语义取自合同正文，如 track_id/hits 输出自 1 起故 zero=invalid；一处旧标格式规范化）+生成链同步 am_sdk_types.h 注释（41 行，仅注释无布局/签名变化）+INTEGRATION 四个级联/跟踪 API 名展开全名（过整词文档检查）。am_sdk_types.h 属 11 项冻结工件（全文哈希），注释同步触发 freeze 检查漂移——**freeze 基线已重算并单文件归因**（无 ABI 布局/符号变化，同批 D 先例）。**验证**：workspace **508 绿/0 失败**（与 0.6.2 基线同数，PM 独立统计）；fmt 稳定/clippy（1.97.1 工具链组件当日补装）零告警/bindings 0 issues/doc-coverage 0 missing/zero-semantics 0 problems/boundary 285（0 业务依赖/0 外部路径）/freeze-abi check 过（11 条目，重算后基线）；探针+复测两臂监控全绿（事件日志零新增、显存满载后完全释放、内存账本闭合）——报告 `.local/evidence/driver-incident-20261112/`（内部证据，不入公开包）。**诚实边界**：本版无新代码验证面（测试数字沿用 0.6.2 基线的同一代码语义）；131 复测为本机单卡口径（RTX 5060 Laptop/621.14），跨机绝对值需接入方复验；GPU 推理验收范围维持既有登记（Windows x64 CPU+硬解探测为主体验收面）。


## v0.6.2 验证状态快照

档案注册表演进（批 D，兼容新增、无 C ABI 变更）：`detect-yolo-world-batch-v1`（批动态双输入亲和档案：images 批动态 H/W 钉 640、3-D txt_feats 批轴对齐、输出 anchors 钉 8400）+ `text-encoder-clip-dynseq-v1`（seq 动态文本塔）——批 C 登记的"无可批 PROFILE 图像模型"接口级缺口解除；vocab 注入批语义 CPU/GPU 统一（3-D 逐帧快照，[N,rows,dim] 字节同构）；raw-affinity 解码 anchors 改取实际运行期形状（批拆分 [1,rows,anchors] 与既有行主序索引兼容，无新解码分支）。**验证**：分 crate 测试 am-vision **112**/am-vision-ffi **61** 零失败（新增单测：注册表条目/批轴解码等价/注入秩/平铺/估计）；真机门全绿——新增 `world_e2e_cpu_real_vocab_pipeline`（真实 text.onnx dynseq 塔编码词表→真实 world.onnx 3-D 注入→亲和解码/hits/label；输出布局 [1,7,8400] 断言）与 `world_e2e_gpu_direct_and_batch_window_match_single_runs`（GPU 直传 vs CPU PNG 亲和分通道 Pearson r=0.927；真实 PROFILE 图像攒批 4 帧成单物理批 formed=1/batched=4/rejected=0 四跑稳定、逐项 raw 对拍 ≤0.4% 相对漂移）；既有 vocab e2e 2/2、FFI batch e2e 3/3、批合同门、GPU 组装门全绿；clippy 改动 crate 零告警；fmt 稳定；闸门 **284**（11 条目链式登记 reason 历史保留）。**诚实边界**：仓内无 CLIP tokenizer——真实权重 e2e 用词表区间确定性 token ids，断言管线正确性不主张检测语义（真实画面近噪声段最高亲和分 0.0042，阈值自 raw 头自标定）；攒批夹具为 world.onnx 仅追加 `am.sdk.batch_axis` 元数据的派生副本（权值不动，`.local/tests/yolo-world-batchd/`）；anchors 全动态档案与级联路径维持既有输出预算回退口径；~~freeze 基线漂移与 vocab.rs 登记缺口~~ **已随 87c4d68 处理**：freeze-v1 全数归因重算（v0.6.0 ABI 1.3 工件+档案演进六工件，RELEASE_CHECKLIST 补 `freeze-abi --check` 检查项堵漏）+vocab.rs 补登记（闸门 284→285）。

**批 E（攒批入账分期计费 `170dd7e`）**：挂起期按实驻留入账（1080p→640 从 17.8MB 全周期估算降至 3.1MB/项）+执行期补账（载荷+合并+ORT 副本 24B/px），上限/拒绝语义零改动。**验证**：池化复跑窗口凑满 3.0→4.0（1843 批凑满触发 1840/截止仅 3）、背压 30270→0、吞吐 +14%（31.9→36.5fps）；净增 3 守恒单测；FFI 批 e2e 3/3（含 6MiB 收紧判别门）。尾延迟形态变化（排队替代节流）入档供接入方按 SLA 选择。

**批 H（JPEG 导出编码提速 `8d48137`）**：底座换 jpeg-encoder 0.7.1（纯 Rust+AVX2 运行时检测），4:4:4 钉死对齐旧底座实际输出（image crate 文档标 4:2:2 与实际不符，SOF0 取证）；纯编码 1.75-2.1x（1080p q95 50.9→26.6ms），PSNR 全档略优+守门测试（参考编码 ±3% 尺寸界）；许可三文本入 NOTICES。诚实边界：端到端增益按 NV12→RGB 换算占比折算，接入方绝对值复验中（AM-RT 排期）。

**workspace 508 绿/0 失败**（发版时点全量）；指纹闸门 **285**；bindings 0 issues；ABI frozen 11 不变。

## v0.6.1 验证状态快照

张量攒批回归修复（`631e68d`：10-29 起 N>1 动态轴合批整批拒，复原为输入首轴对照）+ GPU 直传开放词汇（`fd88c62`）+ 图像项自适应攒批（`cdd3027`：聚合键执行入口三态/GPU 帧接受段预处理 §3.3a 落地/设备侧组装/逐项 params，合同 §3.5 图像项条款，无 C ABI 变更）+ H264 会话 NotReady 热修（`d6b8fc1`）。**验证**：workspace **499 零失败**（两遍口径：批 B 验收 496+批 C 净增 3）；真机门全绿——批合同门（动态 1/3/64+固定 4+漂移拒斥）、FFI `batch_e2e_*` 2/2（真实窗口攒批→单物理批→逐项身份）、vision `gpu_batch_assembly_matches_single_runs`（world.onnx 四帧设备组装对角差<0.8%）、vocab GPU 直传对拍 5/5（PM 独立复跑）、vocab CPU/GPU e2e、m2b-repro full 8 零崩（fmt 后复跑）；clippy 改动 crate 零告警；闸门 **284**；Release DLL 重建+冒烟（攒批真实活跃）。**池化 8 路对照**（`POC/core-performance/briel-c-pooled.md`）：结构性负结论——池化+攒批 AUTO 31.9fps vs 独立会话裸跑 109.6fps（单池化会话串行进单 CUDA 流，合批 3.0×1.36=4.1 流当量<8 流），不推翻"8 路 25fps 单卡不可达"；尾延迟池化 P99 114ms vs 348ms 平滑。**登记未验/边界**：①档案注册表无可批 PROFILE 图像模型（单输入档案锁 batch=1/双输入亲和档案互斥）——机制就绪待注册表演进（PM 决策，world-head 3-D 文本条目候选）；②CPU/GPU 双路径 vocab 批语义暂不一致（受①保护，演进时统一）；③预处理入账口径 17.8MB/项估算 vs ≈8MB 驻留把攒批在飞压到 3 项（多路必背压，重估待办 PM 决策级）；④vocab H2D 搬运成本未单独计时；⑤真实 world.onnx 权重 GPU 直传未测（验证基于合成亲和头）。

## v0.6.0 验证状态快照

ROI 级联推理 C1（合同 §4.1，接入方 8 点确认冻结 2026-11-05）+ 跨帧目标跟踪（§4.1b，接入方 6 点确认，参数上限入合同）+ 单帧 JPEG/PNG 导出（§2.11，接入方确认归档）+ Rust 直依赖调用示例。**验证**：workspace **489 项测试零失败、两遍稳定**（含 cascade 7 核心+1 FFI、tracker 13 核心+7 FFI、frame-export 7 核心+1 FFI）；真机门——cascade/vocab 双 e2e 合成模型真实 ORT 全断言过（静态 ROI 坐标回映手算值/动态规则 primary_index 溯源/独立阈值/跳过计数三态）；帧导出 PNG 无损往返+JPEG 质量序+NV12 矩阵换算单测过；并行测试 ctx 准入竞争修复后两遍全绿；clippy 新码零告警；check-boundary 282 全绿；C ABI `AM_ABI_VERSION` 65539。下游实证背景：小目标全图直检 0.167→ROI 放大 0.461、检出框 7→15（EXTENSION_CAPABILITIES C1）。

## v0.5.0 验证状态快照

FILE 阶段 2（seek/duration/rate/loop，合同 §2.4 阶段 2 段，接入方两轮确认锁定）+ 开放词汇 A1+A2+A4（`amv_vocab_bind` 词表绑定+亲和解码+命中明细，合同 §4.1，接入方六点确认含 pad 约束）+ 审计清偿批（P1/P3/L1/L2 修复+测试盲区 9 项回归锁，七项合同语义全过未发现真缺陷）。**验证**：workspace 447 项测试零失败（437 常规+10 真机门）；真机门——camera 侧 14+12 passed（含 seek 三模式/EOF 复活/循环重放/EXACT 过滤集成、FILE EOF 全链），1 项 D3D11 并行 flaky 单跑通过（既有已知非回归），8 项回环 RTSP/native 隔离库环境缺与基线一致；**vocab 双模型端到端已验**（合成 Gather 常量表文本塔+Mul 广播亲和头，真实 ORT：bind→注入→组内合并→hits/class_name→re-bind 换词全断言过；过程中修出三个集成缺口——load 动态维/图像入口双输入放行/hits 所有权，见提交 2e7a723）；clippy 零告警；check-boundary 270 全绿（指纹链式更新）；四机器闸门（bindings/doc-coverage/zero-semantics/freeze）过。C ABI `AM_ABI_VERSION` 65538。运行库版本：FFmpeg s5-encode（avcodec-63）。

## v0.4.0 验证状态快照

FILE 阶段 2 seek、时长、倍速和循环正在开发，尚未完成验收。

FILE 本地文件源（P1-8 阶段 1）+ 开放词汇首批（A3+B1）+ v0.3.3 制度批次三合一功能版。**三源真机回归**（新 DLL，`.local/tests/v040-regression/`）：RTSP transport 三 codec（H.264/H.265 D3D11VA verified + MJPEG CPU，16 包/慢订阅者重同步/时间基/RA 真实像素全绿）+ ISAPI 抓图全场景（jpegExact/生命周期/推拉互斥/回调屏障）；FILE 源 EOF 全链集成测试通过（真机 FFmpeg，29 包 PULL→EOF→排空 streamEnded→三段式关闭）。Rust 回归 428 项零失败；check-boundary 过闸（含 vocab 批新增 5 条目）。Framing 的 is_avcc 链自洽判定（P1-8 修复）对三源行为等价已由上述回归实证。运行库版本：FFmpeg s5-encode（avcodec-63）。

## v0.3.2 验证状态快照

本版已验运行库版本：OpenVINO 原生 2025.3.0/2026.1.0/2026.2.1；FFmpeg 以 build-record/验证记录为准。

接入方反馈批（2026-11-03，已发布）。本批 P0-1（E1 OpenVINO 原生引擎 2026.x）：
根因为运行库发现只认未文档化的 `AM_SDK_OPENVINO_DIR` 环境变量、未接 ABI 合同的
runtime_dirs 通道——非 2026.x 符号断裂（绑定 28 符号在 2025.3.0/2026.1.0/2026.2.1
三线的 openvino_c.dll 导出表全存在，元素类型枚举/结构布局/变参签名逐项一致；2026 线
仅新增 2 个日志回调符号）。修复后单一绑定覆盖两线，**三线真机实测**（装载+编译+推理+
对拍 yolo26n-pose，matched max_rel 6.468e-4 / 3.150e-4 / 3.375e-4，容差 5e-3）+
回退矩阵三档 + runtime_dirs 接线用例（不设环境变量）全过；失败详情经 amv_get_last_error
携带检测版本与期望线。运行库存放：`.local/deps/ov2026|ov2025`（wheel 解包，仅本地验证用）。

批 D（2026-11-03）：错误详情架构——动态文本驻留下沉 sdk-support（`diagnostics::intern`
内容去重进程保活 + `Detail::dynamic`），键纪律修订为稳定键与动态详情并存；FFmpeg 版本
不匹配失败详情带具体库名/检测主版本/期望主版本（经驻留下发）。C#/Python 绑定参数对齐
（init 线程参数、intra_threads、confidence、capture deadline、PULL 队列参数、stream 主/子
码流、abi_version/magic 生成常量）。本地验证：三 crate 单测/clippy 绿 + 工作区 check 绿 +
check-boundary 过闸（270 核心源码，指纹链式登记）+ C# 全绑定 csc 语法编译通过 + Python
py_compile/生成器 --check 通过。真机 FFmpeg 版本不匹配注入复现未跑（需伪造主版本 DLL，
登记为待验，不阻塞：构造路径为纯格式化字符串，映射单测已锁定）。

## v0.3.1 验证状态快照

审查缺陷全量修复版（无 ABI 破坏，前缀兼容增强）：issue #2-#8 七项缺陷（尺寸校验防越界、
CUDA 缓冲 RAII、MP4 finish 错误上抛、RTMP 连接预算与可唤醒退避、编码 PTS/DTS 时基贯通、
旧版结构前缀协商、C# 绑定补齐）+ 全仓模式审计同修 10 项 + 扩展审计高危 2 项（攒批三锁环
挂死、DETACH 排空假成功）。GPU 帧句柄族导出在闭环落地前统一 notSupported（安全加固）。
合同语义六条增补见 dual-dll-abi.md 2026-11-01 段。Rust 回归 402 项通过；真机 FFmpeg 套件
全绿且失败清单与改动前基线逐项一致；源码修订按仓规则登记 sdk-source-revisions.json。
审计报告与待修台账：.local/audit/（F7 丢包、F5 GPU 借用窗口等后续项不随本版）。

## v0.3.0 验证状态快照

GPU 零拷贝主线全通（M2a/M2b/M2c/M3：每帧 2 次 GPU 内拷贝/0 CPU 回读，真机检测级对拍）；
五推理引擎（E1 OpenVINO 原生对拍 matched max_rel 3.15e-4+回退矩阵三档）；M5 热切换 8 线程
零失败；S5 编码推流 MP4 可播放验证（RTMP 真机待环境如实登记）；R 系列 7 项闭环；V1 消费方
验证体系（基线门禁+场景库+画像过堂）。Rust 回归 408 项通过。FFmpeg 运行库自本版起为 GPL
构建（libx264，用户 2026-10-29 裁决；x264 许可文本随包分发）。

## v0.2.1 验证状态快照

v0.2.1 为兼容优化版（无 ABI 破坏）：R1 帧预算 96→128MiB、R2a/R2b Digest 缓存与
订阅节拍修复、R6 JPEG 直解 RGBA（720p 下游判据达标 GAP 0.79ms，1080p 复测进行
中）、M2c GPU 搬运审计计数。Rust 回归 387 项通过；R7（MJPEG 流源）与 GPU 零拷贝
主线属未发布扩展，不随本版。R6-1080p 保留 1.71ms 已知差距（2026-10-29 裁决：
纯 Rust zune-jpeg 内核取向，不引入 C libjpeg-turbo 依赖；720p 判据达标 GAP 0.79ms）。构建指纹见包内 build-record.json 与 SHA256SUMS.txt。

## v0.2.0 验证状态快照

| 面 | 状态 | 依据 |
| --- | --- | --- |
| Rust 回归 | 384 项通过 / 20 项环境门槛忽略 | workspace 全量（fmt/clippy 零告警） |
| ABI 与生成物 | 冻结 11 工件；C 头/Rust/Python/C# 生成链零漂移 | generate-abi/-rust-abi/-python-abi --check |
| 来源完整性 | 239 核心文件指纹核查通过 | check-boundary |
| 包级验证 | 四语言示例（C/C++17/C#/Python）+ 攒批六场景在干净包端到端通过 | [打包复验](verification/sdk-r5-clean-bundle-20261028.md) |
| 硬件解码 | H.264/H.265 360p～4K（含 Main10）D3D11VA 实测全过；断流恢复/在途关闭矩阵通过 | [矩阵](verification/sdk-r2-h24-matrix-20261027.md)、[故障矩阵](verification/sdk-r2-h23-fault-matrix-20261027.md) |
| 资源对照 | 单/双/四路 MET-2 对照：CPU RGBA 回读合同下硬解不省 CPU（如实负结论，1/2/4 路一致） | [单路](verification/sdk-r2-current-s1-20261027.md)、[双路](verification/sdk-r2-h24-dual-20261027.md)、[四路](verification/sdk-r2-h24-quad-20261028.md) |
| 自适应攒批 | 合成单测 + 真实 ORT 六场景通过（两项调度死锁已修复）；小模型无显著吞吐差异（如实保留） | [实测](verification/sdk-r5-batching-live-20261028.md)、[对照](verification/sdk-r5-scheduler-compare-20261028.md) |
| 模型档案 | Pose 真实图对拍通过；Detect/Classify/Segment/OBB 无本地权重未验 | [档案收口](verification/sdk-r4-ownership-closeout-20261027.md) |
| 推理后端 | CPU 全链验证；CUDA/OpenVINO 分层诊断齐备但无 EP 实测 | [分层收口](verification/sdk-h5-layered-closeout-20261027.md) |

## 未验边界（不阻塞使用，选用前自查）

- 四类模型真实权重端到端；CUDA/OpenVINO 执行后端；现场相机码流
- 多路（>4）解码、跨进程崩溃恢复、固定时长长跑（2026-10-27 用户裁决移出强制门槛）
- GPU 原生输出（非 CPU RGBA 回读合同）下的硬解收益

## 历史候选与制品

- 公开制品：v0.1.0 / v0.2.0 GitHub Release（校验以 Release 附带 SHA256SUMS 为准）
- 本地开发候选：`.local/releases/candidates/`（含 r4clean/r5clean 过程候选，非公开下载依据）

## 历史验证流水归档说明

2026-10-26～28 优化期间的逐批验证播报已并入上方快照与 [CHANGELOG](../CHANGELOG.md)；原始过程记录保留于 [verification 目录](verification/)（工作透明，不作为发布验收替代）。R1～R5 OpenSpec 工作流全部收口，入口见 [OpenSpec](../openspec/README.md)。
