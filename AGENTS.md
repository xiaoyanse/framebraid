# FrameBraid SDK 开发约定

- 一个仓库管理 Media SDK 与 Vision SDK；无任务、工位、报警、PLC、数据库或 UI 业务依赖。
- 七个 crate 与来源提交逐文件同源；原仓解耦交原团队合入。此阶段不要两边分别修补核心；后续唯一维护源及宿主版本引用另行确定。
- 自有代码 Apache-2.0，保留第三方许可；不得自动发布、创建远程、推送或打标签。
- 公开署名使用 GitHub 用户名 xiaoyanse，不自动公开登录邮箱。README.md 为英文入口、README.zh-CN.md 为中文入口；能力、限制、命令与验证状态同步维护，文档入口见 docs/README.md。
- 开源准备和实际发布分开，发布前按 docs/RELEASE_CHECKLIST.md 核对；历史设计/提取清单不改写为新验收，不把 Debug 证据或旧宿主制品当作 FrameBraid Release。
- 公开文档保留当前接入/合同/支持说明和简洁 CHANGELOG；过期审计、交接、探索稿先按原字节归档到仓外再清理并修正引用。生成器输入、ABI 历史链、来源指纹及第三方原件不当过程垃圾删除。
- 兼容 amc_/amv_ 和既有 Rust crate 名称；品牌更名不改变 ABI。
- 修改接口先改 docs/design/dual-dll-abi*.md，使用 scripts/sdk 生成器同步头、Rust、Python/C#；禁止手改生成物。
- cargo fmt、cargo clippy、受影响测试、ABI 检查与 check-boundary 为验证入口。构建输出放仓外 D 盘。
- 测试使用合成数据及随机回环端口，不连接现场设备；模型、运行库、凭据、内部证据不得提交。
