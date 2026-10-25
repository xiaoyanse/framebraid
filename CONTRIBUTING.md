# Contributing / 贡献指南

[English README](README.md) · [中文 README](README.zh-CN.md)

Use the repository issue/PR templates and the [current release status](docs/RELEASE_STATUS.md).
A contribution does not authorize a release or expand the verified support scope.

请使用仓库 Issue/PR 模板并核对当前发布状态；贡献提交不等于发布授权，也不扩大已验证范围。

## Scope / 范围

- Read [support boundaries](docs/SUPPORT.md) and applicable ABI/shared-frame contracts.
- Keep media/inference independent of task, alarm, database, PLC and UI business types.
- Preserve other contributors' work; keep changes focused.
- Discuss ABI changes first. Change definitions/generators, not generated headers/layouts by hand.
- No credentials, customer data, runtime binaries, weights or private environments in commits.
- Keep English/Chinese README and quick-start behavior/limits synchronized.

先读支持边界与接口合同，基础能力保持业务无关。保护他人成果、控制范围；ABI 先讨论、生成物不手改。不提交凭据、客户数据、运行库、权重和私有环境，中英文同步。

## Versions and interface synchronization / 版本与接口同步

Classify each change before implementation. Keep SDK package versions, C ABI versions,
source commits and artifact checksums distinct. Record development changes under
`CHANGELOG.md` → Unreleased; assign the release version when preparing a candidate,
not on every commit. Compatible additions and breaking changes must follow the
[ABI evolution contract](docs/design/dual-dll-abi.md#14-结构-sizeversion-演进与布局策略b09).
A preview release is not permission to silently change existing contracts.

实施前先分类：内部优化、兼容新增、不兼容变更。SDK 包版本、C ABI 版本、源码提交和制品
校验和分别管理；开发期写 Unreleased，候选发布时统一定版，不逐提交升版。
内部优化不改变公开合同则无需升级 ABI；兼容新功能发布时更新 SDK 版本，C ABI 是否变化
独立判断；破坏 C ABI 签名、布局、枚举值、所有权或既有语义须升级 ABI 主版本并给出迁移说明。
Rust API/语言封装的源码兼容性也要单独核对，不能仅凭 C ABI 不变认定兼容。
Check Rust API and wrapper source compatibility separately; an unchanged C ABI is not sufficient.

| Change / 变更 | Required synchronization / 必须核对并同步 |
| --- | --- |
| Public function, structure, constant or behavior / 公开函数、结构、常量或行为 | Applicable `docs/design/dual-dll-abi*.md`; parameters/defaults, results/errors, ownership, callback threads, queues and recovery; generator inputs plus affected C headers, Rust ABI and Python/C# bindings / 对应合同、语义、生成源和受影响的各语言产物 |
| Feature, backend, input format or model profile / 能力、后端、输入格式或模型档案 | Capability reporting, applicable delivery/shared-frame/profile contracts, `docs/SUPPORT.md`, representative validation / 能力查询、对应设计/档案、支持边界与代表性验证 |
| User-facing workflow or configuration / 调用流程或配置 | English/Chinese README and quick starts, `sdk/INTEGRATION.md`, runnable examples and dependency/error guidance / 中英文入口、接入、示例与依赖/错误说明 |
| Any functional change / 所有功能变更 | `CHANGELOG.md`; affected roadmap state and `docs/RELEASE_STATUS.md`, keeping plans, implementation and actual acceptance separate / 变更记录及受影响的路线、验证状态 |
| Release candidate / 发布候选 | Matching SDK/ABI versions, source commit, freeze digest, platform/features/dependencies, bindings/docs, artifact checksums, `sdk/RELEASE-NOTES.md` and release checklist; compatibility and migration notes / 同一候选的版本、源码、合同、制品及迁移关系 |

Update only affected documents; explain why a listed item is not applicable rather than
rewriting every document. Generate affected headers/bindings from controlled definitions,
then run the corresponding generator checks, ABI/layout compatibility checks and targeted
language calls. Never hand-edit generated files or refresh hashes merely to silence failures.

只更新受影响内容，不适用项在变更说明中注明理由。先改合同/定义，再生成绑定并核对实际
能力；按破坏范围检查生成物、ABI/布局与定向语言调用。不手改生成物，不以重算指纹掩盖漂移。
性能优化还须遵守[路线图量化要求](docs/ROADMAP.md#4-性能量化合同实施时必须执行)。

### Identity and downstream upgrades / 指纹与下游升级

- The source manifest preserves extraction provenance; ABI freeze digests detect contract
  drift; release manifests/checksums identify artifacts. A hash difference is not a
  compatibility decision. Preserve historical records when reviewing a new baseline.
- Consumers pin a release/commit and matching headers/bindings/docs. Use ABI negotiation,
  `struct_size`, capability queries and runtime dependency checks to choose a supported
  path or report an explicit incompatibility. Do not read rolling `main` as an old release's contract.
- These rules do not provide an automatic updater, online contract synchronization or
  automatic rewriting of consumer code. Missing new symbols must be handled before use;
  replacing only a DLL is permitted only after the documented compatibility checks.

源码指纹说明来源，ABI 摘要防合同漂移，制品清单/校验和识别具体包；三者不能混为兼容判定。
下游固定版本并携带匹配的头文件、封装和文档，依协商与能力检查决定使用新能力、回退或拒绝。
文档随同版本同步不等于宿主代码自动适配，也不默认联网升级或仅替换 DLL。

Runtime version queries now read the Cargo package version and actual target triple.
Existing notes expose build-time source commit, source-input state, profile and the shared support layer's shared-frame feature. Archives without their own Git repository report unknown source identity, never a
parent repository's commit. These fields do not replace artifact SHA-256; see
[version diagnostics](sdk/INTEGRATION.md#21-运行时版本与构建身份).

运行时版本随 Cargo 包版本生成，并报告实际构建目标与构建时源码身份；无 Git 时明确未知。
诊断信息不是二进制哈希，发布前仍须核对实际载入库与同版本制品清单。

## Checks / 检查

From the root, after obtaining locked dependencies:

```powershell
$env:CARGO_TARGET_DIR = 'D:/FrameBraid-build'
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames -- -D warnings
node scripts/sdk/check-boundary.mjs
node scripts/sdk/generate-abi.mjs --check
node scripts/sdk/generate-python-abi.mjs --check
node scripts/sdk/freeze-abi.mjs --check
node --test scripts/sdk/source-fingerprint.test.mjs
```

The boundary checker uses offline Cargo metadata. Core fingerprints reject unregistered divergence. Authorized source fixes use `sdk-source-revisions.json`, bound to the unchanged extraction manifest SHA-256, review-base commit, previous/current file hashes and reasons. Register new core files explicitly; never replace historical extraction hashes to silence failures.

边界检查需缓存依赖。核心指纹拒绝未登记漂移；授权修复使用独立修订清单记录变更前后摘要及原因，原始提取清单不覆盖。

Runtime changes need targeted SDK tests and language calls using explicit compatible runtimes, synthetic data and isolated ports. Record actual pass/fail/skip counts. Historical fixed-port RTSP groups and three ignored tests need separate isolation/review, not blind execution against a shared machine. See [current validation scope](docs/RELEASE_STATUS.md).

运行改动须定向测试及语言实调，使用显式依赖、合成数据、隔离端口；保留真实统计。旧固定端口媒体测试及 ignored 不盲跑。

## Reporting and review / 报告及审查

Use the repository issue templates for ordinary problems. Include SDK/ABI version, platform/architecture/backend, minimal reproduction and sanitized errors. Use [SECURITY.md](SECURITY.md) for vulnerabilities.

Changes should explain purpose, compatibility impact, actual validation and documentation updates. Source checks, packaged-binary acceptance and field acceptance remain distinct.

用仓库模板报告版本、平台、后端、复现与脱敏错误。变更说明目的、兼容性、实测和文档；源码、制品及现场验收分开。

## Licensing / 许可

Contribute only materials you are authorized to submit. Unless explicitly agreed otherwise, intentional project contributions use Apache-2.0. Preserve upstream notices and identify new dependency licenses. Contribution does not transfer other people's copyright to the maintainer.

只贡献有权提交的材料，项目贡献按 Apache-2.0，明确另议除外。保留第三方声明并说明新增依赖许可，不将他人版权改为维护者所有。
