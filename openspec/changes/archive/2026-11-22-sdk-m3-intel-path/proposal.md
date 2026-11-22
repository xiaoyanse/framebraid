# Proposal: SDK M3 — Intel 路径与回退矩阵完备

## Summary

无独显机器（Intel 核显/CPU-only）的完整可用路径：DirectML（核显）与 OpenVINO
原生（E1 已交付，CPU 设备）为 Intel 平台主路径；回退矩阵在无 GPU 机器上的完备
验证与补齐；硬解在 Intel 核显上的适配验证。

## Motivation

- 覆盖面：E1/E2 交付了引擎路径，但"Intel 核显机器全链路稳"从未整体验证——
  下游部署存在纯核显工控机场景
- 回退矩阵完备是 M2 验收条目（"无 GPU 机器亦稳"）
- E1 的 OpenVINO CPU 设备 + E2 的 DirectML 组合在 Intel 上的实测留白

## What Changes

- 硬解：Intel 核显 D3D11VA 解码实测（本机 Arc 140T 核显可用）+ 故障注入
- 推理：DirectML（核显）+ OpenVINO 原生 CPU 双路径在 Intel 平台的端到端实测
  与回退矩阵（GPU 路径不可用→CPU 全链）
- 媒体：无 GPU 机器的软解全链验证（已在 R2 覆盖大部分，补集成场景）
- 文档：支持矩阵按实测更新（Intel 核显=已验/未验边界如实）

## Impact

- 以实测验证为主（既有能力组合验证），新增代码预期少（按实测缺口决定）
- 依赖本机 Intel Arc 140T 核显（已确认存在）
- 门禁：Intel 平台端到端（硬解→GPU/CPU 推理→结果）+ 回退矩阵全档实测
