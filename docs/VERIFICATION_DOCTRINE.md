# 验证制度：基线门禁 · 场景库 · 消费方画像（Verification Doctrine）

[Documentation / 文档入口](README.md) · [发布清单](RELEASE_CHECKLIST.md) · [路线图](ROADMAP-PLAN.md)

本文定义 FrameBraid 的三件验证套件执行口径：何时跑、怎么判、如何豁免。
三件套回答同一个问题：SDK 对消费方是否"值得用"——不只验证"做得对"（功能正确），
还要验证"相对消费方替代方案与真实场景仍然站得住"。

## 三件套构成

| 组件 | 位置 | 作用 |
|---|---|---|
| 基线门禁 | `scripts/sdk/baselines/` | 量化守门：解码/节拍/多路吞吐相对 recorded.json 首版 |
| 场景库 | `scripts/sdk/scenarios/` | 防回归：已修 R 项各沉淀一个可独立运行的防回归场景 |
| 消费方画像 | RELEASE_CHECKLIST「消费方画像」节 | 发版前四问过堂，画像决定哪些基线/场景必跑 |

## 何时跑

- **发版前（强制）**：RELEASE_CHECKLIST「基线与场景要件」全跑，漂移即阻断
- **动主链路后（强制）**：captures/decoder/fanout/hub/continuous 路径的代码变更，受影响场景必跑；基线建议跑
- **日常开发（自愿）**：单脚本退出码即判据，可独立触发
- 基线与场景均为合成环境可跑（本地 HTTP/multipart 替身，不连现场设备，遵守项目测试红线）

## 怎么判

- **基线**：退出码=判据。`--baseline scripts/sdk/baselines/recorded.json` 时输入
  sha256 先行（同输入铁律，sha256 不一致=FAIL），再断言 P50 漂移 ≤ max(记录值×容差%，绝对下限)
- **场景**：每脚本独立可跑、退出码判据、失败输出 `{"ok":false,"stage":...,"error":...}` 诊断行
- **场景对齐合同**：场景断言 ABI 合同承诺的可观测行为，不把实现细节当判据。
  实例：图片通道故障映射 network、generation/attempts 不递增（与视频通道存在
  跨通道差异），场景如实记录观测值；差异是否收敛由维护者裁决，不静默偏袒任一侧

## 基线漂移豁免三类

基线 FAIL = 发布阻断，除非漂移属于下列三类之一并完成登记：

1. **known-optimization**：已知优化带来的真实改进或取舍。须附优化对应的验证记录，
   并用新数字重录 recorded.json；旧值在验证记录中保留，不冒充新结果
2. **environment-change**：构建平台/运行库版本/硬件变化。须在新环境重录首版，
   并在 RELEASE_STATUS.md 注明环境
3. **profile-change**：消费方画像变化导致输入档位变化（如新增分辨率档）。
   须同步更新画像与 recorded.json 输入档位并重录

不属于三类的漂移按回归处理，先修后发。

## 豁免登记格式

1. `recorded.json` 的 `exemptions[]` 追加：`date`（YYYY-MM-DD）、`kind`（known-optimization / environment / profile）、`baseline`（decode / cadence / lanes）、`cell`（受影响档位或 all）、`reason`（一句话原因 + 对应验证记录路径）
2. `docs/RELEASE_STATUS.md` 同日期增一行同原因记录
3. RELEASE_CHECKLIST 审查时按本节核对；未登记豁免的漂移不放过

## 维护规则

- recorded.json 首版以 2026-10-30 本地实测填入（证据：`.local/tests/v1-baselines/`）
- 旧值只追加不改写；重录须附原因
- 基线/场景判据变更视同合同变更，须同步本文与 CHANGELOG（Unreleased）
