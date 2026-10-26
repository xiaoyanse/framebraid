# FrameBraid SDK 开发约定

- 一个仓库管理 Media SDK 与 Vision SDK；无任务、工位、报警、PLC、数据库或 UI 业务依赖。
- 七个 crate 的历史提取指纹保留原字节；FrameBraid 为公开维护源，AM-RT 已按完整 Git 提交固定消费，升级仍须独立评审，不在两仓分别修核心。授权修复通过 `sdk-source-revisions.json` 绑定原清单摘要、评审基线、变更前后指纹及原因；不覆盖原始 `sdk-source-manifest.json`。
- 自有代码 Apache-2.0，保留第三方许可；不得自动发布、创建远程、推送或打标签。
- 公开署名使用 GitHub 用户名 xiaoyanse，不自动公开登录邮箱。README.md 为英文入口、README.zh-CN.md 为中文入口；能力、限制、命令与验证状态同步维护，文档入口见 docs/README.md。
- 开源准备和实际发布分开，发布前按 docs/RELEASE_CHECKLIST.md 核对；历史设计/提取清单不改写为新验收，不把 Debug 证据或旧宿主制品当作 FrameBraid Release。
- 公开文档保留当前接入/合同/支持说明和简洁 CHANGELOG；过期审计、交接、探索稿先按原字节归档到忽略的 `.local/archive` 再清理并修正引用。生成器输入、ABI 历史链、来源指纹及第三方原件不当过程垃圾删除。
- 兼容 amc_/amv_ 和既有 Rust crate 名称；品牌更名不改变 ABI。
- 修改接口先改 docs/design/dual-dll-abi*.md，使用 scripts/sdk 生成器同步头、Rust、Python/C#；禁止手改生成物。
- cargo fmt、cargo clippy、受影响测试、ABI 检查与 check-boundary 为验证入口。构建输出统一在本仓 `.local/build/current`，验证与归档分别放 `.local/tests`、`.local/evidence`、`.local/archive`；不再创建项目根外的散落目录。
- 测试使用合成数据及随机回环端口，不连接现场设备；模型、运行库、凭据、内部证据不得提交。

## 版本与公开接口同步

- 功能变更先分类为内部优化、兼容新增或不兼容变更。开发期记入 CHANGELOG 的 Unreleased，发布时统一定版；不要求每个提交改版本或打标签。SDK 包版本、C ABI 主/次版本、Git 提交和制品 SHA-256 分开管理。
- 内部优化若公开接口、行为、依赖与支持范围不变，不升级 ABI；兼容新增在发布时更新 SDK 版本，公开 C 接口演进另按 ABI 合同定主/次版本。C ABI 签名、布局、枚举值、所有权或既有语义发生破坏性变化，必须升级 ABI 主版本并提供迁移说明；预览版也不允许静默覆盖既有合同。Rust API/语言封装的源码兼容性另行核对，不能因 C ABI 不变而省略。
- 新增/修改公开能力前，先同步 `docs/design/dual-dll-abi*.md`；涉及交付边界、共享帧或模型档案时同步对应设计及档案。写清参数/默认值、结果、错误、所有权、生命周期、回调线程、队列与失败/恢复行为；不影响的合同注明理由，不全量重写。
- 接口定义/生成器为受控源；用 `scripts/sdk` 同步受影响的 C 头、Rust ABI、Python/C# 绑定、常量与布局，不手改生成物。能力查询和实际支持必须一致，新增能力、缺运行库及未验证状态分别核对；变更需通过相关生成检查、布局/兼容检查及语言调用测试。
- 每个功能/接口变更必须核对：`CHANGELOG.md`、中英文 README/快速接入、`sdk/INTEGRATION.md`、示例、`docs/SUPPORT.md`；仅修改受影响项并记录不适用原因。验证状态写 `docs/RELEASE_STATUS.md`，待办状态回写 `docs/ROADMAP.md`，计划不得冒充已验收。
- 发布候选须把源码提交、SDK/ABI 版本、冻结摘要、构建平台/特性、依赖、文档/绑定和制品校验和绑定到同一候选；同步 `sdk/RELEASE-NOTES.md` 与发布清单，说明兼容范围、迁移步骤及未验项。文档取对应 tag/提交，不能用滚动 main 文档代替旧制品合同。
- `sdk-source-manifest.json` 是历史提取指纹，不是每次发布自动覆盖的清单；ABI 冻结指纹用于防漂移，不等于 DLL 身份或兼容结论。修改核心/合同须评审新的基线，保留旧记录和迁移关系，禁止为过检查盲目重算哈希。
- 下游固定消费版本/提交及匹配绑定，以版本协商、struct_size、能力查询和实际依赖检查决定启用或回退；不从哈希或 GitHub 最新文档猜兼容，不默认自动替换 DLL/改宿主代码。本要求是维护规范，不代表已有自动升级或在线合同同步服务。
- 运行时包版本取 Cargo 版本，目标取实际 target triple；现有 notes 返回构建时提交、源码输入 clean/dirty/unknown、profile 与公共支撑层共享帧编译特性（实际每库能力另查）。无自身 Git 仓库不猜提交；该信息不替代制品 SHA-256。发布前须核实际载入库与候选清单一致，查询能力不等于自动升级。

## 本地目录与宿主接入

- 唯一维护根为本仓与 AM-RT 宿主仓；归属和版本交接见 `docs/LOCAL_LAYOUT.md`、`docs/SOURCE_MAINTENANCE.md`。不在宿主恢复核心副本，也不把本地未公开提交偷偷写成公共 Git 依赖。
- `.cargo/config.toml` 默认 `.local/build/current`；测试/证据/运行库/历史包分类保存。覆盖 `CARGO_TARGET_DIR` 也用本仓 `.local/build` 子目录。正式制品原字节保留，测试和配置不混入公开包；过期缓存确认闲置后及时回收。

## SDK 优化 OpenSpec 与持续目标（2026-10-26 用户确认）

- R1～R5 优化必须按本仓 `openspec/README.md` 与五个 change 推进；恢复先核 `openspec list`、当前 tasks 和最新验证记录，不借用 AM-RT 的 OpenSpec 状态。
- 用户授权 main 串行开发和阶段测试/量化/本地提交，不派子代理。先 R1 与 R5 可独立优先项，R5 后续项按依赖推进；不得遗漏已确认完整路线。
- SDK量化验收统一引用 `docs/PERFORMANCE_ACCEPTANCE.md`（MET-2）：每包测前定主目标与保护指标，区分同路径回归与跨后端资源取舍；历史5%为调查标签，不作统一通过/失败线。原始数据/历史评语保留并追加复核；不得因改口径宣称新测试通过或放宽功能/预算/所有权底线。
- 任务完成以实现和实测证据核定；目标工具状态、计划完成或历史测试不能替代验收。发生范围/接口/预算变化先同步 OpenSpec，阶段提交时更新任务与记录。
- 源码和测试均在 SDK，AM-RT 不改、不自动升级；禁止推送、标签、发布和现场设备操作。性能优化不能牺牲输出正确性、有效交付率、预算、租约或关闭安全。


## 命令被策略拦截时

- 带 `-Force` 的检查命令（如 `Get-ChildItem -Force`）应与删除、修改、服务启停等操作分成独立工具调用，不要放在同一脚本中；仅换行或用分号分隔无效。
- 遇到 `blocked by policy`，先检查完整脚本是否混用了上述命令和参数，不要直接归因于权限不足。
- 每步执行后核验实际结果；仍被拦截时如实记录命令和错误
