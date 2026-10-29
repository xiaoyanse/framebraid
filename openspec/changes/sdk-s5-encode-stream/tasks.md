# Tasks

- [x] S5.0 FFmpeg 运行库重建完成：GPL profile（libx264+aac 编码器、mp4/flv/rtmp muxer 实测可解析）、libx264-165.dll+GPL许可文本入闭包、解码回归全绿（69+1 项，含真实H.264硬解）
- [x] S5.1 合同完成：§2.10+am_encoder_config_t+10导出三语言同步（85布局测试绿）
- [x] S5.2 编码器：libx264+swscale RGBA→YUV420P、zerolatency 确定性出包、三段式四导出+receive（未决1补齐）
- [x] S5.3 MP4 存档：实测 12帧→38KB可播放MP4（I:2+P:10 关键帧间隔=gop）、分段滚动三段均有效
- [x] S5.4 RTMP：StreamSession 断线重连（1s×2^n封顶30s×5次）+三导出；真机对端留S5.6
- [x] S5.5 单测：合成（预算界/错误映射/分段模板/退避曲线）+真实4项全过（含fps分数帧率界放宽）
- [ ] S5.6 端到端实测：合成帧→编码→MP4 落盘可播放（ffprobe 验证）+ RTMP 本地对端拉流验证 + 解码路径回归
- [ ] S5.7 验证记录 + fmt/clippy/workspace 回归 + 边界检查 + 本地提交
