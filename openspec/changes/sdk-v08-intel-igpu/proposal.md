# Proposal: sdk-v08-intel-igpu（v0.8 Intel 核显放开主线）

## Why

决策人 2026-11-15 批串行链 v0.7.2（已完成）→**v0.8 Intel 核显主线**→v0.8.x RTMP。
历史基础：E1 OpenVINO 原生引擎（CPU 设备）+E2 DirectML 后端+M3 Intel 核显整体验证
均已完成，但 M3 留三缺口即本 change 的"Why"：
1. **公共 API 无适配器/设备定向**——默认策略不命中核显（枚举序首个=独显），下游
   无法指定"用核显"
2. **DirectML 运行库缺失**（盘上无 DML EP，环境待获取）——核显 DML 路径未实测
3. **设备定向能力未在合同暴露**（引擎能力在，C ABI 无面）

## What Changes

- **设备/适配器定向合同**（ABI 1.6 兼容新增预估）：推理后端策略的设备定向（按
  索引/LUID/描述匹配）+硬解适配器选择（camera 侧 M3.1 同款缺口）——先合同设计
  （dual-dll-abi*.md）再生成链，禁手改生成物
- **默认策略修正**：无独显/纯核显机器上枚举与选择命中核显（当前取首个的缺陷）
- **DirectML 运行库获取与落位**（.local/runtimes DML EP 包+来源登记）+核显 DML
  实测
- **OpenVINO GPU 设备评估**（核显的 OV GPU 插件在 runtime 已含——评估是否纳入
  v0.8 范围或登记后续，按实测定）
- **纯核显端到端验收**：本机 Arc 140T 核显形态（硬解+推理+多路）全链路稳定+回退
  矩阵（核显不可用→CPU）

## Impact

- crates/sdk-abi+生成链（ABI 1.6 契约）、crates/vision（引擎选择/枚举）、
  crates/camera（适配器选择）、docs/design 合同文档、运行库包管理
- 门禁 L3（公共 API 新增+跨模块），下游兼容（vl pin v0.6.3 不受影响）
- 不承诺：核显上的绝对性能数字（实测如实登记，与独显口径分列）
