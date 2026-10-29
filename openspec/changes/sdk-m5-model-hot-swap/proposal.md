# Proposal: SDK M5 — 动态换模型（运行中热切换）

## Summary

新增 `amv_model_swap` 导出：运行中将槽位的当前模型原子替换为新模型（新源可异构：
路径/内存×档案/raw），在途请求由旧模型完成后排空释放，新请求即时路由新模型。
推理服务不重启、消费方句柄不换、槽位身份（B11）延续。

## Motivation

- 模型迭代部署/分时段切模型是下游实际运维形态（主程序多路场景模型各自升级，
  重启 = 全路断流）
- E1 引擎抽象（ModelEngine）刚落定：换模型对五引擎统一，无 per-engine 分支
- 现状只能 load 新 + close 旧（两步窗口内请求失败/槽位空转），swap 单调用原子化

## What Changes

- ABI（兼容新增）：`amv_model_swap(ctx, slot_key, desc/memory_desc, out_model,
  out_old_model_id)`——按槽位寻址（B11 slot_key；0=全局单槽沿用现有语义），
  返回新模型句柄与被换出模型身份；被换出模型随后走既有 close 排空语义
  （调用方可择机 amv_model_close 释放，或传 AM_SWAP_DETACH 立即排空）
- vision-ffi：槽位注册表的原子替换（写锁内 swap + 旧模型降级为"排空中"）；
  在途请求持 Arc 引用自然续命到完成（现有 executor 语义零改动）
- selection/统计：新模型身份独立（B14 模型身份语义），旧模型入口 selection
  置 legacy_model（既有语义复用）；swap 事件进诊断（换模前后身份/时间）
- 合同：§3.7 模型换代章节扩展（现有"模型换代"段已有 legacy_model 语义锚点）

## Impact

- vision 域（vision-ffi 为主，am-vision 接口零改动）；五引擎统一受益（E1 抽象）
- 门禁 L2（新导出走既有生命周期管线）：原子性单测 + 真实模型热切换实测
  （换模瞬间并发请求不失败：旧模型在途完成、新请求走新模型）
- 风险：槽位寻址与既有 load 的槽注册交互（B11 语义复核）
