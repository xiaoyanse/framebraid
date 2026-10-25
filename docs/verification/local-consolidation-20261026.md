# Local consolidation verification / 本地目录收口验证

Date: 2026-10-26. Review base: `fc293013d1dabf31badf3600c888fb3403eddab0`.
Scope: local path defaults, output guards, preservation of prior packages/evidence,
and documentation of host source consumption. No core/ABI changes or publication.

本轮构建、测试、候选包、发布包、运行库和证据统一在本仓忽略的 `.local` 分类保存；
AM-RT 按完整 Git 提交消费已发布 v0.1.0，保留业务适配，不再保存另一份可编辑核心。
规则见 [目录说明](../LOCAL_LAYOUT.md) 与 [源码维护](../SOURCE_MAINTENANCE.md)。

## Checks / 验证

| Check / 检查 | Result / 结果 |
| --- | --- |
| Independent DLL build | `cargo build -p am-camera-ffi -p am-vision-ffi --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames --locked --offline` passed under `.local/build/current` |
| Runtime identity | `test-version-info.py` 3 passed: real two-DLL version exports; version/HEAD/dirty rebuild tracking; source archive reports unknown Git identity |
| Output paths | `test-local-paths.py` 1 group and `package-paths.test.mjs` 1 group passed: category roots/outside/relative paths and nonempty output rejected; private fixtures cleaned |
| Source boundary | 7 SDK packages, 161 checked source files, 0 business dependencies, 0 external local dependency paths |
| ABI freeze | 8 frozen artifacts unchanged; no ABI version bump or regenerated freeze |
| Host integration | AM-RT fixed full revision `5ee707153796cf1d8f9dd7118c664bacf757f97b`, server check/clippy, camera adapter 8 pass/1 real-device skip, OpenAPI/type/frontend build and hot reload regression passed |
| Existing material | Seven old D roots copied with per-file SHA-256 verification; six delivery/archive groups rehashed successfully before user cleanup handoff |

The Debug verification binaries were built at the review base above with clean
SDK inputs, shared frames enabled, and on-demand runtimes. Their recorded package
version is `0.1.0`; this is not a new public release. Package checksums remain in
private evidence; original published bytes and manifests were not changed.

上述 Debug 验证只覆盖本次目录及身份范围，不替代新 Release、加速后端、设备或持续运行验收。
未修改公共函数、结构、内存合同、源码核心指纹或已发布制品。

## Preservation and cleanup / 保存与回收

Private copy-time manifests and final category relocations are in
`.local/evidence/consolidation-20261026`. Prior releases are under
`.local/releases/published`, candidates under `.local/releases/candidates`, and
historical/private configuration under `.local/archive`. These are ignored and
excluded from source/public packages. Local archive is not an external backup.

复制时逐文件验证；可重建缓存复用后会变化，不要求其持续保持旧字节。
FFmpeg 来源树链接只解除链接本体，目标 10951 文件前后摘要不变。
七个旧 D 目录删除被自动审批拒绝（`blocked by policy`），没有换方式重试；
已向用户提供可手动删除的明确清单，尚未记录为已删除。其他团队工作树未动。

This maintenance is committed locally only. No push, tag, upload or release.
See Git history for the document's owning commit; no self-referential commit hash.

An unrelated local `.gitignore` edit was detected during closeout and preserved
outside this commit. 本轮不代交外部忽略规则修改。
