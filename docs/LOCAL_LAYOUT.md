# Local layout / 本地目录与宿主关系

FrameBraid is the sole SDK source repository. AM-RT consumes a pinned Git source
revision and statically links the core; it does not keep another editable copy or
load these SDK DLLs. Native DLLs remain a separate delivery option for external callers.

FrameBraid 是 Media/Vision SDK 唯一维护源；AM-RT 消费固定 Git 源码版本并静态编译，
不保留另一份核心，也不加载这套 SDK DLL。2026-10-27宿主显式锁定优化中间源码提交
`9ecd93307fc46947cf1aa70be165077e1cd1f072`，已获授权推送GitHub main供团队获取，
未打标签或新建Release。旧v0.1.0保留；其他main变更不会自动进入宿主。

## Owned local artifacts / 仓内产物

All project-specific outputs stay under ignored `.local/` inside their owning
checkout. Do not create scattered top-level build/test/delivery directories.
Cargo/toolchain caches are shared tool infrastructure, not extra project repositories.

项目专属产物在所属仓忽略的 `.local` 下分组，不再创建散落的 D 盘目录；
工具链及 Cargo 公共缓存不属于额外维护的源码副本。

| Folder / 目录 | Purpose / 用途 |
| --- | --- |
| `.local/build/current` | Default Cargo output / 当前临时编译 |
| `.local/build/legacy` | Migrated rebuildable caches / 可重建的历史缓存 |
| `.local/tests` | Disposable fixtures / 隔离测试 |
| `.local/runtimes` | Private native dependencies, models and build sources / 私有依赖、模型及构建源 |
| `.local/releases/published` | Preserved published packages / 已发布包原字节 |
| `.local/releases/candidates` | Unpublished candidates / 未发布候选 |
| `.local/evidence` | Compact verification and migration manifests / 验证及迁移清单 |
| `.local/archive` | Historical materials and private configuration / 历史材料与私有配置 |
| `.local/tools` | Local migration/diagnostic helpers / 本地辅助工具 |

Run builds from the checkout root. `.cargo/config.toml` defaults to
`.local/build/current`. If setting `CARGO_TARGET_DIR`, use an absolute child of
this checkout's `.local/build`; do not share a junction with another checkout.
Maintained build/package helpers enforce their output categories. Tests and
evidence are separate from source and publishable packages even though they are
inside the same checkout.

从仓根构建，默认使用上述目录。覆盖输出时也必须保持在本仓分类下，
不链接共享目录；`.local` 不进源码包，也不能代替外部备份。
迁入的旧包保留原摘要，历史记录保留原路径，当前映射以私有迁移清单为准。

## Version handoff / 版本交接

Fix SDK code here, validate and commit it, then update the host's complete Git
revision and lockfile in a separate reviewed change. A source revision must be
available to other developers before a public Git dependency can consume it.
No file synchronization or automatic DLL replacement occurs. Revert the host's
manifest/lock/adapter change together to roll back; do not roll back business data.

SDK 与宿主分别提交：SDK 先修复验证，再由宿主显式升级固定版本并验证。
本地提交不等于推送或发布，不能把不可获取的本地提交写成公共依赖。
包版本、ABI、构建身份、制品 SHA-256 和接口文档各司其职，维护要求见
[AGENTS.md](../AGENTS.md) 和 [CONTRIBUTING.md](../CONTRIBUTING.md)。

## Cleanup / 回收

Stop owned processes, check resolved paths and all reparse points, preserve
uncommitted/unmerged work and required ignored evidence, then verify copies
before deleting old directories. Do not change existing release bytes. Live
build caches may change after reuse; copy-time manifests preserve their original
identity. Never use an alternative command to bypass a rejected deletion.

只清理已确认归属且闲置的产物；另一团队工作树和业务数据不属于垃圾。
本次迁移验证见 [记录](verification/local-consolidation-20261026.md)。
