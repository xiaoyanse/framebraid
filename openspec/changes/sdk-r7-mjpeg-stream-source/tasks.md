# Tasks

- [ ] 7.1 合同：dual-dll-abi.md 源类型章节新增 AM_SRC_MJPEG_URL（流语义：boundary 分帧为真相源、双超时、有界约束、重连退避）
- [ ] 7.2 生成器同步（枚举新值）+ C 头/Rust ABI/Python/C# 绑定 --check 绿
- [ ] 7.3 multipart 增量解析器（纯逻辑状态机）：boundary 提取/防误切/双校验/有界约束，恶意流字节序列单测（含边界序列、超限、伪装 boundary）
- [ ] 7.4 流读取器：HTTP 长连接 + Digest 复用 + 双超时（无数据/无 boundary 进展）+ 连续超限断连重连（退避复用）
- [ ] 7.5 源类型接入现有生命周期（open/subscribe/close 走既有源管线；JPEG 帧进 R6 优化后的解码路径）
- [ ] 7.6 端到端实测：本地合成 MJPEG 服务器（脚本生成流含正常/恶意/超时场景）+ 帧送达/重连/预算断言
- [ ] 7.7 验证记录 + fmt/clippy/workspace 回归 + 边界检查 + 本地提交
