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
    /// 最近一次 open/write 成功路径=true；write 因连接失败返回错误后=false。
    connected: bool,
    /// 已随重连折叠的旧封装器字节（现封装器字节数经 status() 现算合计）。
    folded_bytes: u64,
    /// 最近一次实际等待的退避档（毫秒）；0=从未退避。
    last_backoff_ms: u64,
    /// 最近一次导出给宿主的失败（成功不清除）；None=无失败。
    last_error: Option<MuxError>,
    stop: Arc<AtomicBool>,
}

/// 推流状态只读快照（amc_stream_get_status 导出；调用边界观察值）。
pub struct StreamStatus {
    pub connected: bool,
    pub reconnect_attempts: u32,
    pub bytes_written: u64,
    pub last_backoff_ms: u64,
    pub last_error: Option<MuxError>,
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
        let mut last_backoff_ms = 0u64;
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
                        connected: true,
                        folded_bytes: 0,
                        last_backoff_ms,
                        last_error: None,
                        stop,
                    });
                }
                Err(error @ MuxError::Io(_))
                    if attempts < STREAM_MAX_RETRIES && Instant::now() < budget =>
                {
                    attempts += 1;
                    let backoff = backoff_delay_ms(attempts - 1);
                    last_backoff_ms = backoff;
                    if !wait_interruptible(backoff, &stop) || Instant::now() >= budget {
                        return Err(MuxError::Timeout);
                    }
                    let _ = error;
                }
                Err(_) if stop.load(Ordering::Relaxed) => return Err(MuxError::Timeout),
                Err(error) => {
                    // 连接段可能跨过预算终点（单段失败暴露最迟 5s）：超预算归一 Timeout。
                    return Err(if Instant::now() >= budget {
                        MuxError::Timeout
                    } else {
                        error
                    });
                }
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
                    self.folded_bytes = self.folded_bytes.saturating_add(self.mux.bytes_written());
                    self.mux = mux;
                    return Ok(());
                }
                Err(MuxError::Io(_))
                    if attempts < STREAM_MAX_RETRIES && Instant::now() < budget =>
                {
                    attempts += 1;
                    let backoff = backoff_delay_ms(attempts - 1);
                    self.last_backoff_ms = backoff;
                    if !wait_interruptible(backoff, &self.stop) || Instant::now() >= budget {
                        // 预算耗尽/取消也如实累计已发生的重连尝试（可观测语义）。
                        self.attempts = self.attempts.saturating_add(attempts);
                        return Err(MuxError::Timeout);
                    }
                }
                Err(_) if self.stop.load(Ordering::Relaxed) => {
                    self.attempts = self.attempts.saturating_add(attempts);
                    return Err(MuxError::Timeout);
                }
                Err(error) => {
                    self.attempts = self.attempts.saturating_add(attempts);
                    // 连接段可能跨过预算终点（单段失败暴露最迟 5s）：超预算归一 Timeout。
                    return Err(if Instant::now() >= budget {
                        MuxError::Timeout
                    } else {
                        error
                    });
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
            Ok(()) => {
                self.connected = true;
                Ok(())
            }
            Err(error @ MuxError::Io(_)) => {
                self.connected = false;
                if let Err(reconnect) = self.rebuild() {
                    self.last_error = Some(reconnect);
                    return Err(reconnect);
                }
                let _ = error;
                match unsafe { self.mux.write(rgba, stride) } {
                    Ok(()) => {
                        self.connected = true;
                        Ok(())
                    }
                    Err(retry) => {
                        self.connected = false;
                        self.last_error = Some(retry);
                        Err(retry)
                    }
                }
            }
            // 非 IO 失败（编码/配置）不改变连接状态观测，仍登记最近一次失败。
            Err(error) => {
                self.last_error = Some(error);
                Err(error)
            }
        }
    }

    /// 只读状态快照：连接状态/累计重连尝试/累计推送字节/最近退避档/最近失败。
    pub fn status(&self) -> StreamStatus {
        StreamStatus {
            connected: self.connected,
            reconnect_attempts: self.attempts,
            bytes_written: self.folded_bytes.saturating_add(self.mux.bytes_written()),
            last_backoff_ms: self.last_backoff_ms,
            last_error: self.last_error,
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
