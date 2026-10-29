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
