# Changelog / 变更记录

## Unreleased / 未发布

- Consolidated local build/test/release/evidence paths under ignored `.local` categories; documented AM-RT pinned-source consumption and upgrade/rollback boundaries. SDK core/ABI and published artifacts unchanged.
- 本地目录分类与宿主固定版本接入已整理；构建/打包工具拒绝分类外输出，不改变公开 SDK 接口。

- Fixed runtime version diagnostics: Cargo package version, actual target, source commit/state,
  build profile and shared-frame feature, without changing C ABI 1.0 layouts or exports.
- Retained original extraction provenance and added an explicit source-revision overlay.

修复运行时固定开发版本文本，新增构建身份回归验证；仅本地候选，未替换 v0.1.0 发布制品。

- Added a staged optimization roadmap distinguishing existing capabilities, improvements
  and optional extensions, with soak and fault-injection acceptance criteria.
- Documentation only; SDK code, ABI, v0.1.0 artifacts and support commitments unchanged.

新增分阶段优化路线与持续运行/故障注入验收要求，仅整理文档，尚未开始实施。

## 0.1.0 — 2026-10-26 / 开发者预览版

Initial public developer preview: source SDK and Windows x64 CPU binaries.

首次公开开发者预览版，提供源码及 Windows x64 CPU 二进制。

### SDK extraction / SDK 提取

- Seven-crate independent Rust workspace for Media SDK and Vision SDK.
- Apache-2.0 original code and preserved third-party notices.
- Existing DLL names and C ABI symbols retained.
- Local extraction tests recorded separately from Release acceptance.

独立七 crate，保留兼容接口和第三方许可，提取验证与 Release 验收分开。

### Open-source preparation / 开源准备

- Verified account/attribution: `xiaoyanse`.
- English/Chinese README and quick-start, support boundaries and documentation entry.
- Contribution/security guidance, issue/PR templates and future release checklist.
- Corrected current source-layout instructions.
- Added user-selected green FB logo/banner and reproducible charts from scoped historical observations, with separate extraction/Release labels.
- Added a locally validated Windows Release candidate and matching dependency materials; see docs/RELEASE_STATUS.md.
- Archived superseded preparation/extraction reports and logo exploration assets outside the public tree; retained generator inputs, ABI history and provenance.
- GitHub repository, v0.1.0 release and private vulnerability reporting.

首发包含双语说明、绿色 FB 视觉素材、匹配依赖材料及受限验证结果；历史过程资料内部归档。
