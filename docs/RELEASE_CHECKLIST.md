# Release checklist / 发布前清单

[Documentation / 文档入口](README.md) · [Current local status / 当前本地状态](RELEASE_STATUS.md)

**v0.1.0 publication authorized by the owner on 2026-10-26; v0.2.0 authorized on 2026-10-28; v0.2.1 on 2026-10-29.**

**锚点双层哈希（annotated tag，下游对接必读）**：发布回执须同时报 **tag 对象哈希**（ls-remote 直接可见）与**底层 commit**（tag^{commit}）；下游 Cargo git 依赖写 tag 名（）而非任何裸哈希。示例 v0.2.1：tag 对象 ，底层 commit 。 Each authorization covers only the identified build and scoped validation, not future versions or platforms.

**负责人已于 2026-10-26 授权发布 v0.1.0、2026-10-28 授权发布 v0.2.0。**每版授权仅覆盖具名构建与限定验证；未来版本须重新核对。

## Completed locally / 本地已完成

- [x] One FrameBraid repository containing Media SDK and Vision SDK; no host business dependency.
- [x] GitHub account `xiaoyanse` verified; attribution excludes private login email.
- [x] Apache-2.0 originals, upstream notices, bilingual guides, support limits, contribution/security guidance and issue/PR templates.
- [x] Extraction provenance retained; original decoupling commit is in host main ancestry.
- [x] Current reachable history screened for credentials/private details; synthetic credential fixtures reviewed.
- [x] Windows x64 Release built from recorded FrameBraid source with toolchain/features/input fingerprints.
- [x] Packaged DLLs verified through 11 groups, including C/C++/C#/Python, missing dependencies, resource rejection, reload and shared-frame ownership.
- [x] Final artifacts inventoried and hashed; relative links checked; isolated source archive build checked.
- [x] Exact `minimal-20261026` runtimes paired with FFmpeg source/build archives, toolchain notices and ORT LICENSE/ThirdPartyNotices.
- [x] Cargo.lock-based Windows dependency inventory and original notices collected; conservative build-dependency superset identified.
- [x] Matching FFmpeg recipe archive physically included in Windows candidate, also available as a separate release asset.
- [x] Public candidate excludes environment files, customer data, private logs and weights; internal evidence stored separately.

详见当前状态：Release 单测 177 通过，11 组 DLL 验证通过。未复跑五类真实权重的
端到端测试；不冒充 GPU、硬解、跨平台、现场或长跑验收。旧 BtbN 档案仍不用于分发。

- [x] Process document scope confirmed and superseded records archived; generator/ABI/license/provenance inputs preserved.

## Publication authorization and scope / 发布授权与范围

- [x] FrameBraid is the public SDK maintenance source; host version-pinning migration is separate work.
- [x] Owner authorized public source/assets and matching Windows runtime materials.
- [x] Selected a clean initial public snapshot and v0.1.0 developer preview; release manifests bind the exact artifacts.
- [x] GitHub private vulnerability reporting enabled and verified.
- [x] Explicit authorization covers repository creation, push, tag and this release; future releases require new authorization.

源码与匹配运行库已获本次分发授权，安全反馈入口已配置。发布前核制品、指纹与源码对应，
不以历史测试扩大当前验收；未来版本按改动半径复验。

## Intended first delivery / 拟首发内容

Source snapshot, C headers, C#/Python bindings, C/C++/C#/Python examples, bilingual
guides and a separate Windows x64 Release package with matching dependency materials.
No weights, Java wrapper, mobile binaries or universal accelerator promise.

首次交付源码、接口/封装/示例、双语指南及单独 Windows x64 Release 包；不包含模型权重，
不承诺 Java、移动端或通用加速适配。版本为 v0.1.0 开发者预览版，日期 2026-10-26。
