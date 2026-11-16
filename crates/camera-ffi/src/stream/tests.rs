//! 审计盲区回归：amc_stream_write 视图宽高必须与编码配置一致，不一致直接拒绝。
use super::*;

fn open_camera_ctx() -> usize {
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    // 并行测试共享进程域的上下文准入上限；名额被占满时等待其它测试释放
    for _ in 0..600 {
        match unsafe { am_sdk_support::context::initialize_camera(crate::domain(), &params) } {
            Ok(key) => return key,
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED) => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            Err(other) => panic!("camera ctx init: {other}"),
        }
    }
    panic!("camera ctx admission never freed")
}

fn synthetic_object(width: u32, height: u32) -> Arc<StreamObject> {
    Arc::new(StreamObject {
        session: Mutex::new(None),
        state: AtomicU32::new(AM_LIFECYCLE_RUNNING as u32),
        width,
        height,
        stop: Arc::new(AtomicBool::new(false)),
        _permit: Mutex::new(None),
    })
}

#[test]
fn stream_write_rejects_view_dimensions_mismatching_encoder_config() {
    let ctx = open_camera_ctx();
    let mismatch = crate::domain().insert(ctx, synthetic_object(4, 4)).unwrap();
    let matching = crate::domain().insert(ctx, synthetic_object(8, 8)).unwrap();

    // 8x8 RGBA 合成视图：自洽且尺寸与 matching 配置一致
    let data = vec![0u8; 8 * 8 * 4];
    let view = am_frame_view_t {
        width: 8,
        height: 8,
        format: AM_PF_RGBA8 as u32,
        plane_count: 1,
        planes: [
            data.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        ],
        strides: [8 * 4, 0, 0, 0],
        plane_size_bytes: [(8 * 8 * 4) as u64, 0, 0, 0],
        ..Default::default()
    };
    let pixel = am_pixel_view_t {
        magic: AM_PIXEL_VIEW_MAGIC,
        view: &view,
        ..Default::default()
    };

    // 空视图指针：NULL_ARGUMENT
    assert_eq!(
        unsafe { amc_stream_write(mismatch as *mut _, std::ptr::null()) },
        AM_ERR_SDK_NULL_ARGUMENT
    );
    // 视图宽高与编码配置不一致：直接拒绝，不得进入会话写入
    assert_eq!(
        unsafe { amc_stream_write(mismatch as *mut _, &pixel) },
        AM_ERR_SDK_INVALID_ARGUMENT,
        "视图自洽不等于可用：底层按创建配置尺寸读取，宽高不一致必须拒绝"
    );
    // 宽高一致：通过闸门；无会话按句柄已关闭收口，证明闸门放行且未触碰网络
    assert_eq!(
        unsafe { amc_stream_write(matching as *mut _, &pixel) },
        AM_ERR_SDK_HANDLE_CLOSED
    );

    for key in [mismatch, matching] {
        assert_eq!(
            crate::domain().close(key, 1000).unwrap(),
            AM_LIFECYCLE_CLOSED as u32
        );
        crate::domain().destroy(key).unwrap();
    }
    assert_eq!(
        crate::domain().close(ctx, 1000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    crate::domain().destroy(ctx).unwrap();
}

#[test]
fn stream_get_status_validates_arguments_and_closed_session() {
    let ctx = open_camera_ctx();
    let key = crate::domain().insert(ctx, synthetic_object(4, 4)).unwrap();

    // NULL 输出指针：NULL_ARGUMENT
    assert_eq!(
        unsafe { amc_stream_get_status(key as *mut _, std::ptr::null_mut()) },
        AM_ERR_SDK_NULL_ARGUMENT
    );
    // struct_size=0：STRUCT_SIZE_INVALID（版本协商失败）
    let mut bad = am_stream_status_t {
        struct_size: 0,
        ..Default::default()
    };
    assert_eq!(
        unsafe { amc_stream_get_status(key as *mut _, &mut bad) },
        AM_ERR_SDK_STRUCT_SIZE_INVALID
    );
    // 会话不存在（synthetic 对象无会话）：HANDLE_CLOSED，不得触碰网络
    let mut status = am_stream_status_t::default();
    assert_eq!(
        unsafe { amc_stream_get_status(key as *mut _, &mut status) },
        AM_ERR_SDK_HANDLE_CLOSED
    );

    assert_eq!(
        crate::domain().close(key, 1000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    crate::domain().destroy(key).unwrap();
    assert_eq!(
        crate::domain().close(ctx, 1000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    crate::domain().destroy(ctx).unwrap();
}

// ── 回环 RTMP 真机门（MediaMTX 服务端 + SDK 推流端 + ffprobe/ffmpeg 收流验证器）──

const RTMP_GATE_W: u32 = 320;
const RTMP_GATE_H: u32 = 240;
/// RTMP 门固定端口（rtmp-gate.yml 批脚本口径；run-camera-gates.ps1 按门串行避免争抢）。
const RTMP_GATE_PORT: u16 = 11935;

/// 端口占用预检（审计 F14）：11935 被其它进程持有（另一 RTMP 门并行/残留 mediamtx）
/// 时门以 skip 收场并输出说明，而非 panic 或串台到他人服务端。预检后到本门
/// mediamtx 起停之间存在窄 TOCTOU 窗口，靠批脚本串行口径兜底。
fn rtmp_gate_port_free() -> bool {
    std::net::TcpListener::bind(("127.0.0.1", RTMP_GATE_PORT)).is_ok()
}

/// 端口被占用的 skip 出口：带说明返回（libtest 记 ok）；说明走 stderr 且测试名
/// 保持绿灯可辨——批脚本串行口径下正常不触发。
fn rtmp_gate_skip_when_port_busy() {
    eprintln!(
        "SKIP: 127.0.0.1:{RTMP_GATE_PORT} 已被占用（另一 RTMP 门并行或残留 mediamtx？）；\
         本门按 run-camera-gates.ps1 串行口径设计，占用即跳过"
    );
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}

/// mediamtx 进程守卫：Drop 兜底清理，不留后台进程。
struct ServerGuard(Option<std::process::Child>);
impl ServerGuard {
    fn start() -> ServerGuard {
        let dir = std::env::var("AM_SDK_MEDIAMTX_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| repo_root().join(".local/tools/mediamtx-h2"));
        let config = dir.join("rtmp-gate.yml");
        assert!(config.exists(), "RTMP 门配置缺失：{}", config.display());
        let log = std::fs::File::create(repo_root().join(".local/rtmp-gate-mediamtx.log"))
            .expect("mediamtx 日志文件创建失败");
        let err_log = log.try_clone().expect("mediamtx 日志句柄克隆失败");
        let child = std::process::Command::new(dir.join("mediamtx.exe"))
            .arg(&config)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(log))
            .stderr(std::process::Stdio::from(err_log))
            .spawn()
            .expect("mediamtx 启动失败");
        ServerGuard(Some(child))
    }
    fn kill(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.kill();
    }
}

fn wait_tcp_open(port: u16, attempts: u32) -> bool {
    for _ in 0..attempts {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    false
}

fn wait_tcp_closed(port: u16, attempts: u32) -> bool {
    for _ in 0..attempts {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    false
}

/// 有界等待子进程退出：超时杀掉，返回 (退出码, stdout, stderr)。
fn finish_child(
    child: &mut std::process::Child,
    deadline_ms: u64,
) -> (Option<i32>, String, String) {
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = read_stdio(child.stdout.take());
                let err = read_stdio(child.stderr.take());
                return (status.code(), out, err);
            }
            Ok(None) => {
                if start.elapsed().as_millis() as u64 >= deadline_ms {
                    let _ = child.kill();
                    let _ = child.wait();
                    return (None, String::new(), String::new());
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => return (None, String::new(), String::new()),
        }
    }
}

fn read_stdio<T: std::io::Read>(mut stdio: Option<T>) -> String {
    let mut text = String::new();
    if let Some(stream) = stdio.as_mut() {
        let _ = stream.read_to_string(&mut text);
    }
    text
}

/// 观察线程句柄传递（进程域整数键，跨线程只读调用 get_status）。
type StreamKey = usize;

unsafe fn rtmp_gate_ctx() -> *mut amc_context_t {
    let directory = std::env::var("AM_RT_FFMPEG_DIR").expect("AM_RT_FFMPEG_DIR 未设置");
    let path = std::ffi::CString::new(directory).unwrap();
    let dirs = [path.as_ptr()];
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: dirs.as_ptr(),
        runtime_dir_count: 1,
        ..Default::default()
    };
    let mut ctx = std::ptr::null_mut();
    assert_eq!(unsafe { crate::amc_init(&params, &mut ctx) }, 0);
    ctx
}

fn rtmp_gate_status(stream: *mut amc_stream_t) -> am_stream_status_t {
    let mut status = am_stream_status_t::default();
    assert_eq!(unsafe { amc_stream_get_status(stream, &mut status) }, 0);
    status
}

#[test]
#[ignore = "回环 RTMP 真机门：需本机 MediaMTX+rtmp-gate.yml、系统 ffprobe/ffmpeg 与 AM_RT_FFMPEG_DIR 运行库；由 run-camera-gates.ps1 批 RTMP 调度"]
fn rtmp_loopback_gate_pushes_valid_stream_and_exports_status() {
    let url = std::env::var("AM_SDK_RTMP_GATE_URL")
        .unwrap_or_else(|_| format!("rtmp://127.0.0.1:{RTMP_GATE_PORT}/live/cam"));
    let url_c = std::ffi::CString::new(url.clone()).unwrap();
    let ffprobe = std::env::var("AM_SDK_FFPROBE").unwrap_or_else(|_| "ffprobe".to_string());

    if !rtmp_gate_port_free() {
        rtmp_gate_skip_when_port_busy();
        return;
    }
    let mut server = ServerGuard::start();
    assert!(
        wait_tcp_open(RTMP_GATE_PORT, 60),
        "mediamtx 未在 127.0.0.1:{RTMP_GATE_PORT} 监听（rtmp-gate.yml）"
    );

    unsafe {
        let ctx = rtmp_gate_ctx();
        let cfg = am_encoder_config_t {
            width: RTMP_GATE_W,
            height: RTMP_GATE_H,
            bitrate_kbps: 1500,
            gop_size: 30,
            fps_num: 30,
            fps_den: 1,
            queue_max_items: 64,
            queue_max_bytes: 1 << 20,
            ..Default::default()
        };
        let mut stream = std::ptr::null_mut();
        assert_eq!(
            amc_stream_open(ctx, url_c.as_ptr(), &cfg, &mut stream),
            0,
            "首连 RTMP 服务端失败（服务端已监听）"
        );

        // 阶段一：open 后状态快照——已连接、零计数、无失败、无退避。
        let status = rtmp_gate_status(stream);
        assert_eq!(status.connection_state, AM_STREAM_CONNECTED);
        assert_eq!(status.reconnect_attempts, 0, "首连成功不计重连");
        assert_eq!(status.bytes_written, 0, "尚未写帧");
        assert_eq!(status.backoff_ms, 0, "从未退避");
        assert_eq!(status.last_error_status, 0, "无失败");

        // 阶段二：泵送合成帧（x264 veryfast+zerolatency），期间并发收流验证。
        let mut buffer = vec![0u8; (RTMP_GATE_W * RTMP_GATE_H * 4) as usize];
        for (i, byte) in buffer.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }
        let view = am_frame_view_t {
            width: RTMP_GATE_W,
            height: RTMP_GATE_H,
            format: AM_PF_RGBA8 as u32,
            plane_count: 1,
            planes: [
                buffer.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            ],
            strides: [RTMP_GATE_W * 4, 0, 0, 0],
            plane_size_bytes: [buffer.len() as u64, 0, 0, 0],
            ..Default::default()
        };
        let pixel = am_pixel_view_t {
            magic: AM_PIXEL_VIEW_MAGIC,
            view: &view,
            ..Default::default()
        };

        let spawn_probe = || {
            std::process::Command::new(&ffprobe)
                .args([
                    "-v",
                    "error",
                    "-rtmp_live",
                    "live",
                    "-probesize",
                    "50000",
                    "-i",
                    url_c.to_str().unwrap(),
                    "-show_entries",
                    "stream=codec_name,width,height:format=format_name",
                    "-of",
                    "default=nokey=0",
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("ffprobe 启动失败")
        };
        let spawn_pull = || {
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-rtmp_live",
                    "live",
                    "-i",
                    url_c.to_str().unwrap(),
                    "-t",
                    "2",
                    "-f",
                    "null",
                    "-",
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("ffmpeg 启动失败")
        };

        // MediaMTX 收到媒体数据后才把路径置为 available（对照臂实测：发布注册在
        // 数据流动后 ~1-2s）——先泵 ~2s 建立发布路径，再做收流验证。
        for _ in 0..60 {
            assert_eq!(amc_stream_write(stream, &pixel), 0, "健康期写帧失败");
            std::thread::sleep(std::time::Duration::from_millis(33));
        }

        // 收流特征验证：ffprobe 对早于路径可用窗口的连接按 no-stream 拒绝（服务端
        // 正常行为），验证器带重试；每次尝试期间持续泵送供数据。
        let mut probe_report: Option<String> = None;
        let mut probe_failures: Vec<String> = Vec::new();
        for _ in 0..5 {
            let mut probe = spawn_probe();
            for _ in 0..120 {
                assert_eq!(amc_stream_write(stream, &pixel), 0, "健康期写帧失败");
                std::thread::sleep(std::time::Duration::from_millis(33));
                if matches!(probe.try_wait(), Ok(Some(_))) {
                    break;
                }
            }
            let (rc, out, err) = finish_child(&mut probe, 5_000);
            if rc == Some(0) && out.contains("codec_name=h264") {
                probe_report = Some(out);
                break;
            }
            probe_failures.push(format!("rc={rc:?} out={out} err={err}"));
        }
        let probe_out = match probe_report {
            Some(out) => out,
            None => panic!("ffprobe 收流验证 5 次重试均失败：{probe_failures:?}"),
        };
        assert!(
            probe_out.contains("codec_name=h264"),
            "收流侧编解码器断言失败：{probe_out}"
        );
        assert!(
            probe_out.contains(&format!("width={RTMP_GATE_W}"))
                && probe_out.contains(&format!("height={RTMP_GATE_H}")),
            "收流侧尺寸断言失败：{probe_out}"
        );
        assert!(
            probe_out.contains("format_name=flv"),
            "收流侧容器断言失败：{probe_out}"
        );

        // 2 秒接收窗口（时长证据）：验证期间持续泵送。
        let mut pull = spawn_pull();
        for _ in 0..180 {
            assert_eq!(amc_stream_write(stream, &pixel), 0, "健康期写帧失败");
            std::thread::sleep(std::time::Duration::from_millis(33));
            if matches!(pull.try_wait(), Ok(Some(_))) {
                break;
            }
        }
        let (pull_rc, _pull_out, pull_err) = finish_child(&mut pull, 10_000);
        assert_eq!(pull_rc, Some(0), "ffmpeg 2 秒接收窗口失败：{pull_err}");

        // 阶段三：状态导出——健康推送后字节累计、仍零重连。
        let status = rtmp_gate_status(stream);
        assert_eq!(status.connection_state, AM_STREAM_CONNECTED);
        assert!(status.bytes_written > 0, "推送字节必须累计");
        assert_eq!(status.reconnect_attempts, 0, "健康期不产生重连");

        // 阶段四：断线重连臂——杀服务端，write 触发退避重连直至预算耗尽。
        let bytes_before_kill = status.bytes_written;
        server.kill();
        assert!(
            wait_tcp_closed(RTMP_GATE_PORT, 40),
            "服务端端口未随进程退出释放"
        );

        // write 将阻塞在重连退避（约 15s 预算）；并发 get_status 应观察到 tryAgain。
        let observer_key: StreamKey = stream as usize;
        let observer = std::thread::spawn(move || {
            let handle = observer_key as *mut amc_stream_t;
            let mut saw_try_again = false;
            for _ in 0..400 {
                let mut st = am_stream_status_t::default();
                if amc_stream_get_status(handle, &mut st) == AM_ERR_SDK_TRY_AGAIN {
                    saw_try_again = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            saw_try_again
        });
        let write_rc = amc_stream_write(stream, &pixel);
        let saw_try_again = observer.join().expect("观察线程 panic");
        assert_eq!(
            write_rc, AM_ERR_CAMERA_TIMEOUT,
            "预算耗尽归一 camera.timeout"
        );
        assert!(
            saw_try_again,
            "write 重连持锁期间并发 get_status 必须返回 tryAgain"
        );

        // 断线后状态：DISCONNECTED + 重连计数/退避档/最近失败可观测。
        let status = rtmp_gate_status(stream);
        assert_eq!(status.connection_state, AM_STREAM_DISCONNECTED);
        assert!(status.reconnect_attempts > 0, "重连尝试必须计数");
        assert!(status.backoff_ms > 0, "退避档必须导出（1s×2^n 封顶 30s）");
        assert!(status.backoff_ms <= 30_000);
        assert_eq!(
            status.last_error_status, AM_ERR_CAMERA_TIMEOUT,
            "最近失败=预算耗尽稳定码"
        );
        assert!(
            status.bytes_written >= bytes_before_kill,
            "断线不回退已计字节"
        );
        // F08 回归锚点：捕获预算耗尽后的计数，阶段五首轮重连成功必须在此之上增量。
        let attempts_after_exhaustion = status.reconnect_attempts;

        // 阶段五：重启服务端，write 内部重连成功续推。
        server = ServerGuard::start();
        assert!(wait_tcp_open(RTMP_GATE_PORT, 60), "mediamtx 重启后未监听");
        assert_eq!(
            amc_stream_write(stream, &pixel),
            0,
            "服务端恢复后重连续推失败"
        );
        for _ in 0..30 {
            assert_eq!(amc_stream_write(stream, &pixel), 0);
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
        let status = rtmp_gate_status(stream);
        assert_eq!(status.connection_state, AM_STREAM_CONNECTED);
        assert!(
            status.bytes_written > bytes_before_kill,
            "重连后字节继续累计"
        );
        assert!(
            status.reconnect_attempts > 0 && status.backoff_ms > 0,
            "历史重连/退避观测保留"
        );
        // F08：服务端恢复后的重连通常首轮即成功——该次真实重连尝试也必须登记，
        // 只验历史 >0 抓不住快速恢复漏计（修复前此处增量为 0）。
        assert!(
            status.reconnect_attempts > attempts_after_exhaustion,
            "首轮重连成功也必须累计重连尝试（F08）：{} → {}",
            attempts_after_exhaustion,
            status.reconnect_attempts
        );
        assert_eq!(
            status.last_error_status, AM_ERR_CAMERA_TIMEOUT,
            "成功不清除最近失败"
        );

        let mut lifecycle = 0u32;
        assert_eq!(amc_stream_close(stream, 5_000, &mut lifecycle), 0);
        assert_eq!(lifecycle, AM_LIFECYCLE_CLOSED as u32);
        assert_eq!(crate::amc_object_destroy(stream.cast()), 0);
        assert_eq!(crate::amc_shutdown(ctx, 5_000, &mut lifecycle), 0);
        assert_eq!(crate::amc_object_destroy(ctx.cast()), 0);
    }

    server.kill();
}

// ── 回环 RTMP 多路真机门（同 MediaMTX 实例并发多路径推流，清偿"多路并发未测"边界）──

/// 多路尺寸表：互不相同的宽高是"互不串流"的端到端断言载体——若 lane_i 的
/// URL 上收到其它路的流，ffprobe 的宽高断言必然失败。全部偶数（H.264 要求）。
const RTMP_MULTI_LANE_SPECS: [(u32, u32); 8] = [
    (320, 240),
    (352, 288),
    (384, 288),
    (416, 336),
    (448, 336),
    (480, 384),
    (512, 400),
    (544, 416),
];

/// 泵线程结果：累计成功帧数，或首次 write 失败的描述。
type PumpOutcome = Result<u64, String>;

/// 泵线程 RAII 守卫（审计 F14）：Drop 置停止位并 join 全部泵——断言 panic 的
/// 展开路径也回收泵线程，不 detach 残留到进程退出；正常路径经 finish() 取回结果。
struct PumpGuard {
    stop: Arc<AtomicBool>,
    pumps: Vec<std::thread::JoinHandle<PumpOutcome>>,
}

impl PumpGuard {
    fn new(stop: Arc<AtomicBool>, pumps: Vec<std::thread::JoinHandle<PumpOutcome>>) -> Self {
        Self { stop, pumps }
    }

    /// 正常收尾：置停止位、逐路 join，返回各路结果（顺序=泵序）。
    fn finish(&mut self) -> Vec<PumpOutcome> {
        self.stop.store(true, Ordering::Release);
        self.pumps
            .drain(..)
            .map(|pump| pump.join().expect("泵线程 panic"))
            .collect()
    }
}

impl Drop for PumpGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for pump in self.pumps.drain(..) {
            let _ = pump.join();
        }
    }
}

/// F14 常驻单测（RED 检查法固化）：守卫持有期间 panic 展开，Drop 必须停泵并
/// join 全部泵线程——live 归零即"无 detach 残留"的可判定证据。
#[test]
fn pump_guard_stops_and_joins_pumps_on_panic_unwind() {
    let stop = Arc::new(AtomicBool::new(false));
    let live = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let pumps: Vec<std::thread::JoinHandle<PumpOutcome>> = (0..3)
        .map(|_| {
            let stop = stop.clone();
            let live = live.clone();
            std::thread::spawn(move || -> PumpOutcome {
                live.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                while !stop.load(Ordering::Acquire) {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                live.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                Ok(0)
            })
        })
        .collect();
    let guard = PumpGuard::new(stop, pumps);
    let live_after = live.clone();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = guard;
        panic!("注入断言：验证 panic 展开路径停泵并 join（F14）");
    }));
    assert!(unwind.is_err(), "注入的断言必须触发展开");
    assert_eq!(
        live_after.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "panic 展开路径 Drop 必须停泵并 join 全部泵线程，不得 detach 残留"
    );
}

fn spawn_lane_probe(ffprobe: &str, url: &str) -> std::process::Child {
    std::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-rtmp_live",
            "live",
            "-probesize",
            "50000",
            "-i",
            url,
            "-show_entries",
            "stream=codec_name,width,height:format=format_name",
            "-of",
            "default=nokey=0",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("ffprobe 启动失败")
}

fn spawn_lane_pull(url: &str) -> std::process::Child {
    std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-rtmp_live",
            "live",
            "-i",
            url,
            "-t",
            "2",
            "-f",
            "null",
            "-",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("ffmpeg 启动失败")
}

#[test]
#[ignore = "回环 RTMP 多路真机门：需本机 MediaMTX+rtmp-gate.yml、系统 ffprobe/ffmpeg 与 AM_RT_FFMPEG_DIR 运行库；由 run-camera-gates.ps1 批 RTMP 调度（默认 4 路并发；AM_SDK_RTMP_GATE_LANES=2..=8 供边界探测复跑）"]
fn rtmp_multi_lane_gate_pushes_concurrent_streams() {
    let lanes = std::env::var("AM_SDK_RTMP_GATE_LANES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4);
    assert!(
        (2..=RTMP_MULTI_LANE_SPECS.len()).contains(&lanes),
        "AM_SDK_RTMP_GATE_LANES 须在 2..={}",
        RTMP_MULTI_LANE_SPECS.len()
    );
    let base = std::env::var("AM_SDK_RTMP_GATE_MULTI_BASE")
        .unwrap_or_else(|_| format!("rtmp://127.0.0.1:{RTMP_GATE_PORT}/live"));
    let ffprobe = std::env::var("AM_SDK_FFPROBE").unwrap_or_else(|_| "ffprobe".to_string());

    if !rtmp_gate_port_free() {
        rtmp_gate_skip_when_port_busy();
        return;
    }
    let mut server = ServerGuard::start();
    assert!(
        wait_tcp_open(RTMP_GATE_PORT, 60),
        "mediamtx 未在 127.0.0.1:{RTMP_GATE_PORT} 监听（rtmp-gate.yml）"
    );

    unsafe {
        let ctx = rtmp_gate_ctx();

        // 阶段一：同 ctx 并发 open 各路（同服务端实例、不同路径、不同尺寸配置）。
        let mut streams: Vec<(*mut amc_stream_t, String, (u32, u32))> = Vec::new();
        for (lane, &(width, height)) in RTMP_MULTI_LANE_SPECS[..lanes].iter().enumerate() {
            let url = format!("{base}/lane{lane}");
            let url_c = std::ffi::CString::new(url.clone()).unwrap();
            let cfg = am_encoder_config_t {
                width,
                height,
                bitrate_kbps: 1500,
                gop_size: 30,
                fps_num: 30,
                fps_den: 1,
                queue_max_items: 64,
                queue_max_bytes: 1 << 20,
                ..Default::default()
            };
            let mut stream = std::ptr::null_mut();
            assert_eq!(
                amc_stream_open(ctx, url_c.as_ptr(), &cfg, &mut stream),
                0,
                "lane {lane} 首连 RTMP 服务端失败（服务端已监听）"
            );
            streams.push((stream, url, (width, height)));
        }

        // open 后各路状态：全部已连接、零计数、无失败——并发首连互不干扰。
        for (lane, (stream, _, _)) in streams.iter().enumerate() {
            let status = rtmp_gate_status(*stream);
            assert_eq!(status.connection_state, AM_STREAM_CONNECTED, "lane {lane}");
            assert_eq!(status.reconnect_attempts, 0, "lane {lane} 首连不计重连");
            assert_eq!(status.bytes_written, 0, "lane {lane} 尚未写帧");
            assert_eq!(status.backoff_ms, 0, "lane {lane} 从未退避");
            assert_eq!(status.last_error_status, 0, "lane {lane} 无失败");
        }

        // 阶段二：每路独立泵线程（独立合成帧，尺寸匹配本路配置）30fps 并发写帧。
        // 泵句柄随即收入 RAII 守卫：此后任一断言 panic 的展开路径也停泵并 join。
        let pump_stop = Arc::new(AtomicBool::new(false));
        let pumps: Vec<_> = streams
            .iter()
            .enumerate()
            .map(|(lane, (stream, _, _))| {
                let stop = pump_stop.clone();
                let key = *stream as usize;
                std::thread::spawn(move || -> PumpOutcome {
                    let (width, height) = RTMP_MULTI_LANE_SPECS[lane];
                    let mut buffer = vec![0u8; (width as usize) * (height as usize) * 4];
                    for (i, byte) in buffer.iter_mut().enumerate() {
                        *byte = ((i + lane * 37) % 251) as u8;
                    }
                    let view = am_frame_view_t {
                        width,
                        height,
                        format: AM_PF_RGBA8 as u32,
                        plane_count: 1,
                        planes: [
                            buffer.as_ptr(),
                            std::ptr::null(),
                            std::ptr::null(),
                            std::ptr::null(),
                        ],
                        strides: [width * 4, 0, 0, 0],
                        plane_size_bytes: [buffer.len() as u64, 0, 0, 0],
                        ..Default::default()
                    };
                    let pixel = am_pixel_view_t {
                        magic: AM_PIXEL_VIEW_MAGIC,
                        view: &view,
                        ..Default::default()
                    };
                    let handle = key as *mut amc_stream_t;
                    let mut frames = 0u64;
                    while !stop.load(Ordering::Acquire) {
                        let rc = amc_stream_write(handle, &pixel);
                        if rc != 0 {
                            return Err(format!("lane {lane} write rc={rc}（已推 {frames} 帧）"));
                        }
                        frames += 1;
                        std::thread::sleep(std::time::Duration::from_millis(33));
                    }
                    Ok(frames)
                })
            })
            .collect();
        let mut pump_guard = PumpGuard::new(pump_stop, pumps);

        // 建立发布路径：MediaMTX 在数据流动 ~1-2s 后才把路径置为 available，
        // 先并发泵 ~2.5s 再开始收流验证（期间各路持续泵送）。
        std::thread::sleep(std::time::Duration::from_millis(2_500));

        // 阶段三：逐路 ffprobe 收流校验（未过的路并行重试，至多 5 轮）——
        // h264 + 本路宽高 + flv + 单视频流（width= 恰一次：两路共用同一路径会
        // 出现两个 STREAM 块）。宽高即"互不串流"断言：串流必致尺寸失配。
        let mut verified = vec![false; lanes];
        let mut probe_failures: Vec<String> = Vec::new();
        for round in 0..5 {
            let pending: Vec<(usize, std::process::Child)> = streams
                .iter()
                .enumerate()
                .filter(|(lane, _)| !verified[*lane])
                .map(|(lane, (_, url, _))| (lane, spawn_lane_probe(&ffprobe, url)))
                .collect();
            for (lane, mut probe) in pending {
                let (rc, out, err) = finish_child(&mut probe, 5_000);
                let (width, height) = RTMP_MULTI_LANE_SPECS[lane];
                if rc == Some(0)
                    && out.contains("codec_name=h264")
                    && out.contains(&format!("width={width}"))
                    && out.contains(&format!("height={height}"))
                    && out.contains("format_name=flv")
                    && out.matches("width=").count() == 1
                {
                    verified[lane] = true;
                } else {
                    probe_failures.push(format!(
                        "round {round} lane {lane} rc={rc:?} out={out} err={err}"
                    ));
                }
            }
            if verified.iter().all(|v| *v) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        assert!(
            verified.iter().all(|v| *v),
            "ffprobe 逐路收流校验未全过：{probe_failures:?}"
        );

        // 阶段四：并行 2 秒接收窗口（每路 ffmpeg 拉流解码，时长证据）。
        let pulls: Vec<(usize, std::process::Child)> = streams
            .iter()
            .enumerate()
            .map(|(lane, (_, url, _))| (lane, spawn_lane_pull(url)))
            .collect();
        for (lane, mut pull) in pulls {
            let (rc, _out, err) = finish_child(&mut pull, 10_000);
            assert_eq!(rc, Some(0), "lane {lane} ffmpeg 2 秒接收窗口失败：{err}");
        }

        // 阶段五：停泵回收（守卫正常路径）——各路必须全程健康（零 write 失败）且
        // 帧数合理；此前任一断言 panic 时由 PumpGuard::drop 停泵并 join（F14）。
        let outcomes = pump_guard.finish();
        for (lane, outcome) in outcomes.into_iter().enumerate() {
            match outcome {
                Ok(frames) => assert!(frames >= 30, "lane {lane} 泵送帧数不足（{frames} < 30）"),
                Err(failure) => panic!("lane {lane} 泵送失败：{failure}"),
            }
        }

        // 阶段六：状态导出——各路独立 CONNECTED + 字节独立累计 + 零重连零失败
        //（字节互不串账的 SDK 侧证据：各会话计数器独立）。
        for (lane, (stream, _, _)) in streams.iter().enumerate() {
            let status = rtmp_gate_status(*stream);
            assert_eq!(
                status.connection_state, AM_STREAM_CONNECTED,
                "lane {lane} 健康推送后必须仍连接"
            );
            assert!(status.bytes_written > 0, "lane {lane} 推送字节必须累计");
            assert_eq!(status.reconnect_attempts, 0, "lane {lane} 健康期不产生重连");
            assert_eq!(status.backoff_ms, 0, "lane {lane} 从未退避");
            assert_eq!(status.last_error_status, 0, "lane {lane} 无失败");
        }

        // 阶段七：三段式关闭各路 + ctx 收口。
        for (lane, (stream, _, _)) in streams.iter().enumerate() {
            let mut lifecycle = 0u32;
            assert_eq!(
                amc_stream_close(*stream, 5_000, &mut lifecycle),
                0,
                "lane {lane}"
            );
            assert_eq!(lifecycle, AM_LIFECYCLE_CLOSED as u32, "lane {lane}");
            assert_eq!(crate::amc_object_destroy(stream.cast()), 0, "lane {lane}");
        }
        let mut lifecycle = 0u32;
        assert_eq!(crate::amc_shutdown(ctx, 5_000, &mut lifecycle), 0);
        assert_eq!(lifecycle, AM_LIFECYCLE_CLOSED as u32);
        assert_eq!(crate::amc_object_destroy(ctx.cast()), 0);
    }

    server.kill();
}
