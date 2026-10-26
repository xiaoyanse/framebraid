# SDK 优化持续目标与 OpenSpec 入口

> 当前用户指令：仅完成[MET-2指标与验收](../docs/PERFORMANCE_ACCEPTANCE.md)文档收口，不继续代码开发、不重跑基准；恢复实现等后续指示。原始数据/历史记录及未验项保留。

持续目标：在 FrameBraid main 串行完成已授权 R1～R5，阶段任务、验收、性能前后对比及本地提交统一由本仓 OpenSpec 管理。当前无发布授权，不影响 AM-RT。

## 开工和中断恢复

1. 读本仓 AGENTS.md，核 Git 状态及外部改动，保护既有成果。
2. 运行 `openspec list --json`；读当前 change 的 proposal、design、specs、tasks 与相应验证记录。
3. 按真实未完成任务继续；每个完整小包测试、量化对比并提交，不把目标描述当作完成证明。
4. 修改计划、接口或验收边界时先同步对应 OpenSpec，不能默默删掉路线任务。指标/验收统一引用MET-2，R1新增量化前置不得冒充已测。统计只引用各 tasks，不在本入口维护第二套勾选。

## 阶段包与优先顺序

| 阶段 | Change / 任务入口 | 依赖与顺序 |
| --- | --- | --- |
| R1 | [性能基线](changes/sdk-r1-performance-baseline/tasks.md) | 最先：固定负载、计时边界、证据和预算 |
| R5 优先项 | [推理调度](changes/sdk-r5-inference-scheduling/tasks.md) 第 1 组 | 在 R1 上先核现有公平调度、取消/替换积压和低延迟路径，不等自动组批 |
| R2 | [数据热路径](changes/sdk-r2-frame-hot-path/tasks.md) | R1 后，转换上下文→采样前移→张量复制；共享帧/SIMD 按实测纳入 |
| R3 | [有界压力与恢复](changes/sdk-r3-bounded-recovery/tasks.md) | 沿用 R1/R2 候选，补组合故障、退出和长跑 |
| R4 | [语言接入与档案](changes/sdk-r4-language-profiles/tasks.md) | 所有权与错误稳定后，完善封装及五类代表模型 |
| R5 后续项 | [推理调度](changes/sdk-r5-inference-scheduling/tasks.md) 第 2～3 组 | 依赖 R1～R4 的数据与模型合同，显式可关闭组批/后端策略 |

完整范围和三项目参考位置沿用 [路线图](../docs/ROADMAP.md)。R5 提前实施部分是用户最新优先级，不等于跳过其前置条件。已有基础优先增强，不新增第三个 DLL，不承诺全链路零拷贝。

2026-10-26 用户追加硬件能力：R1 最小基线后，R5 第 1 组和 H5 推理硬件探测/自动选择、R2 H2 原生硬解探测/自动选择作为前置优先项，先于自动组批；随后继续原路线。H2/H5 具体范围及验收以对应 tasks/specs 为准。基础探测/选择不再留 E2，专用后端/GPU 零拷贝/其他系统仍后续；不改变 AM-RT 或浏览器 WebCodecs。硬件不可用时验证诊断/回退，真实硬件效果保持未验。

## 完成和禁止边界

H2.0显式Rust原语与本机合成硬解基础证据见[阶段记录](../docs/verification/sdk-r2-hardware-foundation-20261026.md)。不代表自动策略/C ABI/RTSP、所有编码特征或长跑已验，H2.1～H2.4继续保留。

- SDK 内 main 串行、不派代理。AM-RT 固定依赖不自动升级，不修改其源码、任务和验收。
- 不推送、不打标签、不发布，不接触用户业务库或现场设备；原发布包不改写。
- 构建/测试/证据置于 .local 分类，及时回收可重建大样本，保留精简数据与指纹。
- 每阶段核 ABI/生成物/来源修订、必要中英文文档和未发布记录。外部 .gitignore 改动保留，不代交。
- 旧版本 CPU/历史验证不能替代本轮候选数据；真实模型、硬件和长跑未验项不得伪装通过。
- 用户已将持续目标正文补入“使用 OpenSpec 推进”；本入口、配置及 AGENTS 同步约束中断恢复，不用目标状态替代任务证据。
