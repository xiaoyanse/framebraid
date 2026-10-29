//! 推流会话：FLV/RTMP 连接与断线重连（退避 1s×2^n 封顶 30s，上限 5 次）。
use super::encode::EncoderSettings;
use super::ffmpeg_lib::FfmpegApi;
use super::mux::{backoff_delay_ms, MuxError, MuxFormat, Muxer, STREAM_MAX_RETRIES};
use std::ffi::{CStr, CString};
use std::sync::Arc;

pub struct StreamSession {
    api: Arc<FfmpegApi>,
    settings: EncoderSettings,
    url: CString,
    mux: Muxer,
    attempts: u32,
}

impl StreamSession {
    /// 建立连接；失败按退避重试，超限返回最后一次错误。
    pub fn new(
        api: Arc<FfmpegApi>,
        settings: EncoderSettings,
        url: &CStr,
    ) -> Result<Self, MuxError> {
        let builder = || Muxer::create(api.clone(), settings, MuxFormat::Flv);
        let mut mux = builder()?;
        let mut attempts = 0;
        loop {
            match mux.connect_stream(url) {
                Ok(()) => break,
                Err(error @ MuxError::Io(_)) if attempts < STREAM_MAX_RETRIES => {
                    mux = builder()?;
                    let delay = backoff_delay_ms(attempts);
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                    let _ = error;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(Self {
            api,
            settings,
            url: url.to_owned(),
            mux,
            attempts,
        })
    }

    fn rebuild(&mut self) -> Result<(), MuxError> {
        let mut mux = Muxer::create(self.api.clone(), self.settings, MuxFormat::Flv)?;
        let mut attempts = 0u32;
        loop {
            match mux.connect_stream(&self.url) {
                Ok(()) => break,
                Err(MuxError::Io(_)) if attempts < STREAM_MAX_RETRIES => {
                    mux = Muxer::create(self.api.clone(), self.settings, MuxFormat::Flv)?;
                    std::thread::sleep(std::time::Duration::from_millis(backoff_delay_ms(
                        attempts,
                    )));
                    attempts += 1;
                }
                Err(error) => {
                    self.attempts = self.attempts.saturating_add(attempts);
                    return Err(error);
                }
            }
        }
        self.attempts = self.attempts.saturating_add(attempts);
        self.mux = mux;
        Ok(())
    }

    /// 写一帧；底层连接错误触发重连后重投当前帧。
    ///
    /// # Safety
    /// `rgba` 须指向 `stride*(height-1) + width*4` 字节可读内存。
    pub unsafe fn write(&mut self, rgba: *const u8, stride: usize) -> Result<(), MuxError> {
        match unsafe { self.mux.write(rgba, stride) } {
            Ok(()) => Ok(()),
            Err(MuxError::Io(_)) => {
                self.rebuild()?;
                unsafe { self.mux.write(rgba, stride) }
            }
            Err(error) => Err(error),
        }
    }

    /// 本会话累计重连尝试次数。
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// 关闭会话（直播流不写 trailer，直接断开）。
    pub fn close(mut self) {
        self.mux.abort();
    }
}
