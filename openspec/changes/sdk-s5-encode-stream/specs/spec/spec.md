## ADDED Requirements

### Requirement: 帧编码输出

SDK SHALL 提供像素帧→H.264 编码能力（码率/GOP/分辨率可配），编码包语义与既有 am_encoded_packet_t 一致（引用计数/retain-release）。

#### Scenario: 编码产出
- **WHEN** 像素帧序列送入编码会话
- **THEN** 产出 H.264 编码包（关键帧间隔符合配置），会话关闭排空缓冲包

### Requirement: MP4 存档

SDK SHALL 提供 MP4 文件存档会话（open→write→close 三段式），分段时长与关键帧可配；产出文件可被标准播放器/ffprobe 解析。

#### Scenario: 存档可播放
- **WHEN** 编码会话写入 N 帧后正常关闭
- **THEN** MP4 文件可 ffprobe 解析且流参数与配置一致

### Requirement: RTMP 推流

SDK SHALL 提供 RTMP 推流会话（断线按既有退避策略重连），推流状态与故障可观测（复用源状态通道语义）。

#### Scenario: 推流与重连
- **WHEN** 推流中网络断开再恢复
- **THEN** 按退避策略重连后续推，故障期间状态可观测且缓冲策略有界
