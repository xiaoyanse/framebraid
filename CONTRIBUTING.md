# Contributing / 贡献指南

[English README](README.md) · [中文 README](README.zh-CN.md)

FrameBraid is in local preparation. Issue/PR instructions below apply after publication; they do not imply an existing remote.

当前仅本地准备，Issue/PR 流程在公开后使用，不表示已有远程。

## Scope / 范围

- Read [support boundaries](docs/SUPPORT.md) and applicable ABI/shared-frame contracts.
- Keep media/inference independent of task, alarm, database, PLC and UI business types.
- Preserve other contributors' work; keep changes focused.
- Discuss ABI changes first. Change definitions/generators, not generated headers/layouts by hand.
- No credentials, customer data, runtime binaries, weights or private environments in commits.
- Keep English/Chinese README and quick-start behavior/limits synchronized.

先读支持边界与接口合同，基础能力保持业务无关。保护他人成果、控制范围；ABI 先讨论、生成物不手改。不提交凭据、客户数据、运行库、权重和私有环境，中英文同步。

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

The boundary checker uses offline Cargo metadata. Core fingerprints intentionally reject divergence from the extraction baseline; updating provenance needs a reviewed source-baseline change, not a blind hash refresh.

边界检查需缓存依赖。核心指纹拒绝偏离提取基线，不能为消除失败而随意重算。

Runtime changes need targeted SDK tests and language calls using explicit compatible runtimes, synthetic data and isolated ports. Record actual pass/fail/skip counts. Historical fixed-port RTSP groups and three ignored tests need separate isolation/review, not blind execution against a shared machine. See [current validation scope](docs/RELEASE_STATUS.md).

运行改动须定向测试及语言实调，使用显式依赖、合成数据、隔离端口；保留真实统计。旧固定端口媒体测试及 ignored 不盲跑。

## Reporting and review / 报告及审查

Use the eventual repository issue templates for ordinary problems. Include SDK/ABI version, platform/architecture/backend, minimal reproduction and sanitized errors. Use [SECURITY.md](SECURITY.md) for vulnerabilities.

Changes should explain purpose, compatibility impact, actual validation and documentation updates. Source checks, packaged-binary acceptance and field acceptance remain distinct.

公开后用模板报告版本、平台、后端、复现与脱敏错误。变更说明目的、兼容性、实测和文档；源码、制品及现场验收分开。

## Licensing / 许可

Contribute only materials you are authorized to submit. Unless explicitly agreed otherwise, intentional project contributions use Apache-2.0. Preserve upstream notices and identify new dependency licenses. Contribution does not transfer other people's copyright to the maintainer.

只贡献有权提交的材料，项目贡献按 Apache-2.0，明确另议除外。保留第三方声明并说明新增依赖许可，不将他人版权改为维护者所有。
