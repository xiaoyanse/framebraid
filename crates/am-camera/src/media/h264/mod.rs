//! H.264 搬运模式（媒体链 v2 批 1.2/1.3）：
//! 不解码直取 Annex B 访问单元（annexb 内核）→ 会话分发（session：GOP 缓存/
//! per-client 有界队列/源代次）→ WS 协议帧（protocol）；接入编码探测与进程级
//! 缓存（codec_probe：H.264 直连，非 H.264 返回指引，区分超时/认证失败/不支持）。
pub mod annexb;
pub mod codec_probe;
pub mod fanout;
pub mod protocol;
pub mod session;

#[cfg(test)]
mod annexb_tests;
#[cfg(test)]
mod codec_probe_tests;
#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod session_tests;
