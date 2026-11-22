## ADDED Requirements

### Requirement: MJPEG 流源类型

SDK SHALL 支持以 `AM_SRC_MJPEG_URL` 打开 HTTP multipart/x-mixed-replace 流源，boundary 分帧为唯一真相源（Content-Length 不参与分帧，谎报仅计数），JPEG 帧进入既有解码与像素订阅管线。

#### Scenario: 正常流逐帧送达
- **WHEN** 流源打开且服务器按 boundary 分帧推送 JPEG
- **THEN** 帧订阅按既有语义收到解码后像素帧，EXIF/预算/统计与 ISAPI 快照一致

#### Scenario: 恶意流有界防护
- **WHEN** boundary 超 70B、part 头超 64KB 或单帧超 8MB
- **THEN** 丢帧并计数，连续 3 次超限断开连接进入重连退避

### Requirement: 防误切分帧

解析器 SHALL 仅在行首 `\r\n--boundary` 上下文处分帧，并校验 part 的 Content-Type=image/jpeg 与 JPEG SOI/EOI 魔数；载荷内出现 boundary 字节序列不误切。

#### Scenario: 载荷含伪 boundary
- **WHEN** JPEG 载荷字节序列包含 boundary 字符串（非行首上下文）
- **THEN** 不切帧，该载荷完整交付解码

### Requirement: 双超时断流判定

流读取器 SHALL 区分无数据（对端黑洞）与无 boundary 进展（恶意无界流）两种超时，超时后断连重连（复用既有退避策略）。

#### Scenario: 无数据超时
- **WHEN** 连接建立后持续无字节到达超过约定时限
- **THEN** 断开重连，超时原因可观测
