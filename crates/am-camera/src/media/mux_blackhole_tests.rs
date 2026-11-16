//! F05 定向门：网络 IO 可中断性（黑洞/背压/停止信号三臂，纯本地 TCP 黑洞无外部进程）。
//! 修复前 avio_open 不给网络 URLContext 安装 interrupt 回调——握手/写入/关闭 flush
//! 阻塞不受 5s 单段截止与停止信号约束；修复后经 avio_open2 同源安装。
use super::ffmpeg_lib::FfmpegApi;
use super::mux::{MuxError, MuxFormat, Muxer, STREAM_CONNECT_BUDGET_MS, STREAM_IO_DEADLINE};
use super::stream::StreamSession;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 黑洞监听：accept 后持有连接不读不写（黑洞/背压形态），守卫 Drop 时一并关闭。
struct BlackHole {
    port: u16,
    stop: Arc<AtomicBool>,
    acceptor: Option<std::thread::JoinHandle<()>>,
}

impl BlackHole {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("黑洞监听绑定失败");
        let port = listener.local_addr().unwrap().port();
        listener
            .set_nonblocking(true)
            .expect("黑洞监听设非阻塞失败");
        let stop = Arc::new(AtomicBool::new(false));
        let acceptor_stop = stop.clone();
        let acceptor = std::thread::spawn(move || {
            let mut held = Vec::new();
            while !acceptor_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => held.push(stream),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
            // held 随线程退出 drop：持有期间连接保持建立不读，测试后统一关闭。
        });
        Self {
            port,
            stop,
            acceptor: Some(acceptor),
        }
    }

    fn url(&self, scheme: &str, path: &str) -> CString {
        CString::new(format!("{scheme}://127.0.0.1:{}/{path}", self.port)).unwrap()
    }
}

impl Drop for BlackHole {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(acceptor) = self.acceptor.take() {
            let _ = acceptor.join();
        }
    }
}

fn runtime_api() -> Arc<FfmpegApi> {
    let dir = std::env::var("AM_RT_FFMPEG_DIR").expect("AM_RT_FFMPEG_DIR 未设置");
    Arc::new(FfmpegApi::load_from(std::path::Path::new(&dir)).expect("运行库装载失败"))
}

fn gate_settings() -> super::encode::EncoderSettings {
    super::encode::EncoderSettings {
        width: 320,
        height: 240,
        bitrate_kbps: 1500,
        gop_size: 10,
        fps_num: 30,
        fps_den: 1,
    }
}

/// 黑洞噪声帧（320×240 RGBA）：高熵内容保证编码包尺寸，快速填满收发缓冲。
fn noise_frame() -> Vec<u8> {
    let mut buffer = vec![0u8; 320 * 240 * 4];
    let mut state = 0x1234_5678u32;
    for byte in buffer.iter_mut() {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *byte = (state >> 24) as u8;
    }
    buffer
}

#[test]
#[ignore = "F05 黑洞门：需显式FFmpeg运行库（AM_RT_FFMPEG_DIR）；由 run-camera-gates.ps1 批 F05 调度"]
fn stream_connect_blackhole_exhausts_budget_without_hanging() {
    let hole = BlackHole::start();
    let stop = Arc::new(AtomicBool::new(false));
    let t0 = Instant::now();
    let error = match StreamSession::new(
        runtime_api(),
        gate_settings(),
        &hole.url("rtmp", "live/cam"),
        stop,
    ) {
        Ok(_) => panic!("黑洞握手不应连接成功"),
        Err(error) => error,
    };
    let elapsed = t0.elapsed();

    // 黑洞握手（accept 不响应）：每段被 5s 截止打断，总预算 15s 内归一 Timeout。
    assert_eq!(error, MuxError::Timeout, "预算耗尽必须归一 Timeout");
    assert!(
        elapsed >= Duration::from_secs(12),
        "黑洞段为真实阻塞等待（每段 5s），过快返回暗示未走网络路径：{elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_millis(STREAM_CONNECT_BUDGET_MS) + Duration::from_secs(5),
        "总耗时不得越出预算+单段容差：{elapsed:?}"
    );
}

#[test]
#[ignore = "F05 背压门：需显式FFmpeg运行库（AM_RT_FFMPEG_DIR）；由 run-camera-gates.ps1 批 F05 调度"]
fn muxer_write_backpressure_fails_within_segment_deadline() {
    let hole = BlackHole::start();
    let mut muxer = Muxer::create(runtime_api(), gate_settings(), MuxFormat::Flv)
        .expect("muxer 创建（无 IO 阶段）");
    // tcp:// 为载波：与 rtmp:// 同经 URLContext interrupt/重试包装层（F05 修复层），
    // 且无 RTMP 握手依赖——黑洞 accept 即成立，open/写头即小量数据可入缓冲。
    muxer
        .connect_stream(&hole.url("tcp", "sink"), None, None)
        .expect("黑洞 TCP 连接与 FLV 写头应成功（数据入收发缓冲）");

    let frame = noise_frame();
    let mut written = 0u32;
    let outcome = loop {
        let t_write = Instant::now();
        // SAFETY：帧指向 320×240×4 连续可读内存，stride 与宽一致。
        let result = unsafe { muxer.write(frame.as_ptr(), 320 * 4) };
        match result {
            Ok(()) => {
                written += 1;
                assert!(
                    t_write.elapsed() < STREAM_IO_DEADLINE,
                    "健康段写帧不应触截止：{written} 帧 {elapsed:?}",
                    elapsed = t_write.elapsed()
                );
            }
            Err(error) => break (error, t_write.elapsed()),
        }
    };
    let (error, blocked_for) = outcome;

    assert!(written >= 1, "背压前必须有成功写入（连接/写头真实建立）");
    assert!(
        matches!(error, MuxError::Io(_)),
        "写背压必须报 Io 类（重连语义）：{error:?}"
    );
    assert!(
        blocked_for >= Duration::from_secs(4),
        "背压段必须真实阻塞至单段截止（远小于截止即失败=非背压形态）：{blocked_for:?}"
    );
    assert!(
        blocked_for <= STREAM_IO_DEADLINE + Duration::from_secs(1),
        "写段截止最迟 5s+容差暴露，不得无限阻塞：{blocked_for:?}"
    );
    // Drop 走 abort：已到期截止使 close flush 立即退出——若此处挂起即 abort 回归。
}

#[test]
#[ignore = "F05 停止信号门：需显式FFmpeg运行库（AM_RT_FFMPEG_DIR）；由 run-camera-gates.ps1 批 F05 调度"]
fn stream_connect_stop_signal_interrupts_open_immediately() {
    let hole = BlackHole::start();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let url = hole.url("rtmp", "live/cam");
    let api = runtime_api();
    let settings = gate_settings();

    let worker = std::thread::spawn(move || StreamSession::new(api, settings, &url, worker_stop));
    // 等待进入黑洞握手阻塞段，再置停止信号：中断必须远早于 5s 单段截止。
    std::thread::sleep(Duration::from_millis(800));
    let t_signal = Instant::now();
    stop.store(true, Ordering::Release);
    let error = match worker.join().expect("open 线程 panic") {
        Ok(_) => panic!("黑洞握手不应连接成功"),
        Err(error) => error,
    };
    let signal_to_return = t_signal.elapsed();

    assert_eq!(error, MuxError::Timeout, "停止信号取消归一 Timeout");
    assert!(
        signal_to_return < Duration::from_secs(2),
        "停止信号须在握手阻塞段内立即中断（非等满 5s 截止）：{signal_to_return:?}"
    );
}
