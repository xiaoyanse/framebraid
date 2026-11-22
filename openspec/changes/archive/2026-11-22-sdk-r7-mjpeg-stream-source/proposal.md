# Proposal: SDK R7 — MJPEG over HTTP 流源接入

## Summary

新增 `multipart/x-mixed-replace` MJPEG 流源：HTTP 长连接逐帧解析（boundary 分帧），JPEG 帧进入现有解码/像素订阅管线。覆盖主程序主链路协议（AIeng 主程序当前以自研 Python 实现该协议，接入 SDK 后可整体替换）。

## Motivation

- 主程序主链路（AIeng）以 MJPEG 流为主协议，SDK 缺此源类型则其视频接入无法整体切到 SDK（R7 是其全链替换前置）
- 参考实现协议层薄（boundary 解析纯逻辑 ~300 行），下游已验证协议正确性（恶意流测试、防误切、有界约束）
- 复用度高：JPEG 解码管线（R6 优化后）、凭据/Digest（R2a 缓存）、重连退避、帧订阅/共享池全部现成

## What Changes

- ABI（兼容新增）：源类型枚举 `AM_SRC_MJPEG_URL = 3`（现有 ISAPI 快照/RTSP 不变）
- am-camera：multipart 增量解析器（纯逻辑状态机：preamble→headers→body）+ 流读取器（长连接、无数据/无 boundary 双超时）
- 防恶意流有界约束（对齐下游实战规格）：boundary ≤70B、part 头 ≤64KB、单帧 ≤8MB，连续 3 次超限断连重连
- 防误切：行首 `\r\n--boundary` 上下文 + Content-Type=image/jpeg + SOI/EOI 双校验
- 重连：现有退避策略复用（1s×2^n 封顶 30s）
- 合同：dual-dll-abi.md 源类型章节扩展 + 流为真相源（Content-Length 不参与分帧）语义

## Impact

- 媒体域新增源类型，零触碰推理域；C ABI 兼容新增（枚举新值）
- 门禁等级 L2（新源类型走既有源生命周期管线）；恶意流单测（字节序列直测，参考下游测试向量）
- 主程序收益：其 mjpeg_source.py + multipart_reader.py（~800 行）可由 SDK 源类型整体替换
