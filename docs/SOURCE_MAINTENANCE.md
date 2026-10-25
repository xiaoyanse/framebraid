# Source maintenance boundary / 后续源码维护边界

FrameBraid contains an exact extracted SDK core. The original host's main branch
already contains the original decoupling branch; ancestry was verified locally.
This check does not authorize changing that host's dependencies.

已通过真实祖先关系核对原宿主 main 包含解耦分支；本轮不改宿主依赖或其开发成果。

FrameBraid is the SDK's public maintenance source;
the host consumes a reviewed, pinned source snapshot or native artifact version.
Core fixes should not be implemented independently in both repositories.

以 FrameBraid 为 SDK 公开维护源，宿主消费审过的固定源码快照或二进制版本，
不在两个仓库分别修同一核心。此依赖迁移尚未实施，也未代表另一团队接手确认。

The host integration owner and version-pinning migration remain a separate
coordination task; publication does not modify the host or install a Git dependency,
submodule or package-registry workflow.

宿主接入负责人及固定版本迁移另行协调；本次开源不修改宿主，也不自动安装 Git 依赖、
子模块或包注册流程。
