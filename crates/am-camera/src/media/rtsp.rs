//! RTSP 拉流底座（P08 1.1 技术门矩阵② + 2.1 持续拉流）：
//! 打开（TCP 传输 + 5s 网络超时）→ 单帧软解 → RGBA；持续拉流线程复用 open/probe/read 工具。
//! 硬解路径经 hardware_hwaccel 探测后由管线层装配，此处管拉流+解码+取帧链路。
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::ffmpeg_lib::{api, FfmpegApi};
use super::ffmpeg_sys::*;

/// RGBA 解码帧（原 app-core media::hardware_hwaccel 定义，随拉流域迁入本 crate；
/// app-core 侧经 hardware_hwaccel 模块 re-export 保持既有路径）。
/// 解码产物：RGBA8 像素（驾驶舱/推理统一落点）。
#[derive(Clone, Debug)]
pub struct RgbaFrame {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
}

/// 常驻包守卫（原 app-core media::decoder 定义）：av_packet_alloc 由 FFmpeg 按真实
/// 布局分配并清零——栈上构造手抄 AVPacket 缺尾部字段（opaque_ref 等），
/// send_packet 内部按真实大小读取会解引用栈垃圾（已实证段错误根因）。
pub struct PacketGuard(pub *mut AVPacket);
impl Drop for PacketGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            if let Ok(lib) = api() {
                // SAFETY：指针来自 packet_alloc 且未二次释放。
                unsafe { (lib.av_packet_free)(&mut self.0) };
            }
        }
    }
}

/// 解码器低延迟装配（原 app-core media::decoder 定义，RTSP 源 open2 前调用）：
/// LOW_DELAY 标志 + 单线程无帧级并行；thread_type 布局未抄入 ffmpeg_sys，
/// 经 AVClass 选项系统写入。
// 原为 app-core 内部函数（pub(crate) 不触发该 lint）：ctx 由内部受控调用点
// （decode_first_frame/decoder 装配）传入并拥有，随 crate 迁出放宽可见性不改语义
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn apply_low_latency(lib: &FfmpegApi, ctx: *mut AVCodecContext) -> Result<(), String> {
    let type_key = CString::new("thread_type").map_err(|_| "选项键非法".to_string())?;
    // SAFETY：ctx 来自 alloc_context3（av_class 在布局首位）；位域与选项写入后立即读回。
    unsafe {
        (*ctx).flags |= AV_CODEC_FLAG_LOW_DELAY;
        (*ctx).thread_count = 1;
        let rc = (lib.av_opt_set_int)(
            ctx.cast::<core::ffi::c_void>(),
            type_key.as_ptr(),
            0, // AV_THREAD_TYPE_NONE
            0,
        );
        if rc != 0 {
            return Err(format!("thread_type 设置失败: {}", lib.errstr(rc)));
        }
        if (*ctx).flags & AV_CODEC_FLAG_LOW_DELAY == 0 || (*ctx).thread_count != 1 {
            return Err("解码器低延迟参数读回不一致".into());
        }
    }
    Ok(())
}

/// 单段 IO（握手/探测/读帧）最长等待：超时经 interrupt 回调中断。
const IO_DEADLINE: Duration = Duration::from_secs(5);

/// 中断状态：deadline 限单段等待；stop 由租约释放触发立即中断阻塞 IO。
/// 存 Box（内容地址稳定），opaque 指针在 InputGuard 移动后仍有效。
pub(crate) struct InterruptState {
    deadline: std::cell::Cell<Instant>,
    stop: *const AtomicBool,
}

// SAFETY：stop 指向拉流线程持有的 SourceEntry.stop，生命周期覆盖 InputGuard。
unsafe impl Send for InterruptState {}

/// FFmpeg 中断回调：stop 置位或超 deadline 返回非 0。
unsafe extern "C" fn interrupt_cb(opaque: *mut core::ffi::c_void) -> core::ffi::c_int {
    // SAFETY：opaque 指向 InputGuard 持有的 Box<InterruptState>，仅 IO 同步调用期间触发。
    let state = unsafe { &*(opaque.cast::<InterruptState>()) };
    if !state.stop.is_null() && unsafe { (*state.stop).load(Ordering::Relaxed) } {
        return 1;
    }
    if Instant::now() >= state.deadline.get() {
        return 1;
    }
    0
}

/// 网络初始化幂等（首次调用方触发）。
fn network_init(lib: &super::ffmpeg_lib::FfmpegApi) {
    static NETWORK: OnceLock<()> = OnceLock::new();
    NETWORK.get_or_init(|| {
        // SAFETY：幂等初始化。
        unsafe { (lib.avformat_network_init)() };
    });
}

/// 视频流定位结果（codecpar 读数）。
pub struct VideoStream {
    pub stream_index: i32,
    pub width: i32,
    pub height: i32,
    pub codec_id: i32,
    /// 全局头（SDP sprop 参数集等）：持续解码器初始化必需（2.2）。
    pub extradata: Vec<u8>,
    /// 流帧率（2.3 预算表数据点）；探测失败为 None。
    pub fps: Option<f64>,
}

/// 读出的压缩包（data 为拷贝，FFmpeg packet 复用前已 unref）。
pub struct RawPacket {
    pub pts: i64,
    pub dts: i64,
    pub keyframe: bool,
    pub data: Vec<u8>,
}

/// 打开的输入源：RAII 关闭输入并释放常驻 packet。
pub struct InputGuard {
    fmt: *mut AVFormatContext,
    pkt: *mut AVPacket,
    interrupt: std::boxed::Box<InterruptState>,
}

impl InputGuard {
    /// 刷新单段 IO deadline（连接建立后每轮读前调用，静默断流 5s 内暴露）。
    fn refresh_deadline(&self) {
        self.interrupt.deadline.set(Instant::now() + IO_DEADLINE);
    }

    /// 打开参数读回快照（flags/probesize/max_analyze_duration/max_delay，测试与排障证据）。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn open_option_state(&self) -> (i32, i64, i64, i32) {
        // SAFETY：fmt 来自 open 成功路径。
        unsafe {
            (
                (*self.fmt).flags,
                (*self.fmt).probesize,
                (*self.fmt).max_analyze_duration,
                (*self.fmt).max_delay,
            )
        }
    }
}

impl Drop for InputGuard {
    fn drop(&mut self) {
        if !self.pkt.is_null() {
            if let Ok(lib) = api() {
                // SAFETY：指针来自 packet_alloc 且未二次释放。
                unsafe { (lib.av_packet_free)(&mut self.pkt) };
            }
        }
        if !self.fmt.is_null() {
            if let Ok(lib) = api() {
                // SAFETY：指针来自 open_input 成功路径且未二次关闭。
                unsafe { (lib.avformat_close_input)(&mut self.fmt) };
            }
        }
    }
}

/// RTSP host:port 预解析：TcpStream 5s 探测（仅远程主机）。
/// FFmpeg Windows 构建的 tcp timeout 选项在 connect 阶段误报 ETIMEDOUT（实测），
/// connect 快速失败由 Rust 侧承担，FFmpeg 侧用 interrupt 回调管握手与读超时；
/// localhost 豁免探测：单会话 listen 源（测试用）会被探测连接占掉 accept。
fn probe_tcp_reachable(url: &str) -> Result<(), String> {
    let Some(rest) = url
        .strip_prefix("rtsp://")
        .or_else(|| url.strip_prefix("rtsps://"))
    else {
        return Ok(()); // 本地文件等非 RTSP 源不探测
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    // 容错 URL 内嵌凭据（契约禁止，但探测不应因此 panic）
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let default_port = if url.starts_with("rtsps://") {
        322
    } else {
        554
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !authority.ends_with(']') => (
            host.to_string(),
            port.parse::<u16>().map_err(|_| "端口非法".to_string())?,
        ),
        _ => (authority.to_string(), default_port),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost")
        || host.starts_with("127.")
        || host.eq_ignore_ascii_case("::1")
    {
        return Ok(());
    }
    use std::net::ToSocketAddrs;
    let mut addresses = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("主机解析失败: {e}"))?;
    let Some(address) = addresses.next() else {
        return Err("主机无可用地址".into());
    };
    std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5))
        .map_err(|e| format!("TCP 预连接失败: {e}"))?;
    Ok(())
}

/// 低延迟打开参数全集（复刻参考 read_frames input_params，子进程/进程内同源）：
/// fmt 层（fflags/probesize/analyzeduration/max_delay）+ rtsp 私有（transport/reorder_queue_size）。
/// 生效位置：fmt 级选项由 avformat_open_input 消费后写 fmt_ctx 字段；
/// reorder_queue_size 是 rtsp demuxer 私有选项，仅 RTSP 源消费。
const OPEN_OPTIONS: [(&str, &str); 6] = [
    ("rtsp_transport", "tcp"),
    ("fflags", "nobuffer"),
    ("probesize", "32"),
    ("analyzeduration", "0"),
    ("max_delay", "0"),
    ("reorder_queue_size", "0"),
];

/// 打开参数生效验证：fmt 层四项字段读回 + RTSP 专属项 dict 消费检查。
/// 返回 Err 表示选项未被 FFmpeg 消费（构建异常或版本行为变化，fail fast）。
fn verify_open_options(
    lib: &super::ffmpeg_lib::FfmpegApi,
    fmt_ctx: *mut AVFormatContext,
    dict: *mut AVDictionary,
    url: &str,
) -> Result<(), String> {
    // SAFETY：fmt_ctx 来自 open 成功路径；dict 释放前只读。
    let (flags, probesize, analyzed, max_delay) = unsafe {
        (
            (*fmt_ctx).flags,
            (*fmt_ctx).probesize,
            (*fmt_ctx).max_analyze_duration,
            (*fmt_ctx).max_delay,
        )
    };
    if flags & AVFMT_FLAG_NOBUFFER == 0 {
        return Err("低延迟选项未生效: fflags=nobuffer".into());
    }
    if probesize <= 0 || probesize > 32 {
        return Err(format!("低延迟选项未生效: probesize={probesize}"));
    }
    if analyzed > 0 {
        return Err(format!("低延迟选项未生效: analyzeduration={analyzed}"));
    }
    if max_delay != 0 {
        return Err(format!("低延迟选项未生效: max_delay={max_delay}"));
    }
    // rtsp 专属项：RTSP 源必须被 demuxer 消费（留在 dict = 未生效）；本地文件不认属预期
    let is_rtsp = url.starts_with("rtsp://") || url.starts_with("rtsps://");
    if is_rtsp {
        for key in ["rtsp_transport", "reorder_queue_size"] {
            let key_c = CString::new(key).map_err(|_| "选项键非法".to_string())?;
            // SAFETY：dict 为本函数创建的有效字典；返回项指向 dict 内部存储。
            let leftover = unsafe { (lib.av_dict_get)(dict, key_c.as_ptr(), std::ptr::null(), 0) };
            if !leftover.is_null() {
                return Err(format!("低延迟选项未被 RTSP 源消费: {key}"));
            }
        }
    }
    Ok(())
}

/// 打开 RTSP/文件源：TCP 传输 + 低延迟参数全集 + interrupt 中断（deadline/stop）。
/// `stop` 指向租约释放标志（可为 null：一次性调用无停机需求）。
pub fn open_input(url: &str, stop: *const AtomicBool) -> Result<InputGuard, String> {
    let lib = api().map_err(|c| c.to_string())?;
    network_init(lib);
    probe_tcp_reachable(url)?;
    let url_c = CString::new(url).map_err(|_| "URL 含非法字节".to_string())?;
    // interrupt 状态在 alloc 前建好（opaque 指向 Box 内容，移动安全）
    let interrupt = std::boxed::Box::new(InterruptState {
        deadline: std::cell::Cell::new(Instant::now() + IO_DEADLINE),
        stop,
    });
    // SAFETY：返回的 ctx 失败路径由 open_input 内部 free，成功路径由 guard 持有。
    let mut fmt_ctx = unsafe { (lib.avformat_alloc_context)() };
    if fmt_ctx.is_null() {
        return Err("avformat_alloc_context 失败".into());
    }
    // SAFETY：fmt_ctx 有效；opaque 指向本函数 Box（内容地址稳定）。
    unsafe {
        (*fmt_ctx).interrupt_callback = AVIOInterruptCB {
            callback: Some(interrupt_cb),
            opaque: std::ptr::from_ref(interrupt.as_ref())
                .cast_mut()
                .cast::<core::ffi::c_void>(),
        };
    }
    // SAFETY：dict 空初始；set 仅写入选项键值。
    let mut dict: *mut AVDictionary = std::ptr::null_mut();
    for (key, value) in OPEN_OPTIONS {
        let key = CString::new(key).map_err(|_| "选项键非法".to_string())?;
        let value = CString::new(value).map_err(|_| "选项值非法".to_string())?;
        // SAFETY：dict 指针有效，键值 NUL 结尾。
        let rc = unsafe { (lib.av_dict_set)(&mut dict, key.as_ptr(), value.as_ptr(), 0) };
        if rc != 0 {
            // SAFETY：dict 由本函数创建。
            unsafe { (lib.av_dict_free)(&mut dict) };
            return Err(format!("设置打开选项失败: {}", lib.errstr(rc)));
        }
    }
    // SAFETY：fmt_ctx 来自 alloc（失败时 FFmpeg 置 NULL 且内部已清理）；dict 为本函数创建。
    let rc = unsafe {
        (lib.avformat_open_input)(
            &mut fmt_ctx,
            url_c.as_ptr(),
            std::ptr::null(),
            std::ptr::from_mut(&mut dict).cast::<*mut core::ffi::c_void>(),
        )
    };
    if rc != 0 || fmt_ctx.is_null() {
        // SAFETY：dict 由本函数创建，失败路径同样释放。
        unsafe { (lib.av_dict_free)(&mut dict) };
        return Err(format!("打开输入失败: {}", lib.errstr(rc)));
    }
    if let Err(error) = verify_open_options(lib, fmt_ctx, dict, url) {
        // SAFETY：dict 由本函数创建；打开失败路径关回输入避免泄漏。
        unsafe { (lib.av_dict_free)(&mut dict) };
        unsafe { (lib.avformat_close_input)(&mut fmt_ctx) };
        return Err(error);
    }
    // SAFETY：dict 由本函数创建，验证完成后释放。
    unsafe { (lib.av_dict_free)(&mut dict) };
    // SAFETY：由 guard 释放。
    let pkt = unsafe { (lib.av_packet_alloc)() };
    if pkt.is_null() {
        return Err("av_packet_alloc 失败".into());
    }
    Ok(InputGuard {
        fmt: fmt_ctx,
        pkt,
        interrupt,
    })
}

impl InputGuard {
    /// 探测流信息并定位最佳视频流。
    pub fn probe_video(&self) -> Result<VideoStream, String> {
        let lib = api().map_err(|c| c.to_string())?;
        self.refresh_deadline();
        // SAFETY：fmt 有效。
        let rc = unsafe { (lib.avformat_find_stream_info)(self.fmt, std::ptr::null_mut()) };
        if rc != 0 {
            return Err(format!("流信息探测失败: {}", lib.errstr(rc)));
        }
        // SAFETY：fmt 有效；返回流序号或负错误码。
        let stream_index = unsafe {
            (lib.av_find_best_stream)(
                self.fmt,
                AVMEDIA_TYPE_VIDEO,
                -1,
                -1,
                std::ptr::null_mut(),
                0,
            )
        };
        if stream_index < 0 {
            return Err(format!("无视频流: {}", lib.errstr(stream_index)));
        }
        // SAFETY：stream_index 经 find_best_stream 校验在界内。
        let stream = unsafe { *(*self.fmt).streams.add(stream_index as usize) };
        if stream.is_null() {
            return Err("视频流指针无效".into());
        }
        // SAFETY：stream 由 avformat 管理，codecpar 在布局前部。
        let codecpar = unsafe { (*stream).codecpar };
        if codecpar.is_null() {
            return Err("流参数缺失".into());
        }
        // SAFETY：codecpar 有效；读取字段在已抄前缀内。
        let par = unsafe { &*codecpar };
        let extradata = if !par.extradata.is_null() && par.extradata_size > 0 {
            // SAFETY：extradata 指向 size 字节有效内存。
            unsafe {
                std::slice::from_raw_parts(par.extradata, par.extradata_size as usize).to_vec()
            }
        } else {
            Vec::new()
        };
        // fps 走 av_guess_frame_rate 官方 API（avg 优先回退 r_frame_rate）：
        // 手抄 AVStream 布局到该字段无法按 vendor 头文件核对（本地无 include），ABI 安全优先
        let rate = unsafe { (lib.av_guess_frame_rate)(self.fmt, stream, std::ptr::null_mut()) };
        let fps = (rate.den > 0 && rate.num >= 0).then(|| rate.num as f64 / rate.den as f64);
        Ok(VideoStream {
            stream_index,
            width: par.width,
            height: par.height,
            codec_id: par.codec_id,
            extradata,
            fps,
        })
    }

    /// 头直读全部视频流（批 1.3 编码探测）：SDP 源 open 后即可读 codecpar，
    /// 不触发 find_stream_info 全流分析（ffprobe 读流头语义）；非 SDP 源返回空由调用方回退。
    pub fn probe_streams_header(&self) -> Vec<VideoStream> {
        let Ok(lib) = api() else {
            return Vec::new();
        };
        // SAFETY：fmt 来自 open 成功路径；count 在 streams 数组界内。
        let count = unsafe { (*self.fmt).nb_streams } as usize;
        let streams = unsafe { (*self.fmt).streams };
        let mut out = Vec::new();
        for index in 0..count {
            // SAFETY：index < nb_streams。
            let stream = unsafe { *streams.add(index) };
            if stream.is_null() {
                continue;
            }
            // SAFETY：stream 由 avformat 管理，codecpar 在布局前部。
            let codecpar = unsafe { (*stream).codecpar };
            if codecpar.is_null() {
                continue;
            }
            // SAFETY：codecpar 有效；codec_type/codec_id/尺寸在已抄前缀内。
            let par = unsafe { &*codecpar };
            if par.codec_type != AVMEDIA_TYPE_VIDEO {
                continue;
            }
            let extradata = if !par.extradata.is_null() && par.extradata_size > 0 {
                // SAFETY：extradata 指向 size 字节有效内存。
                unsafe {
                    std::slice::from_raw_parts(par.extradata, par.extradata_size as usize).to_vec()
                }
            } else {
                Vec::new()
            };
            // SAFETY：stream 来自 streams 数组；帧率官方 API 读取。
            let rate = unsafe { (lib.av_guess_frame_rate)(self.fmt, stream, std::ptr::null_mut()) };
            let fps = (rate.den > 0 && rate.num >= 0).then(|| rate.num as f64 / rate.den as f64);
            out.push(VideoStream {
                stream_index: unsafe { (*stream).index },
                width: par.width,
                height: par.height,
                codec_id: par.codec_id,
                extradata,
                fps,
            });
        }
        out
    }

    /// 读下一个视频包；非视频流包跳过返回 None；EOF/网络错误返回 Err。
    pub fn read_video_packet(&mut self, stream_index: i32) -> Result<Option<RawPacket>, String> {
        let lib = api().map_err(|c| c.to_string())?;
        self.refresh_deadline();
        // SAFETY：fmt/pkt 有效。
        let rc = unsafe { (lib.av_read_frame)(self.fmt, self.pkt) };
        if rc != 0 {
            return Err(format!("读包失败: {}", lib.errstr(rc)));
        }
        // SAFETY：pkt 已由 read_frame 填充。
        let pkt = unsafe { &*self.pkt };
        if pkt.stream_index != stream_index {
            // SAFETY：包复用前重置。
            unsafe { (lib.av_packet_unref)(self.pkt) };
            return Ok(None);
        }
        let keyframe = pkt.flags & AV_PKT_FLAG_KEY != 0;
        let size = pkt.size.max(0) as usize;
        let data = if size > 0 && !pkt.data.is_null() {
            // SAFETY：data 指向 size 字节有效内存；拷贝后立即 unref 复用包。
            let bytes = unsafe { std::slice::from_raw_parts(pkt.data, size) }.to_vec();
            bytes
        } else {
            Vec::new()
        };
        let raw = RawPacket {
            pts: pkt.pts,
            dts: pkt.dts,
            keyframe,
            data,
        };
        // SAFETY：包复用前重置（data 已拷贝）。
        unsafe { (lib.av_packet_unref)(self.pkt) };
        Ok(Some(raw))
    }
}

/// 打开 RTSP/文件源并解码首个视频帧为 RGBA。
pub fn decode_first_frame(url: &str) -> Result<RgbaFrame, String> {
    let lib = api().map_err(|c| c.to_string())?;
    let mut input = open_input(url, std::ptr::null())?;
    let video = input.probe_video()?;
    // SAFETY：内建解码器按 codec_id 查找。
    let codec = unsafe { (lib.avcodec_find_decoder)(video.codec_id) };
    if codec.is_null() {
        return Err(format!("解码器缺失 codec_id={}", video.codec_id));
    }
    // SAFETY：probe 保证 codecpar 有效；fmt 布局前缀已核对。
    let stream = unsafe { *(*input.fmt).streams.add(video.stream_index as usize) };
    let codecpar = unsafe { (*stream).codecpar };
    struct CtxGuard(*mut AVCodecContext);
    impl Drop for CtxGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                let Ok(lib) = api() else { return };
                // SAFETY：指针来自 alloc_context3 且未二次释放。
                unsafe { (lib.avcodec_free_context)(&mut self.0) };
            }
        }
    }
    let ctx = CtxGuard(
        // SAFETY：codec 有效。
        unsafe { (lib.avcodec_alloc_context3)(codec) },
    );
    if ctx.0.is_null() {
        return Err("alloc_context3 失败".into());
    }
    // SAFETY：ctx/codecpar 均有效；参数拷贝由 FFmpeg 完成。
    let rc = unsafe { (lib.avcodec_parameters_to_context)(ctx.0, codecpar) };
    if rc != 0 {
        return Err(format!("参数拷贝失败: {}", lib.errstr(rc)));
    }
    if url.starts_with("rtsp") {
        apply_low_latency(lib, ctx.0)?;
    }
    // SAFETY：ctx 有效。
    let rc = unsafe { (lib.avcodec_open2)(ctx.0, codec, std::ptr::null_mut()) };
    if rc != 0 {
        return Err(format!("open2 失败: {}", lib.errstr(rc)));
    }
    struct FrameGuard(*mut AVFrame);
    impl Drop for FrameGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                let Ok(lib) = api() else { return };
                // SAFETY：指针来自 frame_alloc 且未二次释放。
                unsafe { (lib.av_frame_free)(&mut self.0) };
            }
        }
    }
    let frame = FrameGuard(
        // SAFETY：由 guard 释放。
        unsafe { (lib.av_frame_alloc)() },
    );
    if frame.0.is_null() {
        return Err("av_frame_alloc 失败".into());
    }
    // 常驻包（复用 decoder 同法）：av_packet_alloc 按真实布局清零，
    // 栈上构造手抄包缺尾部字段（opaque_ref 等）会解引用栈垃圾（2.2 已实证段错误根因）
    let pkt = PacketGuard(
        // SAFETY：由 guard 释放；alloc 清零尾部字段。
        unsafe { (lib.av_packet_alloc)() },
    );
    if pkt.0.is_null() {
        return Err("av_packet_alloc 失败".into());
    }
    // 读包循环：直到解出首帧
    loop {
        let raw = match input.read_video_packet(video.stream_index)? {
            Some(raw) => raw,
            None => continue,
        };
        if raw.data.is_empty() {
            continue; // 空包防御：NULL 包语义是 flush，不误触发
        }
        // SAFETY：pkt 来自 packet_alloc（真实布局+尾部字段清零）；data 在 send 同步期间存活。
        unsafe {
            (lib.av_packet_unref)(pkt.0);
            (*pkt.0).pts = raw.pts;
            (*pkt.0).dts = raw.dts;
            (*pkt.0).data = raw.data.as_ptr() as *mut u8;
            (*pkt.0).size = raw.data.len() as i32;
            (*pkt.0).stream_index = 0;
        }
        // SAFETY：ctx/pkt 有效。
        let rc = unsafe { (lib.avcodec_send_packet)(ctx.0, pkt.0) };
        // SAFETY：send 已完成（buf=NULL 路径 FFmpeg 已拷贝数据），复用前重置。
        unsafe { (lib.av_packet_unref)(pkt.0) };
        if rc != 0 && rc != AVERROR_EAGAIN {
            return Err(format!("send_packet 失败: {}", lib.errstr(rc)));
        }
        // SAFETY：frame 有效。
        let rc = unsafe { (lib.avcodec_receive_frame)(ctx.0, frame.0) };
        if rc == AVERROR_EAGAIN {
            continue;
        }
        if rc != 0 {
            return Err(format!("receive_frame 失败: {}", lib.errstr(rc)));
        }
        // 首帧：转 RGBA 返回
        // SAFETY：frame 已由 receive_frame 填充。
        let raw_frame = unsafe { &*frame.0 };
        let (w, h, fmt) = (raw_frame.width, raw_frame.height, raw_frame.format);
        let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
        let sws = unsafe {
            (lib.sws_get_context)(
                w,
                h,
                fmt,
                w,
                h,
                AV_PIX_FMT_RGBA,
                2,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if sws.is_null() {
            return Err("sws_getContext 失败".into());
        }
        let mut dst_ptr = rgba.as_mut_ptr();
        let dst_stride = [w * 4; 8];
        // SAFETY：源平面来自解码帧，目标缓冲长度匹配。
        let scaled = unsafe {
            (lib.sws_scale)(
                sws,
                raw_frame.data.as_ptr().cast::<*const u8>(),
                raw_frame.linesize.as_ptr(),
                0,
                h,
                std::ptr::from_mut(&mut dst_ptr),
                dst_stride.as_ptr(),
            )
        };
        // SAFETY：转换完成后释放 sws 上下文。
        unsafe { (lib.sws_free_context)(sws) };
        if scaled != h {
            return Err(format!("sws_scale 仅转换 {scaled}/{h} 行"));
        }
        return Ok(RgbaFrame {
            width: w,
            height: h,
            data: rgba,
        });
    }
}
