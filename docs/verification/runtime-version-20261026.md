# Runtime version fix / 运行时版本修复验证

日期：2026-10-26。评审基线：`c4369d340dec6db8f60440cfbf9dd9c6b322e430`。
本文与修复同提交；所属提交从 Git 历史查，不回填自身提交号。

## Scope / 范围

- 仅修复公共版本查询：Cargo 包版本、实际 target triple、构建时提交/源码输入状态、
  profile 与公共支撑层共享帧编译特性。字符串由 SDK 静态持有；不在运行时调用 Git。
- `source_state` 仅覆盖 Cargo.toml、Cargo.lock、crates/、src/；无自身 Git 或不可核实为 unknown，
  源码包不得借用外层仓库身份。该诊断不是源码/二进制精确哈希，不替代发行清单。
- 保留 C ABI 1.0 的 121 个函数、74 个结构及冻结工件；不新增接口、升级服务或自动更新器。
- 原始 `sdk-source-manifest.json` 原字节保留；独立修订清单登记 1 个原文件修复及
  2 个新增核心构建/测试文件。校验覆盖 161 个文件，未登记漂移仍失败。
- 本地候选，未发布；未改 AM-RT、参考项目或 R1～R5 路线任务的实施状态。

## Checks / 已执行验证

构建目录：仓外 `D:/AM-RT-build/framebraid`。临时测试工程在独立目录生成并由测试自动回收，
不连接相机、不初始化 FFmpeg/ORT、不安装依赖。Windows x64 CPU 工具链，锁定离线构建。

| 验证 | 实际结果 |
| --- | --- |
| `cargo test -p am-sdk-support --locked --offline --features shared-frames` | 15 项支撑层单测 + 2 项版本合同测试通过；无失败/忽略 |
| 同包默认特性测试 | 11 项支撑层单测 + 2 项版本合同测试通过；无失败/忽略 |
| `cargo fmt -p am-sdk-support -- --check` | 通过 |
| 支撑层/两 FFI 包 `cargo clippy --all-targets --locked --offline`，启用两库共享帧，`-D warnings` | 通过 |
| 两 DLL 锁定离线 Release 构建，启用共享帧 | 通过 |
| `test-version-info.py --dll-dir .../release --work-dir ...` | 3 组通过，0 失败/错误；包括两 DLL 实际导出、Cargo 重新构建、无 Git 源码场景 |
| `node --test scripts/sdk/source-fingerprint.test.mjs` | 2 组通过；含错基线、错前摘要、缺文件、重复登记及未登记漂移拒绝 |
| `node scripts/sdk/check-boundary.mjs` | 7 包、0 业务依赖、0 仓外本地依赖，161 个文件指纹通过 |
| C 头与 Python ABI 生成检查、ABI 冻结检查 | 121 函数、74 结构与 8 项冻结工件保持匹配 |
| 文档引用、`git diff --check`、变更范围审查 | 通过；无公开接口/布局变更，不需要重生绑定或改 ABI 主/次版本 |

关键断言：

1. 实际两库均返回 Cargo 当前版本 `0.1.0`，目标 `x86_64-pc-windows-msvc`，ABI 1.0。
2. 预提交候选 notes 返回本记录评审基线提交、`source_state=dirty`、`profile=release`，
   与当时源码状态一致；`support_shared_frames=enabled` 不代替每库实际能力查询。
3. 隔离 Cargo 工程从 `8.7.6` 改为 `8.7.7` 后重建，返回版本相应变化；
   仅提交说明文件推进 HEAD 时亦刷新提交号；改源码不提交时标 dirty。
4. 无 Git 的源码工程即使位于另一个 Git 仓库之下，也返回 unknown 提交/状态。
5. null 参数、仅 struct_size 前缀、非法 size、静态字符串寿命及两库一致性保持原合同。

复验入口：

```powershell
$env:CARGO_TARGET_DIR = 'D:/AM-RT-build/framebraid'
cargo build -p am-camera-ffi -p am-vision-ffi --release --locked --offline --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames
python -X utf8 scripts/sdk/test-version-info.py --dll-dir D:/AM-RT-build/framebraid/release --work-dir D:/AM-RT-build/framebraid-version-tests
```

提交后重建时，notes 应记录新的已提交 HEAD 与 clean 源码状态；脚本按当时实际工作区核对，
不要求等于本文的旧评审基线。后续文档提交不改变已有 DLL 身份，必须重建才改变嵌入信息。

## Review and limits / 审查及限制

- 审查覆盖正确性、输入校验、静态字符串所有权、Git/源码包身份、并发只读、构建失效规则、
  来源清单和兼容性；提交前未发现未解决的致命/警告项。项目未提供 `.Codex/memory/patterns/`。
- 本次是版本诊断定向验证；未重跑视频、真实模型、加速器、跨平台或长期压力验收，
  不改变原有支持范围和未验项目，不把测试用小工程当成其他平台构建通过。
- 发布时仍须审源码版本、文档/绑定、运行库与制品 SHA-256；本地提交不等于上传、tag 或发布。
