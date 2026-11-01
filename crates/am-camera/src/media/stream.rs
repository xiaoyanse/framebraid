//! 推流会话：FLV/RTMP 连接与断线重连（退避 1s×2^n 封顶 30s，上限 5 次）。
//! 连接/重连受总预算约束，退避等待可被停止信号唤醒；单段 IO 由 muxer 的
//! interrupt 回调中断——宿主不再被无界的连接重试长时间阻塞。
use super::encode::EncoderSettings;
use super::ffmpeg_lib::FfmpegApi;
use super::mux::{
    backoff_delay_ms, MuxError, MuxFormat, Muxer, STREAM_CONNECT_BUDGET_MS, STREAM_MAX_RETRIES,
};
use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 退避等待分片：等待期间反复检查停止信号，close/停止能立即打断重连节奏。
const WAIT_SLICE: Duration = Duration::from_millis(50);

pub struct StreamSession {
    api: Arc<FfmpegApi>,
    settings: EncoderSettings,
    url: CString,
    mux: Muxer,
    attempts: u32,
    stop: Arc<AtomicBool>,
}

/// 可唤醒等待：全程未收到停止信号返回 true，被取消返回 false。
fn wait_interruptible(delay_ms: u64, stop: &AtomicBool) -> bool {
    let mut remaining = Duration::from_millis(delay_ms);
    while remaining > Duration::ZERO {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let slice = remaining.min(WAIT_SLICE);
        std::thread::sleep(slice);
        remaining -= slice;
    }
    !stop.load(Ordering::Relaxed)
}

impl StreamSession {
    /// 建立连接：单段 IO 中断 + 总预算 + 可唤醒退避；取消/超预算归一为 Timeout。
    pub fn new(
        api: Arc<FfmpegApi>,
        settings: EncoderSettings,
        url: &CStr,
        stop: Arc<AtomicBool>,
    ) -> Result<Self, MuxError> {
        let budget = Instant::now() + Duration::from_millis(STREAM_CONNECT_BUDGET_MS);
        let mut attempts = 0u32;
        loop {
            let mut mux = Muxer::create(api.clone(), settings, MuxFormat::Flv)?;
            match mux.connect_stream(url, Some(&stop)) {
                Ok(()) => {
                    return Ok(Self {
                        api,
                        settings,
                        url: url.to_owned(),
                        mux,
                        attempts,
                        stop,
                    });
                }
                Err(error @ MuxError::Io(_))
                    if attempts < STREAM_MAX_RETRIES && Instant::now() < budget =>
                {
                    attempts += 1;
                    if !wait_interruptible(backoff_delay_ms(attempts - 1), &stop)
                        || Instant::now() >= budget
                    {
                        return Err(MuxError::Timeout);
                    }
                    let _ = error;
                }
                Err(_) if stop.load(Ordering::Relaxed) => return Err(MuxError::Timeout),
                Err(error) => return Err(error),
            }
        }
    }

    fn rebuild(&mut self) -> Result<(), MuxError> {
        let budget = Instant::now() + Duration::from_millis(STREAM_CONNECT_BUDGET_MS);
        let mut attempts = 0u32;
        loop {
            let mut mux = Muxer::create(self.api.clone(), self.settings, MuxFormat::Flv)?;
            match mux.connect_stream(&self.url, Some(&self.stop)) {
                Ok(()) => {
                    self.attempts = self.attempts.saturating_add(attempts);
                    self.mux = mux;
                    return Ok(());
                }
                Err(MuxError::Io(_))
                    if attempts < STREAM_MAX_RETRIES && Instant::now() < budget =>
                {
                    attempts += 1;
                    if !wait_interruptible(backoff_delay_ms(attempts - 1), &self.stop)
                        || Instant::now() >= budget
                    {
                        return Err(MuxError::Timeout);
                    }
                }
                Err(_) if self.stop.load(Ordering::Relaxed) => return Err(MuxError::Timeout),
                Err(error) => {
                    self.attempts = self.attempts.saturating_add(attempts);
                    return Err(error);
                }
            }
        }
    }

    /// 写一帧；底层连接错误触发重连后重投当前帧（重连受预算与停止信号约束）。
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

    /// 停止信号引用（宿主侧关闭/取消时置位）。
    pub fn stop_flag(&self) -> &Arc<AtomicBool> {
        &self.stop
    }

    /// 关闭会话（直播流不写 trailer，直接断开）。
    pub fn close(mut self) {
        self.mux.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// issue #8：退避等待必须可被停止信号立即打断（close/取消不长时间阻塞）。
    #[test]
    fn wait_interruptible_returns_false_immediately_when_stopped() {
        let stop = Arc::new(AtomicBool::new(true));
        let t0 = Instant::now();
        assert!(!wait_interruptible(10_000, &stop), "已置位：立即取消");
        assert!(
            t0.elapsed() < Duration::from_millis(500),
            "不得等满退避时长"
        );
    }

    #[test]
    fn wait_interruptible_elapses_when_not_stopped() {
        let stop = Arc::new(AtomicBool::new(false));
        let t0 = Instant::now();
        assert!(wait_interruptible(120, &stop), "未置位：等满返回 true");
        assert!(t0.elapsed() >= Duration::from_millis(110));
    }
}
