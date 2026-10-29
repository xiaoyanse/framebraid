# Tasks

- [x] S5.0 FFmpeg 运行库重建完成：GPL profile（libx264+aac 编码器、mp4/flv/rtmp muxer 实测可解析）、libx264-165.dll+GPL许可文本入闭包、解码回归全绿（69+1 项，含真实H.264硬解）
- [x] S5.1 合同完成：§2.10+am_encoder_config_t+10导出三语言同步（85布局测试绿）
- [ ] S5.2 编码器实现：像素帧→H.264（avcodec CPU），码率/GOP/分辨率配置，编码包产出对齐 am_encoded_packet 语义
- [ ] S5.3 MP4 存档：avformat mp4 muxer + 分段/关键帧可配 + 文件生命周期（三段式）
- [ ] S5.4 RTMP 推流：avformat rtmp muxer + 断线重连（退避复用）+ 推流会话生命周期
- [ ] S5.5 单测：配置校验/会话生命周期/错误映射（合成数据）
- [ ] S5.6 端到端实测：合成帧→编码→MP4 落盘可播放（ffprobe 验证）+ RTMP 本地对端拉流验证 + 解码路径回归
- [ ] S5.7 验证记录 + fmt/clippy/workspace 回归 + 边界检查 + 本地提交
