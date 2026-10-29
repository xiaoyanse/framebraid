//! 摄像机媒体底座：FFmpeg LGPL 动态库 FFI 与 RTSP 拉流（含解码原语）。
pub mod codec;
pub mod encode;
pub mod ffmpeg_lib;
pub mod ffmpeg_sys;
pub mod h264;
pub mod hardware;
pub mod mjpeg;
pub mod multipart;
pub mod mux;
pub mod pixel_ops;
pub mod rtsp;
pub mod sdk_io;
pub mod stream;

#[cfg(test)]
mod rtsp_tests;

#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
