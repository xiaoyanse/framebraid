# Release checklist / 发布前清单

[Documentation / 文档入口](README.md) · [Current local status / 当前本地状态](RELEASE_STATUS.md)

**v0.1.0 publication authorized by the owner on 2026-10-26; v0.2.0 authorized on 2026-10-28; v0.2.1 on 2026-10-29.**

**锚点双层哈希（annotated tag，下游对接必读）**：发布回执须同时报 **tag 对象哈希**（ls-remote 直接可见）与**底层 commit**（tag 解引用所得）。下游 Cargo git 依赖两种写法皆可：`tag = "v0.2.1"`（随 tag 引用，语义等价）或 `rev = 底层 commit 全哈希`（下游门禁锁定场景，tag 是可移动引用与锁定语义冲突时用）。示例 v0.2.1：tag 对象 `4cfb2afd`，底层 commit `656c8ac8`。 Each authorization covers only the identified build and scoped validation, not future versions or platforms.

**负责人已于 2026-10-26 授权发布 v0.1.0、2026-10-28 授权发布 v0.2.0。**每版授权仅覆盖具名构建与限定验证；未来版本须重新核对。

## 消费方画像 / Consumer profile（发版前必答）

发布候选确认前逐条作答，答案进发布说明；画像变化须复核基线/场景档位（豁免类别 profile-change）。

1. **多路形态**：几路并发接入？各自目标频率？
2. **主链路协议**：ISAPI 抓图 / RTSP H.264+H.265 / MJPEG 流，主消费路径是哪条？
3. **分辨率档**：实际接入档位（含最大档，4K 档须过预算边界场景）？
4. **认证方式**：HTTP Digest / 明文凭据 / 无认证？

v0.2.1 实例：1-4 路并发；ISAPI HTTP 抓图 + RTSP H.264/H.265 + MJPEG 流；720p-2160p（4K 档预算边界已验证）；HTTP Digest（合成凭据本地验证）。

## 基线与场景要件 / Baseline & scenario gate（发版前必跑）

- [ ] 基线三件（`scripts/sdk/baselines/`）：`baseline_decode.py`（解码配对）、`baseline_capture_cadence.py`（节拍精度）、`baseline_lanes.py`（多路线性度）带 `--baseline recorded.json` 全过
- [ ] 场景五件（`scripts/sdk/scenarios/`）：`scenario_budget_4k.py`、`scenario_cadence.py`、`scenario_mjpeg_stream.py`、`scenario_fault_observability.py`、`scenario_binding_load.py` 全过
- [ ] 命令样例：`python scripts/sdk/baselines/baseline_decode.py --dll-dir <release> --runtime-dir <ffmpeg bin> --baseline scripts/sdk/baselines/recorded.json`
- [ ] 绑定参数覆盖：`node scripts/sdk/check-bindings.mjs --check`（2026-11-03 新增）
- [ ] 文档符号覆盖：`node scripts/sdk/check-doc-coverage.mjs --check`（2026-11-03 新增）
- [ ] 零值语义覆盖：`node scripts/sdk/check-zero-semantics.mjs --check`（2026-11-03 新增）
- [ ] ABI 冻结基线：`node scripts/sdk/freeze-abi.mjs --check` 过（漂移=发布阻断，须先按授权变更重算基线并登记原因；2026-11-07 补——v0.6.0 发版漏检教训：10-27 基线后 ABI 1.3/档案演进未随批重算，漂移积压到批 D 才发现）
- [ ] 漂移阻断：基线 FAIL=发布阻断；豁免仅限验证制度三类并按登记格式执行（见 VERIFICATION_DOCTRINE.md）
- [ ] 运行结果（数字）进当版验证记录，输入档位与消费方画像对齐

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

## Publication actions / 发布动作（推送获批后按序执行，缺一即发布不完整）

- [ ] **Git 层**：annotated tag（`git tag -a vX.Y.Z -m ...`）+ `git push origin main --tags`——按当次授权，推送通常由用户亲手执行
- [ ] **GitHub Release 页**（git push 只同步标签对象，不建发布条目，必须单独创建）：`gh release create vX.Y.Z --title ... --notes-file ...`，正文必须含**双哈希锚点**（tag 对象 + 底层 commit，见本页锚点段）与升级须知
- [ ] **资产上传**：按下方固定清单上传全部 Release 附件，上传后 `gh release view` 核对文件名与字节数
- [ ] **回执**：向决策通道报 tag/commit 双哈希 + Release URL；`docs/RELEASE_STATUS.md` 首行版本与链接同步
- [ ] **下游联动通知**：向固定引用方（vl-SDK）发送 tag 双哈希锚点通知（按供应链冻结纪律对称约定）

> 教训登记（2026-11-02，v0.3.1）：推送完成后 Release 页未建即回执"发布完成"，缺口由用户问询发现——根因是本清单此前无此段，发布动作靠会话临场发挥。

### Artifact set / 附件形态固定清单（每版逐项勾选）

v0.3.2 起最低形态：

- [ ] `am_camera.dll` + `am_vision.dll`（裸 DLL，直取即用）
- [ ] `FrameBraid-SDK-windows-x64-<ver>.zip`：`sdk/` 目录包含双 DLL、`include/`、`bindings/`、`INTEGRATION.md`、`RELEASE-NOTES.md`、`SHA256SUMS.txt`、`build-record.json` 和 `LICENSE`。
- [ ] `FrameBraid-SDK-source-<ver>.zip`（git archive 全仓源码）
- [ ] `SHA256SUMS.txt`（裸 DLL 哈希）
- [ ] `build-record.json`（独立附件，与 zip 内同源）

附件形态不得低于上一正式版；缺项须当版登记原因，不得静默缩水。zip 打包前必须过排除检查（下节），上传前必须核对附件字节大小与本地一致。

### Package hygiene / 打包排除检查（发布前必跑）

- 禁入公开包：`__pycache__/`、`*.pyc`、`.exp`/`.lib`（导入库另有交付渠道时）、`.local/`、`.omc/`、IDE/临时文件。
- 必入项核对：`build-record.json` 必须在 windows zip 内且含 `sourceCommit`、`binaries`、`buildInputs`；上传前重算 `BUNDLE-SUMS`。
- 检查示例（无输出即通过此项）：`unzip -l <zip> | grep -Ei 'pycache|\.pyc'`；其余禁入项亦须逐项检查。

> 教训登记（2026-11-03，v0.3.2）：首版 windows zip 混入 3 个 .pyc 且缺 build-record.json，发布前核查发现后清污重打包——本段即该教训的制度化。

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
