//! 编码探测与指引（批 1.3）：接入时读流头 codec_id（ffprobe 语义，不拉全流），
//! H.264 直连；非 H.264 分类返回（超时/认证失败/不支持），结果进程级缓存带有效期。
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::session::AV_CODEC_ID_H264;

/// 探测结果（detail 为脱敏诊断文本，不含 URL 与凭据）。
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeOutcome {
    /// 主流 H.264：可直连（携带流参数）
    H264 {
        width: i32,
        height: i32,
        fps: Option<f64>,
    },
    /// 非 H.264：需相机改配码流编码
    NotSupported { codec: String },
    /// 打开超时（IO deadline/连接超时）
    Timeout { detail: String },
    /// 认证失败（401/403）
    AuthFailed { detail: String },
    /// 网络不可达（拒绝/解析失败）
    Unreachable { detail: String },
    /// 其他失败
    Other { detail: String },
}

/// codec_id → 语言无关编码名（未知给数字串）。
pub fn codec_display(codec_id: i32) -> String {
    match codec_id {
        27 => "h264".into(),
        173 => "hevc".into(),
        7 => "mjpeg".into(),
        other => other.to_string(),
    }
}

/// 打开失败文本分类（纯函数，测试锚点）：认证→超时→不可达→其他，先命中先归类。
pub fn classify_open_failure(message: &str) -> ProbeOutcome {
    let detail = message.to_string();
    const AUTH_MARKERS: [&str; 4] = ["401", "Unauthorized", "403", "Forbidden"];
    const TIMEOUT_MARKERS: [&str; 6] = [
        "timed out",
        "timeout",
        "超时",
        "Immediate exit",
        "period of time",
        "10060",
    ];
    const UNREACHABLE_MARKERS: [&str; 6] = [
        "refused",
        "拒绝",
        "unreachable",
        "No route",
        "主机解析失败",
        "预连接失败",
    ];
    if AUTH_MARKERS.iter().any(|m| message.contains(m)) {
        return ProbeOutcome::AuthFailed { detail };
    }
    if TIMEOUT_MARKERS.iter().any(|m| message.contains(m)) {
        return ProbeOutcome::Timeout { detail };
    }
    if UNREACHABLE_MARKERS.iter().any(|m| message.contains(m)) {
        return ProbeOutcome::Unreachable { detail };
    }
    ProbeOutcome::Other { detail }
}

/// 流参数探测结果（复用 rtsp::VideoStream 字段）。
struct ProbedStream {
    codec_id: i32,
    width: i32,
    height: i32,
    fps: Option<f64>,
}

/// 探测一次：打开（头直读，SDP 源不触发全流分析；失败回退 find_stream_info）。
pub fn probe_once(url: &str) -> ProbeOutcome {
    let input = match crate::media::rtsp::open_input(url, std::ptr::null()) {
        Ok(input) => input,
        Err(error) => return classify_open_failure(&error),
    };
    let probed = input
        .probe_streams_header()
        .into_iter()
        .next()
        .map(|video| ProbedStream {
            codec_id: video.codec_id,
            width: video.width,
            height: video.height,
            fps: video.fps,
        })
        .or_else(|| {
            // 头直读无视频流（无 SDP 源）：退回有界流信息探测（probesize/analyzeduration 已收敛）
            input.probe_video().ok().map(|video| ProbedStream {
                codec_id: video.codec_id,
                width: video.width,
                height: video.height,
                fps: video.fps,
            })
        });
    match probed {
        Some(stream) if stream.codec_id == AV_CODEC_ID_H264 => ProbeOutcome::H264 {
            width: stream.width,
            height: stream.height,
            fps: stream.fps,
        },
        Some(stream) => ProbeOutcome::NotSupported {
            codec: codec_display(stream.codec_id),
        },
        None => ProbeOutcome::Other {
            detail: "无视频流".into(),
        },
    }
}

/// 探测结果有效期：成功长缓存，失败短缓存（相机重配/网络恢复后可重试）。
pub const PROBE_SUCCESS_TTL: Duration = Duration::from_secs(300);
pub const PROBE_FAILURE_TTL: Duration = Duration::from_secs(30);

/// 进程级探测缓存（键=sourceKey；值不落 URL/凭据）。
pub struct CodecProbeCache {
    entries: Mutex<HashMap<String, (Arc<ProbeOutcome>, Instant)>>,
    success_ttl: Duration,
    failure_ttl: Duration,
}

impl Default for CodecProbeCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CodecProbeCache {
    pub fn new() -> Self {
        Self::with_ttl(PROBE_SUCCESS_TTL, PROBE_FAILURE_TTL)
    }

    /// 测试注入短 TTL（有效期语义用例）。
    pub fn with_ttl(success_ttl: Duration, failure_ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            success_ttl,
            failure_ttl,
        }
    }

    fn fresh(&self, outcome: &ProbeOutcome, at: Instant, now: Instant) -> bool {
        let ttl = match outcome {
            ProbeOutcome::H264 { .. } => self.success_ttl,
            _ => self.failure_ttl,
        };
        now.duration_since(at) < ttl
    }

    /// 带有效期读缓存。
    pub fn get(&self, key: &str) -> Option<Arc<ProbeOutcome>> {
        let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        match entries.get(key) {
            Some((outcome, at)) if self.fresh(outcome, *at, now) => Some(outcome.clone()),
            Some(_) => {
                entries.remove(key); // 过期即清除
                None
            }
            None => None,
        }
    }

    /// 写缓存（测试与预热路径）。
    pub fn put(&self, key: &str, outcome: ProbeOutcome) {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(key.to_string(), (Arc::new(outcome), Instant::now()));
    }

    /// 探测入口：命中新鲜缓存直用，否则现场探测并缓存。
    /// url_provider 惰性解析认证 URL（凭据仅探测瞬间存在）。
    pub fn probe(&self, key: &str, url_provider: impl FnOnce() -> String) -> Arc<ProbeOutcome> {
        if let Some(cached) = self.get(key) {
            return cached;
        }
        let outcome = Arc::new(probe_once(&url_provider()));
        self.put(key, (*outcome).clone());
        outcome
    }
}

/// 全局进程级缓存（server 端点用）。
pub fn codec_probe_cache() -> &'static CodecProbeCache {
    static CACHE: OnceLock<CodecProbeCache> = OnceLock::new();
    CACHE.get_or_init(CodecProbeCache::new)
}
