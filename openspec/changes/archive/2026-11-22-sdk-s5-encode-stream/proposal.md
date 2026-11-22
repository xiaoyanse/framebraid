# Proposal: SDK ⑤ — 编码推流（MP4 存档与 RTMP）

## Summary

Media SDK 新增编码输出能力：像素帧/解码帧 → H.264 编码（FFmpeg CPU 起步，NVENC
GPU 为增强项）→ MP4 文件存档与 RTMP 推流两种交付。一次编码器投资三处收益
（⑤/F 抓图编码/未来录制）。

## Motivation

- 主线 3 核心：视频链路下半程（解码→处理→**编码输出**）补全，SDK 从"只进不出"
  变为完整媒体管线
- 下游需求：主程序多路场景需要 MP4 存档（合规留痕）与 RTMP 推流（浏览器预览）
- 现有 am_encoded_packet_t（引用计数）/encoded 订阅管线已铺好消费侧合同锚点

## What Changes

- **编码器**（am-camera media）：FFmpeg avcodec 编码（现有运行库 API 全可用——
  find_encoder/send_frame/receive_packet 已实证）。**首版 CPU（h264，当前运行库
  构建未含编码器，需重建 FFmpeg 运行库加入 libx264 或 mpeg4 内建编码器）**；
  NVENC（h264_nvenc）为 GPU 增强（依赖 M2b 纹理直通，后续批次）
- **MP4 存档**：avformat muxer（mp4/mov）+ avio 文件输出；关键帧间隔/分段时长
  可配；存档文件经现有 am_encoded_packet 语义或新导出
- **RTMP 推流**：avformat rtp/rtmp muxer + avio 网络输出；断线重连复用既有退避
- ABI（兼容新增）：编码器配置结构（codec/码率/GOP/分辨率）+ 存档会话导出
  （open/write/close 走既有生命周期三段式）+ 推流会话导出
- 帧来源：像素帧订阅（CPU 帧编码起步）；GPU 帧→NVENC 直通为后续增强（M2b 纹理）

## Impact

- 媒体域（am-camera/camera-ffi）；推理域零涉及
- 前置：FFmpeg 运行库重建（加编码器组件——现有最小构建只含解码器，configure
  需加 --enable-encoder=libx264 + --enable-libx264 或内建 mpeg4；重走构建指纹链）
- 门禁 L2：编码质量/关键帧/断流恢复实测 + MP4 可播放性验证 + RTMP 对端拉流验证
- 风险：FFmpeg 运行库重建影响面（解码路径回归全测）；RTMP 对端依赖（本地
  mediamtx/nginx-rtmp 测试环境）
