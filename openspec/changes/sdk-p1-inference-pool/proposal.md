# Proposal: SDK P1 — 多 Model 编排 API 化（推理池/分片自动路由）

> v0.7 主线首项（2026-11-13 决策人批）。门禁等级：**L3**（公开 API 契约
> 新增+主链路能力——ABI 兼容新增无破坏，但动生成链/绑定/合同全链）。

## Summary

把 B1 已实测的多 Model 编排形态（GPU 帧直传双 Model 批 4=8 路@25fps
97.4%）从"接入方手工编排文档"升级为 SDK 内建 API：**推理池**一次调用
创建 k 个分片 Model 实例（内部 load k 份+逐片攒批配置），提交带 lane
语义自动连续分组路由（lane→shard），关闭一次排空全部。接入方从
"建 k 个 Model+自己分组+逐个配置"变为一处配置。

## Motivation

- **成果兑现最后一步**：B1 的 8 路满额需要接入方手工编排（创建分组
  绑定+分通道批形状选择），文档指导已随 v0.6.4 发布但接入成本仍在；
  API 化=一行配置拿 8 路（"开箱 8 路"）。
- **时机**：AM-RT 在接入窗口期，API 形态可直接对接入指导落地。
- **技术上下文**：多 Model 并行的预算语义（B1 错峰槽/GPU 直传零份额
  旁路）与分片实测（194.8/97.7 分通道）刚完成，设计输入新鲜。

## What Changes（方向；合同细节 design 冻结）

- **ABI 兼容新增**（无破坏，`AM_ABI_VERSION` 次版本演进）：
  - `amv_pool_create(ctx, &pool_desc, &out_pool)`：desc 含模型源（路径/
    内存×档案/raw 同现有 load）、shards 数（k）、攒批配置（批形状/
    wait）、lane 数；内部 load k 个 Model+逐片 configure_batching，
    lane→shard 连续分组绑定（实测形态固化）
  - 提交路由：`amv_pool_submit_*(pool, lane, input, params, &out_id)`
    系（sync/async/GPU 帧入口按现有 infer 家族对齐；lane 为调用方路
    序号，池内路由分片）——或等价最小面（design 冻结时定）
  - `amv_pool_close(pool, timeout, ...)`：逐片排空关闭（复用既有
    model close 语义）
  - `amv_pool_get_stats`：聚合 formed/batched/timeout/rejected 计数
- 生成链全同步（C 头/Rust/Python/C#/常量）；合同先改
  docs/design/dual-dll-abi*.md（含 pool 语义/错误码/生命周期/线程）
- 分通道批形状：desc 显式传（文档指导 GPU 直传批 4/CPU 像素批 2），
  不做自动通道探测（保守，YAGNI）

## 验收前置与门禁（裁定入档，2026-11-13）

- **通道长稳前置（已启动）**：现有手工编排形态 GPU 直传 8 路 ≥8h 长稳
  （锚点 2026-11-13 启动，内存 5min 采样+事件日志监控条款；120s 实测
  194.8 的长窗口背书）——异常即停即报。
- API 完成后**双验收**：编排 API e2e（真装配：池创建→8 路提交→路由
  正确性→逐片身份→关闭排空守恒）+ API 形态长稳窗口（时长随 e2e 结果
  与决策人确认，不低于通道前置口径）。
- L3 要件：关键路径单测+关键集成点真装配+e2e+退出门证据（长稳）+
  ABI 演进（次版本）+生成链/绑定检查+审计交叉。

## Impact

- vision/vision-ffi（池管理+路由）、sdk-abi（新导出+常量）、生成链、
  三语言绑定、文档链（合同/INTEGRATION 多 Model 段升级/README）。
- 风险：ABI 面扩张的合同质量（L3 由合同先行控制）；GPU 直传池形态的
  长稳窗口未知性（前置已在跑）。

## 证据链

- B1 四臂量化+GPU 直传 194.8：`.local/evidence/sdk-b1-20261112/`
- 分通道指导：INTEGRATION 多 Model 段（v0.6.4）
- 8h 长稳（在跑）：`POC/core-performance/results/multilane/v07-gpu-shards2-n8w40-8h.json`
  + `.local/tmp/soak8h-mem.log`
