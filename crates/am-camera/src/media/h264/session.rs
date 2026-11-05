//! H.264 搬运会话（批 1.2）：独立 avformat 连接不解码直取包（禁 avcodec_send_packet），
//! av_read_frame 每包=一个访问单元（RTP marker/MP4 sample 边界）。
//! 分发原语（per-client 队列/GOP 缓存）见 fanout；本模块管会话生命周期与拉流线程。
//! 加锁顺序恒定：clients → config → gop → fan.queue，禁止逆序。
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::annexb::{self, ParameterSets};
use super::fanout::{ClientFan, ClientQueue, GopCache};

/// AV_CODEC_ID_H264（FFmpeg codec_id，对齐 rtsp_tests 用例）。
pub const AV_CODEC_ID_H264: i32 = 27;
/// GOP 缓存字节上限（超限整组弃置，下个关键帧重建）。
pub const GOP_CACHE_MAX_BYTES: usize = 4 * 1024 * 1024;
/// 单源客户端上限（DoS 防护，对齐既有管线）。
pub const MAX_CLIENTS_PER_SOURCE: usize = 64;
/// 辅助Rust枢纽的源/拉流工作上限，不代替C ABI Context额度。
pub const MAX_SOURCES_PER_HUB: usize = 16;
/// H.264 RTP 时间基固定 90kHz（AVStream.time_base 未抄入 FFI 布局）。
pub const TIME_BASE_DEN: u32 = 90_000;
/// 重连退避阶梯毫秒（对齐注册表拉流线程）。
const BACKOFF_STEPS_MS: [u64; 4] = [1000, 2000, 4000, 5000];
/// 无客户端宽限：订阅未就绪重试窗口；持续空转即停流防连接泄漏。
const IDLE_STOP: Duration = Duration::from_secs(10);
// 启动期停流上限：覆盖真机 RTSP 建立最坏耗时+数轮退避；到期停流，新订阅重启会话
const SESSION_STARTUP_LIMIT: Duration = Duration::from_secs(60);

/// 引用 URL → 认证 URL 解析器：凭据仅在拉流瞬间组装，不落状态/日志/错误。
pub type UrlResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// 流配置快照（h264_stream_config 内容源）。
#[derive(Clone, Debug)]
pub struct StreamConfig {
    pub width: i32,
    pub height: i32,
    pub fps: Option<f64>,
    pub generation: u32,
}

/// 一个访问单元（一帧）：Annex B 字节（关键帧含 SPS/PPS）+ 序号/PTS/代次。
#[derive(Debug)]
pub struct H264Packet {
    pub seq: u64,
    pub pts: i64,
    pub keyframe: bool,
    pub generation: u32,
    pub data: Arc<[u8]>,
}

/// 推给客户端的消息：包/配置/检测叠层/重同步信令/关闭。
#[derive(Debug)]
pub enum H264Message {
    Packet(Arc<H264Packet>),
    Config(Arc<StreamConfig>),
    /// 检测叠层控制帧（已序列化 JSON；无消费者即丢）
    Overlay(Arc<str>),
    ResyncRequired {
        reason: &'static str,
        generation: u32,
    },
    Closed(u16, Arc<str>),
}

/// 订阅被拒原因（WS 分诊用）。
#[derive(Debug)]
pub enum DenyReason {
    /// 会话连接中/尚无配置：客户端慢重试。
    NotReady,
    /// 单源客户端超上限。
    TooManyClients,
    /// 包含启动/停止中的源，不能用重连替代线程绕过上限。
    TooManySources,
    /// 主码流非 H.264（终态，含探测到的编码名）。
    CodecNotSupported(String),
    /// 认证失败（终态）：防海康连续失败锁机——退避重试会激化锁定，立即终态。
    AuthFailed,
}

/// 单源会话核心：拉流线程与订阅侧的共享状态。
struct SessionCore {
    url: String,
    clients: Mutex<HashMap<u64, Arc<ClientFan>>>,
    next_client: AtomicU64,
    next_seq: AtomicU64,
    gop: Mutex<GopCache>,
    config: Mutex<Option<Arc<StreamConfig>>>,
    generation: AtomicU32,
    param_sets: Mutex<ParameterSets>,
    /// 当前生效 SPS（带内变化检测用；拉流线程独占写）
    current_sps: Mutex<Vec<u8>>,
    ready: AtomicBool,
    stop: AtomicBool,
    terminal: Mutex<Option<DenyReason>>,
    thread_alive: AtomicBool,
    /// 客户端归零时刻（空转停流判据；拉起即空=Some(now)）
    clients_empty_since: Mutex<Option<std::time::Instant>>,
    /// 会话创建时刻：启动期（未就绪）停流上限判据，与空转期分离
    created: std::time::Instant,
    /// 未就绪期存活上限（默认 SESSION_STARTUP_LIMIT；测试经 new_with_limits 注入小值）
    startup_limit: Duration,
    /// 最后订阅意向时刻（P0 热修 2026-11-05）：未就绪期存活上限从"创建起算"
    /// 改为"最后意向起算"——被 NotReady 拒的持续重试是续命证据（RTSP 二次建立
    /// 慢/453 长退避不再被 60s 上限腰斩成重建死循环，真机实证：服务运行一段
    /// 时间后 NotReady 死循环、重启才恢复）；无意向拉起仍按上限自清防泄漏。
    last_interest: Mutex<std::time::Instant>,
    /// 诊断环形缓冲（最近 DIAG_CAPACITY 条 drop-oldest；宿主经 hub 快照轮询）
    diag: Mutex<std::collections::VecDeque<DiagEvent>>,
}

/// 拉流会话诊断事件（时间基准=会话创建时刻；宿主诊断页用）。
#[derive(Debug, Clone)]
pub struct DiagEvent {
    /// 距会话创建的毫秒数。
    pub elapsed_ms: u64,
    /// 事件类别（connect_attempt/connect_ok/connect_fail/probe_fail/
    /// auth_terminal/codec_terminal/bandwidth_backoff/ready/read_fail/
    /// stop_idle/stop_startup_limit/spawn_fail/thread_exit）。
    pub event: &'static str,
    /// 事件细节（错误串/尺寸/退避毫秒等）。
    pub detail: String,
}

/// 每会话诊断缓冲容量。
const DIAG_CAPACITY: usize = 32;

/// 会话概要（诊断页用；hub.session_states()）。
#[derive(Debug)]
pub struct SessionState {
    pub url: String,
    pub ready: bool,
    pub stop: bool,
    pub thread_alive: bool,
    pub generation: u32,
    /// 终态原因（auth_failed / codec:<名> / 无=非终态）。
    pub terminal: Option<String>,
}

/// 终态可读标签（诊断概要用；DenyReason 的 Display 语义面向分诊）。
fn terminal_label(reason: &DenyReason) -> String {
    match reason {
        DenyReason::NotReady => "not_ready".into(),
        DenyReason::TooManyClients => "too_many_clients".into(),
        DenyReason::TooManySources => "too_many_sources".into(),
        DenyReason::CodecNotSupported(codec) => format!("codec:{codec}"),
        DenyReason::AuthFailed => "auth_failed".into(),
    }
}

impl SessionCore {
    fn new(url: &str, gop_max_bytes: usize, startup_limit: Duration) -> Self {
        Self {
            url: url.to_string(),
            clients: Mutex::new(HashMap::new()),
            next_client: AtomicU64::new(1),
            next_seq: AtomicU64::new(0),
            gop: Mutex::new(GopCache::new(gop_max_bytes)),
            config: Mutex::new(None),
            generation: AtomicU32::new(0),
            param_sets: Mutex::new(ParameterSets::default()),
            current_sps: Mutex::new(Vec::new()),
            ready: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            terminal: Mutex::new(None),
            thread_alive: AtomicBool::new(false),
            clients_empty_since: Mutex::new(Some(std::time::Instant::now())),
            created: std::time::Instant::now(),
            startup_limit,
            last_interest: Mutex::new(std::time::Instant::now()),
            diag: Mutex::new(std::collections::VecDeque::with_capacity(DIAG_CAPACITY)),
        }
    }

    fn lock_clients(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Arc<ClientFan>>> {
        self.clients.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// 追加诊断事件（drop-oldest；锁中毒取内值继续，诊断不因毒锁失效）。
    fn diag_push(&self, event: &'static str, detail: impl Into<String>) {
        let entry = DiagEvent {
            elapsed_ms: self.created.elapsed().as_millis() as u64,
            event,
            detail: detail.into(),
        };
        let mut diag = self.diag.lock().unwrap_or_else(|p| p.into_inner());
        if diag.len() == DIAG_CAPACITY {
            diag.pop_front();
        }
        diag.push_back(entry);
    }
}

fn header_is_usable(codec_id: i32, width: i32, height: i32) -> bool {
    codec_id != 0 && (codec_id != AV_CODEC_ID_H264 || (width > 0 && height > 0))
}

#[cfg(test)]
mod header_tests {
    use super::header_is_usable;

    #[test]
    fn codec_classification_does_not_require_dimensions() {
        assert!(header_is_usable(7, 0, 0));
        assert!(header_is_usable(173, 0, 0));
        assert!(!header_is_usable(0, 0, 0));
        assert!(!header_is_usable(27, 0, 0));
        assert!(header_is_usable(27, 640, 360));
    }
}

/// 拉流线程主循环：解析凭据 URL → 打开（不解码）→ 编码核验 → 读包分发；
/// 失败退避重连（每次成功打开递增代次）；stop 即退出并自清。
fn pull_loop(core: &Arc<SessionCore>, resolver: &UrlResolver) {
    let mut backoff_idx = 0usize;
    'outer: loop {
        if core.stop.load(Ordering::Acquire) {
            break;
        }
        core.diag_push(
            "connect_attempt",
            format!("attempt={} backoff_idx={backoff_idx}", backoff_idx + 1),
        );
        // 每轮重连走解析链取认证 URL（链全 None 时按引用 URL 原样拉流）
        let url = resolver(&core.url).unwrap_or_else(|| core.url.clone());
        if core.stop.load(Ordering::Acquire) {
            break;
        }
        let connect_started = std::time::Instant::now();
        let mut input = match crate::media::rtsp::open_input(&url, std::ptr::from_ref(&core.stop)) {
            Ok(input) => input,
            Err(error) => {
                core.diag_push("connect_fail", error.clone());
                // 认证失败立即终态（AIeng V-R2 红线）：海康设备连续 401 会锁机
                // 30 分钟，退避重试等于持续撞锁；凭据修正后由新订阅复活会话。
                if error.contains("401") || error.to_lowercase().contains("unauthorized") {
                    core.diag_push("auth_terminal", error);
                    mark_terminal(core, DenyReason::AuthFailed);
                    break;
                }
                // RTSP 453（会话/带宽资源不足）：相机侧残留会话未清（后端重启后
                // 遗留），短退避反复撞锁会挤兑相机出口带宽（真机实证：突发攒批
                // 发送）。切长退避（30s）等相机自清。
                if error.contains("453") || error.contains("Not Enough Bandwidth") {
                    core.diag_push("bandwidth_backoff", "453: 30s long backoff");
                    if wait_or_stop(core, Duration::from_secs(30)) {
                        break;
                    }
                    continue 'outer;
                }
                if fail_and_backoff(core, error, &mut backoff_idx) {
                    break;
                }
                continue 'outer;
            }
        };
        core.diag_push(
            "connect_ok",
            format!("connect_ms={}", connect_started.elapsed().as_millis()),
        );
        // 流参数探测：头直读优先（ffprobe 语义不吞包；NOBUFFER 下 find_stream_info
        // 会消费并丢弃首包——MP4 单 IDR 流会因此永失关键帧）。头无尺寸（部分 SDP
        // 不带宽高）才回退 find_stream_info（RTSP 周期 IDR 场景可容忍首包损失）。
        let video = match input
            .probe_streams_header()
            .into_iter()
            .next()
            .filter(|video| header_is_usable(video.codec_id, video.width, video.height))
            .or_else(|| {
                input
                    .probe_video()
                    .ok()
                    .filter(|video| header_is_usable(video.codec_id, video.width, video.height))
            }) {
            Some(video) => video,
            None => {
                drop(input);
                core.diag_push("probe_fail", "no usable video stream");
                if fail_and_backoff(core, "无视频流".into(), &mut backoff_idx) {
                    break;
                }
                continue 'outer;
            }
        };
        if video.codec_id != AV_CODEC_ID_H264 {
            // 终态：非 H.264 不重试（相机重配后由新订阅复活会话）
            let codec = super::codec_probe::codec_display(video.codec_id);
            core.diag_push("codec_terminal", codec.clone());
            mark_terminal(core, DenyReason::CodecNotSupported(codec));
            break;
        }
        backoff_idx = 0;
        // 参数集（SDP sprop 或 avcC）与配置代次发布
        let sets = annexb::parse_parameter_sets(&video.extradata);
        *core.param_sets.lock().unwrap_or_else(|p| p.into_inner()) = sets.clone();
        *core.current_sps.lock().unwrap_or_else(|p| p.into_inner()) = sets.sps;
        reconfigure(core, video.width, video.height, video.fps);
        core.diag_push(
            "ready",
            format!(
                "startup_ms={} {}x{} fps={:?} gen={}",
                core.created.elapsed().as_millis(),
                video.width,
                video.height,
                video.fps,
                core.generation.load(Ordering::Acquire)
            ),
        );
        loop {
            if core.stop.load(Ordering::Acquire) {
                break 'outer;
            }
            match input.read_video_packet(video.stream_index) {
                Ok(Some(raw)) => process_packet(core, raw),
                Ok(None) => continue,
                Err(error) => {
                    drop(input);
                    core.diag_push("read_fail", error.clone());
                    if fail_and_backoff(core, error, &mut backoff_idx) {
                        break 'outer;
                    }
                    continue 'outer;
                }
            }
        }
    }
    // 退出原因区分（诊断三问之三：空闲回收是否参与）
    if core.ready.load(Ordering::Acquire) && !core.lock_clients().is_empty() {
        core.diag_push("thread_exit", "stop requested with clients");
    } else if core.ready.load(Ordering::Acquire) {
        core.diag_push("stop_idle", "idle timeout, no clients");
    } else if core.stop.load(Ordering::Acquire) {
        core.diag_push("thread_exit", "stop requested before ready");
    } else {
        core.diag_push("stop_startup_limit", "startup limit hit before ready");
    }
}

/// guard随线程函数真实返回/展开才清理，停止意图不能提前释放工作槽。
struct SessionWorker {
    hub: Arc<H264Hub>,
    core: Arc<SessionCore>,
}
impl Drop for SessionWorker {
    fn drop(&mut self) {
        self.core.stop.store(true, Ordering::Release);
        self.core.ready.store(false, Ordering::Release);
        {
            let clients = self.core.lock_clients();
            for fan in clients.values() {
                fan.queue
                    .close(H264Message::Closed(1001, Arc::from("media.sourceStopped")));
            }
        }
        self.core
            .gop
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        self.core
            .config
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        *self
            .core
            .param_sets
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = ParameterSets::default();
        *self
            .core
            .current_sps
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Vec::new();
        let mut sessions = self.hub.sessions.lock().unwrap_or_else(|p| p.into_inner());
        self.core.thread_alive.store(false, Ordering::Release);
        if sessions
            .get(&self.core.url)
            .is_some_and(|current| Arc::ptr_eq(current, &self.core))
            && self.core.lock_clients().is_empty()
        {
            sessions.remove(&self.core.url);
        }
    }
}

/// 单包处理：Annex B 归一 → 带内参数集变化检测（代次递增）→ 关键帧补齐参数集 → 分发。
fn process_packet(core: &Arc<SessionCore>, raw: crate::media::rtsp::RawPacket) {
    let Some(mut access_unit) = annexb::to_annexb(&raw.data) else {
        return; // 格式异常丢包（Annex B/AVCC 均不匹配）
    };
    if let Some(in_band) = annexb::first_nal_of_type(&access_unit, annexb::NAL_SPS) {
        let sps = in_band.to_vec();
        let pps = annexb::first_nal_of_type(&access_unit, annexb::NAL_PPS).map(|p| p.to_vec());
        let mut current = core.current_sps.lock().unwrap_or_else(|p| p.into_inner());
        if !current.is_empty() && *current != sps {
            // 中途参数集变化≈分辨率/码流变化：刷新参数集并递增代次
            let mut sets = core.param_sets.lock().unwrap_or_else(|p| p.into_inner());
            sets.sps = sps.clone();
            if let Some(pps) = pps {
                sets.pps = pps;
            }
            drop(sets);
            *current = sps;
            drop(current);
            let config = core
                .config
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            if let Some(config) = config {
                reconfigure(core, config.width, config.height, config.fps);
            }
        } else if current.is_empty() {
            *current = sps;
        }
    }
    let keyframe = raw.keyframe || annexb::has_idr_nalu(&access_unit);
    if keyframe {
        let sets = core
            .param_sets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        access_unit = annexb::ensure_parameter_sets(&access_unit, &sets);
    }
    let packet = Arc::new(H264Packet {
        seq: core.next_seq.fetch_add(1, Ordering::Relaxed) + 1,
        pts: raw.pts,
        keyframe,
        generation: core.generation.load(Ordering::Acquire),
        data: access_unit.into(),
    });
    publish_packet(core, packet);
    if idle_expired(core) {
        core.stop.store(true, Ordering::Release); // 空转停流（读循环下轮退出）
    }
}

/// 代次递增并广播新配置：清 GOP、客户端清残留等新代次关键帧。
fn reconfigure(core: &Arc<SessionCore>, width: i32, height: i32, fps: Option<f64>) {
    let generation = core.generation.fetch_add(1, Ordering::AcqRel) + 1;
    let config = Arc::new(StreamConfig {
        width,
        height,
        fps,
        generation,
    });
    let clients = core.lock_clients();
    *core.config.lock().unwrap_or_else(|p| p.into_inner()) = Some(config.clone());
    core.gop.lock().unwrap_or_else(|p| p.into_inner()).clear();
    for fan in clients.values() {
        fan.deliver_config(config.clone());
    }
    drop(clients);
    core.ready.store(true, Ordering::Release);
}

/// 包分发：GOP 缓存更新+逐客户端投递（clients 锁内保证订阅种子与活跃流无缺口）。
fn publish_packet(core: &Arc<SessionCore>, packet: Arc<H264Packet>) {
    let clients = core.lock_clients();
    core.gop
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .push(&packet);
    for fan in clients.values() {
        fan.deliver(H264Message::Packet(packet.clone()));
    }
}

/// 终态登记并通知在订客户端（编码不支持）。
fn mark_terminal(core: &Arc<SessionCore>, reason: DenyReason) {
    let closed = match &reason {
        DenyReason::CodecNotSupported(codec) => {
            Some((1008u16, format!("media.codecNotSupported|{codec}")))
        }
        DenyReason::AuthFailed => Some((1008u16, "media.sourceAuthFailed".to_string())),
        _ => None,
    };
    *core.terminal.lock().unwrap_or_else(|p| p.into_inner()) = Some(reason);
    core.ready.store(false, Ordering::Release);
    if let Some((code, reason)) = closed {
        let message: Arc<str> = Arc::from(reason.as_str());
        let clients = core.lock_clients();
        for fan in clients.values() {
            fan.queue.close(H264Message::Closed(code, message.clone()));
        }
    }
}

/// 失败退避等待；返回 true 表示等待期间收到 stop/空转超时应退出。
fn fail_and_backoff(core: &Arc<SessionCore>, _error: String, backoff_idx: &mut usize) -> bool {
    let wait_ms = BACKOFF_STEPS_MS[(*backoff_idx).min(BACKOFF_STEPS_MS.len() - 1)];
    *backoff_idx += 1;
    wait_or_stop(core, Duration::from_millis(wait_ms))
}
fn wait_or_stop(core: &SessionCore, duration: Duration) -> bool {
    let until = std::time::Instant::now() + duration;
    while std::time::Instant::now() < until {
        if core.stop.load(Ordering::Acquire) || idle_expired(core) {
            return true;
        }
        std::thread::sleep(
            until
                .saturating_duration_since(std::time::Instant::now())
                .min(Duration::from_millis(100)),
        );
    }
    core.stop.load(Ordering::Acquire) || idle_expired(core)
}

/// 空转超时：无客户端持续超宽限即停流（防 NotReady 放弃后连接泄漏）。
fn idle_expired(core: &SessionCore) -> bool {
    let startup_limit = core.startup_limit;
    // 启动期与空转期分离：真机 RTSP 建立（DESCRIBE/SETUP/PLAY+首包）秒级起步，
    // 拉起即空转 10s 会先于就绪触发停流，形成 NotReady 重连死循环（真机验证实证）。
    // 未就绪期按"最后订阅意向"判上限（P0 热修 2026-11-05：创建起算在 RTSP 二次
    // 建立慢/453 长退避场景仍会腰斩成重建死循环——消费方生产实证）；终态会话
    // （ready=false）同样走此分支，终态无订阅刷新即按上限清退。
    if !core.ready.load(Ordering::Acquire) {
        return core
            .last_interest
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .elapsed()
            > startup_limit;
    }
    if !core.lock_clients().is_empty() {
        return false;
    }
    core.clients_empty_since
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .is_some_and(|at| at.elapsed() > IDLE_STOP)
}

/// 搬运枢纽：每源一条拉流线程，订阅聚合复用；末客户端离开即停流。
pub struct H264Hub {
    resolver: UrlResolver,
    sessions: Mutex<HashMap<String, Arc<SessionCore>>>,
    /// 会话 GOP 缓存字节上限（测试可注入小值，生产走 GOP_CACHE_MAX_BYTES）。
    gop_max_bytes: usize,
    /// 未就绪期存活上限（生产 SESSION_STARTUP_LIMIT；测试注入小值加速语义验证）。
    startup_limit: Duration,
}

/// 全局搬运枢纽（HTTP WS 端点用；测试用 new() 局部实例）。
pub fn global(resolver: UrlResolver) -> &'static Arc<H264Hub> {
    static GLOBAL: std::sync::OnceLock<Arc<H264Hub>> = std::sync::OnceLock::new();
    GLOBAL.get_or_init(|| H264Hub::new(resolver, GOP_CACHE_MAX_BYTES))
}

impl H264Hub {
    pub fn new(resolver: UrlResolver, gop_max_bytes: usize) -> Arc<Self> {
        Self::new_with_limits(resolver, gop_max_bytes, SESSION_STARTUP_LIMIT)
    }

    /// 测试/诊断注入变体：未就绪期存活上限可调（语义验证用小值加速）。
    pub fn new_with_limits(
        resolver: UrlResolver,
        gop_max_bytes: usize,
        startup_limit: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            resolver,
            sessions: Mutex::new(HashMap::new()),
            gop_max_bytes,
            startup_limit,
        })
    }

    /// 会话是否就绪（配置已发布）。
    pub fn is_ready(&self, url: &str) -> bool {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(url)
            .is_some_and(|core| {
                core.ready.load(Ordering::Acquire)
                    && !core.stop.load(Ordering::Acquire)
                    && core.thread_alive.load(Ordering::Acquire)
            })
    }

    /// 诊断快照（P0 热修配套）：每源最近诊断事件（时间基准=会话创建；连接
    /// 尝试/成败与耗时/失败原因/退避/就绪/读失败/停流原因），宿主轮询展示。
    pub fn diag_snapshot(&self) -> Vec<(String, Vec<DiagEvent>)> {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|(url, core)| {
                (
                    url.clone(),
                    core.diag
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .iter()
                        .cloned()
                        .collect(),
                )
            })
            .collect()
    }

    /// 会话概要（诊断页一屏态）：就绪/停止/线程存活/终态原因/代次。
    pub fn session_states(&self) -> Vec<SessionState> {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|(url, core)| SessionState {
                url: url.clone(),
                ready: core.ready.load(Ordering::Acquire),
                stop: core.stop.load(Ordering::Acquire),
                thread_alive: core.thread_alive.load(Ordering::Acquire),
                generation: core.generation.load(Ordering::Acquire),
                terminal: core
                    .terminal
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .as_ref()
                    .map(terminal_label),
            })
            .collect()
    }

    /// 订阅：首个消息为 h264_stream_config，其后 GOP 缓存帧+活跃流。
    pub fn subscribe(self: &Arc<Self>, url: &str) -> Result<H264Subscription, DenyReason> {
        let core = self.ensure_session(url)?;
        // 订阅意向续命（P0 热修）：非终态会话每次订阅尝试刷新——NotReady 慢重试
        // 期间会话不因启动上限自杀重建（死循环根因）；终态不续（自清走原路径）。
        if core
            .terminal
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_none()
        {
            *core.last_interest.lock().unwrap_or_else(|p| p.into_inner()) =
                std::time::Instant::now();
        }
        let mut clients = core.lock_clients();
        if core.stop.load(Ordering::Acquire) || !core.thread_alive.load(Ordering::Acquire) {
            return Err(DenyReason::NotReady);
        }
        if let Some(DenyReason::CodecNotSupported(codec)) = core
            .terminal
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            return Err(DenyReason::CodecNotSupported(codec.clone()));
        }
        if clients.len() >= MAX_CLIENTS_PER_SOURCE {
            return Err(DenyReason::TooManyClients);
        }
        let Some(config) = core
            .config
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
        else {
            return Err(DenyReason::NotReady); // 连接中：客户端慢重试
        };
        let client_id = core.next_client.fetch_add(1, Ordering::Relaxed);
        let fan = Arc::new(ClientFan::new());
        let gop = core
            .gop
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .snapshot();
        fan.seed(&config, gop);
        clients.insert(client_id, fan.clone());
        *core
            .clients_empty_since
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        drop(clients);
        Ok(H264Subscription {
            hub: self.clone(),
            core: core.clone(),
            client_id,
            queue: fan.queue.clone(),
        })
    }

    /// 检测叠层注入（批③）：仅推给既有会话的订阅者；无会话=无消费者即丢
    /// （不建会话不拉流不积压）。检测 tick 高频数据不走事件总线的落点。
    pub fn publish_overlay(&self, url: &str, json: Arc<str>) {
        let Some(core) = self
            .sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(url)
            .cloned()
        else {
            return; // 无会话：无人在看该源，overlay 丢弃
        };
        for fan in core.lock_clients().values() {
            fan.deliver_overlay(json.clone());
        }
    }

    /// 取消订阅：末客户端离开置 stop（线程自行退出并自清，不阻塞订阅方）。
    fn unsubscribe(&self, core: &Arc<SessionCore>, client_id: u64) {
        let empty = {
            let mut clients = core.lock_clients();
            clients.remove(&client_id);
            if clients.is_empty() {
                *core
                    .clients_empty_since
                    .lock()
                    .unwrap_or_else(|p| p.into_inner()) = Some(std::time::Instant::now());
                core.stop.store(true, Ordering::Release);
                true
            } else {
                false
            }
        };
        if empty && !core.thread_alive.load(Ordering::Acquire) {
            let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
            if sessions
                .get(&core.url)
                .is_some_and(|current| Arc::ptr_eq(current, core))
            {
                sessions.remove(&core.url);
            }
        }
    }

    /// 建立或复用核心；停止中的原线程先退出，才允许新工作复活。
    fn ensure_session(self: &Arc<Self>, url: &str) -> Result<Arc<SessionCore>, DenyReason> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(core) = sessions.get(url) {
            if core.thread_alive.load(Ordering::Acquire) {
                return Ok(core.clone());
            }
        }
        if !sessions.contains_key(url) && sessions.len() >= MAX_SOURCES_PER_HUB {
            return Err(DenyReason::TooManySources);
        }
        let core = Arc::new(SessionCore::new(
            url,
            self.gop_max_bytes,
            self.startup_limit,
        ));
        core.thread_alive.store(true, Ordering::Release);
        sessions.insert(url.to_string(), core.clone());
        drop(sessions);
        let worker = SessionWorker {
            hub: self.clone(),
            core: core.clone(),
        };
        let resolver = self.resolver.clone();
        let spawned = std::thread::Builder::new()
            .name("h264-pull".into())
            .spawn(move || {
                let active_worker = worker;
                pull_loop(&active_worker.core, &resolver);
            });
        if spawned.is_err() {
            // spawn 失败按未就绪处理：订阅方 NotReady 慢重试
            return Err(DenyReason::NotReady);
        }
        Ok(core)
    }

    /// 测试锚点（批③）：登记无拉流会话并挂 N 个客户端（overlay 分发语义测试用，
    /// 不起拉流线程不碰网络）。
    #[cfg(test)]
    pub(crate) fn inject_clients(
        self: &Arc<Self>,
        url: &str,
        count: usize,
    ) -> Vec<Arc<ClientQueue>> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        let core = sessions.get(url).cloned().unwrap_or_else(|| {
            Arc::new(SessionCore::new(
                url,
                GOP_CACHE_MAX_BYTES,
                SESSION_STARTUP_LIMIT,
            ))
        });
        let fans: Vec<_> = (0..count).map(|_| Arc::new(ClientFan::new())).collect();
        let queues: Vec<_> = fans.iter().map(|fan| fan.queue.clone()).collect();
        {
            let mut clients = core.lock_clients();
            let base = core.next_client.load(Ordering::Relaxed);
            for (offset, fan) in fans.into_iter().enumerate() {
                clients.insert(base + offset as u64, fan);
            }
            core.next_client
                .store(base + count as u64, Ordering::Relaxed);
        }
        sessions.insert(url.to_string(), core);
        queues
    }
}

/// 订阅句柄：Drop 自动退订（末客户端离开停流）。
pub struct H264Subscription {
    hub: Arc<H264Hub>,
    core: Arc<SessionCore>,
    client_id: u64,
    queue: Arc<ClientQueue>,
}

impl H264Subscription {
    pub async fn recv(&self) -> Option<H264Message> {
        self.queue.recv().await
    }
}

impl Drop for H264Subscription {
    fn drop(&mut self) {
        self.hub.unsubscribe(&self.core, self.client_id);
    }
}

#[cfg(test)]
mod budget_tests;
