use super::*;
use crate::{
    decoder::config::{self, Config, Gate},
    reader::BudgetedReader,
    sources::SharedSource,
};
use am_camera_core::media::{
    codec::{CpuDecoder, VideoCodec},
    sdk_io::{IoError, StreamInfo},
};
use std::thread::JoinHandle;
mod consume;
mod native;
mod pipeline;
mod sampling;
#[cfg(test)]
mod tests;
#[derive(Clone, Copy)]
struct SessionStamp {
    revision: u64,
    generation: u32,
    credentials: u32,
}

pub struct Hub {
    pub(super) source: Weak<SharedSource>,
    pub(super) subscribers: Mutex<Vec<Weak<Subscription>>>,
    pub(super) codec: AtomicU32,
    pub(super) changed: Condvar,
    revision: AtomicU64,
    packet_sequence: AtomicU64,
    frame_sequence: AtomicU64,
    dimensions: Mutex<Option<(u32, u32, u32)>>,
    configuration: Mutex<()>,
    interrupt: Mutex<Option<Arc<AtomicBool>>>,
    wait: Mutex<()>,
    alive: AtomicBool,
    handle: Mutex<Option<JoinHandle<()>>>,
    #[cfg(test)]
    clock: Option<Arc<dyn Fn() -> Instant + Send + Sync>>,
    #[cfg(test)]
    pixel_block: Option<Arc<pipeline::Block>>,
}
impl Hub {
    fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(clock) = &self.clock {
            return clock();
        }
        Instant::now()
    }
    pub fn start(source: &Arc<SharedSource>) -> Result<()> {
        if source.isapi {
            return Ok(());
        }
        if source.stop.load(Ordering::Acquire) {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let mut slot = lock(&source.stream)?;
        let hub = slot
            .get_or_insert_with(|| {
                Arc::new(Self {
                    source: Arc::downgrade(source),
                    subscribers: Mutex::default(),
                    codec: AtomicU32::new(0),
                    changed: Condvar::new(),
                    revision: AtomicU64::new(0),
                    packet_sequence: AtomicU64::new(0),
                    frame_sequence: AtomicU64::new(0),
                    dimensions: Mutex::default(),
                    configuration: Mutex::new(()),
                    interrupt: Mutex::default(),
                    wait: Mutex::new(()),
                    alive: AtomicBool::new(false),
                    handle: Mutex::default(),
                    #[cfg(test)]
                    clock: None,
                    #[cfg(test)]
                    pixel_block: None,
                })
            })
            .clone();
        if hub.alive.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        if let Some(old) = lock(&hub.handle)?.take() {
            let _ = old.join();
        }
        let this = hub.clone();
        let thread = std::thread::Builder::new()
            .name("amc-shared-source".into())
            .spawn(move || {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| this.run()));
                if let Ok(mut token) = this.interrupt.lock() {
                    token.take();
                }
                this.alive.store(false, Ordering::Release);
                this.changed.notify_all();
            });
        match thread {
            Ok(handle) => {
                *lock(&hub.handle)? = Some(handle);
                Ok(())
            }
            Err(_) => {
                hub.alive.store(false, Ordering::Release);
                Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
            }
        }
    }
    pub fn credentials_changed(&self) {
        self.revision.fetch_add(1, Ordering::AcqRel);
        self.stop();
        self.reset_subscribers();
    }
    pub fn stop(&self) {
        if let Ok(token) = self.interrupt.lock() {
            if let Some(token) = &*token {
                token.store(true, Ordering::Release);
            }
        }
        self.changed.notify_all();
    }
    pub fn is_stopped(&self) -> bool {
        !self.alive.load(Ordering::Acquire)
    }
    pub fn queue_usage(&self) -> (u64, u64) {
        self.list().iter().fold((0, 0), |(items, bytes), sub| {
            let (i, b) = sub.queue_usage();
            (items + i, bytes + b)
        })
    }
    fn observe_dimensions(&self, source: &SharedSource, width: u32, height: u32) {
        let Ok(mut previous) = self.dimensions.lock() else {
            return;
        };
        let mut generation = source.config_generation.load(Ordering::Acquire);
        let changed = previous.is_some_and(|(w, h, _)| (w, h) != (width, height));
        if changed && previous.is_some_and(|(_, _, g)| g == generation) {
            generation = source.config_generation.fetch_add(1, Ordering::AcqRel) + 1;
        }
        *previous = Some((width, height, generation));
        drop(previous);
        if changed {
            self.reset_subscribers();
            source.event(
                AM_EVENT_SPATIAL_CHANGED,
                AM_EVENT_SCOPE_ALL_CONSUMERS,
                0,
                AM_FAULT_NONE as u32,
                c"camera.frameDimensionsChanged",
            );
        }
    }
    fn list(&self) -> Vec<Arc<Subscription>> {
        let Ok(mut subs) = self.subscribers.lock() else {
            return Vec::new();
        };
        subs.retain(|s| s.upgrade().is_some_and(|s| s.occupies_slot()));
        subs.iter()
            .filter_map(Weak::upgrade)
            .filter(|s| !s.stopped())
            .collect()
    }
    fn reset_subscribers(&self) {
        for sub in self.list() {
            sub.reset();
        }
    }
    fn pause(&self, source: &SharedSource, revision: u64, duration: Duration) {
        let until = Instant::now() + duration;
        while !source.stop.load(Ordering::Acquire)
            && revision == self.revision.load(Ordering::Acquire)
            && Instant::now() < until
        {
            let Ok(wait) = self.wait.lock() else {
                break;
            };
            let _ = self.changed.wait_timeout(
                wait,
                until
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(100)),
            );
        }
    }
    fn run(&self) {
        let mut attempts = 0u32;
        let mut ever_connected = false;
        let mut previous_revision = self.revision.load(Ordering::Acquire);
        loop {
            let Some(source) = self.source.upgrade() else {
                break;
            };
            if source.stop.load(Ordering::Acquire) {
                break;
            }
            let revision = self.revision.load(Ordering::Acquire);
            let mut stamp = SessionStamp {
                revision,
                generation: source.generation.load(Ordering::Acquire),
                credentials: source.credential_version(),
            };
            let secret = match source.secret() {
                Ok(secret) => secret,
                Err(_) => {
                    self.pause(&source, revision, Duration::from_millis(100));
                    continue;
                }
            };
            if !self.current(&source, stamp) {
                continue;
            }
            let api = match config::runtime(&source.state) {
                Ok(api) => api,
                Err(code) => {
                    source.runtime_unavailable(code);
                    source.report(
                        AM_SRC_RECONNECT_WAIT as u32,
                        AM_FAULT_RESOURCE as u32,
                        c"camera.runtimeUnavailable",
                    );
                    self.pause(&source, revision, Duration::from_secs(1));
                    continue;
                }
            };
            let token = Arc::new(AtomicBool::new(false));
            if let Ok(mut slot) = self.interrupt.lock() {
                *slot = Some(token.clone());
            }
            if source.stop.load(Ordering::Acquire)
                || revision != self.revision.load(Ordering::Acquire)
            {
                continue;
            }
            source.report(
                AM_SRC_CONNECTING as u32,
                AM_FAULT_NONE as u32,
                c"camera.connecting",
            );
            let opened = BudgetedReader::open(
                source.state.clone(),
                api,
                &source.url,
                &secret,
                token.clone(),
                Duration::from_secs(5),
            );
            let result = match opened {
                Ok(mut reader) => {
                    if !self.current(&source, stamp) {
                        continue;
                    }
                    if ever_connected && revision == previous_revision {
                        let Some(next) = stamp.generation.checked_add(1) else {
                            break;
                        };
                        if source
                            .generation
                            .compare_exchange(
                                stamp.generation,
                                next,
                                Ordering::AcqRel,
                                Ordering::Acquire,
                            )
                            .is_err()
                        {
                            continue;
                        }
                        stamp.generation = next;
                        source
                            .state
                            .metrics
                            .reconnects
                            .fetch_add(1, Ordering::AcqRel);
                        source.event(
                            AM_EVENT_GENERATION_CHANGED,
                            AM_EVENT_SCOPE_ALL_CONSUMERS,
                            0,
                            AM_FAULT_NONE as u32,
                            c"camera.reconnected",
                        );
                    }
                    previous_revision = revision;
                    ever_connected = true;
                    attempts = 0;
                    source.reconnect_attempts.store(0, Ordering::Release);
                    self.codec.store(reader.info().codec, Ordering::Release);
                    source.report(
                        AM_SRC_LIVE as u32,
                        AM_FAULT_KEYFRAME_WAIT as u32,
                        c"camera.waitingKeyframe",
                    );
                    self.consume(&source, &mut reader, stamp)
                }
                Err(error) => Err(error),
            };
            self.reset_subscribers();
            if source.stop.load(Ordering::Acquire) {
                break;
            }
            if !self.current(&source, stamp) {
                continue;
            }
            let error = result.err().unwrap_or(IoError::Network);
            if error == IoError::Auth {
                source.report(
                    AM_SRC_AUTH_WAITING as u32,
                    AM_FAULT_AUTH as u32,
                    c"camera.authenticationRejected",
                );
                while !source.stop.load(Ordering::Acquire)
                    && revision == self.revision.load(Ordering::Acquire)
                {
                    self.pause(&source, revision, Duration::from_millis(100));
                }
                continue;
            }
            let fault = match error {
                IoError::Resource => AM_FAULT_RESOURCE,
                IoError::Unsupported | IoError::Invalid => AM_FAULT_DECODE,
                _ => AM_FAULT_NETWORK,
            };
            source.report(
                AM_SRC_RECONNECT_WAIT as u32,
                fault as u32,
                c"camera.streamInterrupted",
            );
            attempts = attempts.saturating_add(1);
            source.reconnect_attempts.store(attempts, Ordering::Release);
            let policy = match source.reconnect.lock() {
                Ok(p) => *p,
                Err(_) => break,
            };
            if policy.max_attempts != 0 && attempts >= policy.max_attempts {
                while !source.stop.load(Ordering::Acquire)
                    && revision == self.revision.load(Ordering::Acquire)
                {
                    self.pause(&source, revision, Duration::from_millis(100));
                }
                attempts = 0;
                continue;
            }
            let backoff = (policy.initial_backoff_ms as u64)
                .saturating_mul(1u64 << attempts.saturating_sub(1).min(16))
                .min(policy.max_backoff_ms as u64);
            // 抖动按源身份与重试号计算，避免同机所有相机同时重连。
            let jitter = if policy.jitter_ms == 0 {
                0
            } else {
                source
                    .source_id
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(attempts as u64)
                    % (policy.jitter_ms as u64 + 1)
            };
            self.pause(
                &source,
                revision,
                Duration::from_millis(backoff.saturating_add(jitter).min(60_000)),
            );
        }
        self.reset_subscribers();
    }
    #[cfg(test)]
    fn emit_decoded<const EARLY_SAMPLING: bool>(
        &self,
        source: &Arc<SharedSource>,
        cpu: &mut crate::decoder::budget::BudgetedDecoder,
        info: &StreamInfo,
        subs: &[Arc<Subscription>],
        times: &VecDeque<(i64, i64)>,
        stamp: SessionStamp,
    ) -> std::result::Result<(), IoError> {
        self.emit_decoded_checked::<EARLY_SAMPLING>(source, cpu, info, subs, times, stamp, None)
    }
    #[allow(clippy::too_many_arguments)]
    fn emit_decoded_checked<const EARLY_SAMPLING: bool>(
        &self,
        source: &Arc<SharedSource>,
        cpu: &mut crate::decoder::budget::BudgetedDecoder,
        info: &StreamInfo,
        subs: &[Arc<Subscription>],
        times: &VecDeque<(i64, i64)>,
        stamp: SessionStamp,
        mut delivery: Option<&mut pipeline::Delivery<'_>>,
    ) -> std::result::Result<(), IoError> {
        loop {
            if !self.current(source, stamp) || delivery.as_ref().is_some_and(|d| !d.current(source))
            {
                return Err(IoError::Closed);
            }
            let Some(next) = cpu.peek().map_err(|_| IoError::Invalid)? else {
                break;
            };
            {
                let _guard = lock(&self.configuration).map_err(|_| IoError::Resource)?;
                if !self.current(source, stamp)
                    || delivery.as_ref().is_some_and(|d| !d.current(source))
                {
                    return Err(IoError::Closed);
                }
                let mut hardware = lock(&source.decode).map_err(|_| IoError::Resource)?;
                hardware.actual = Some(match cpu.hardware_backend() {
                    Some(am_camera_core::media::hardware::Backend::D3d11va) => "ffmpeg-d3d11va",
                    Some(am_camera_core::media::hardware::Backend::Dxva2) => "ffmpeg-dxva2",
                    None => "ffmpeg-cpu",
                });
                hardware.active = true;
                hardware.phase = if hardware.recovered {
                    "recovered_cpu"
                } else {
                    "verified"
                };
            }
            source.state.metrics.decoded.fetch_add(1, Ordering::AcqRel);
            let received = times
                .iter()
                .rev()
                .find(|(pts, _)| *pts == next.pts)
                .map_or_else(am_sdk_support::clock::monotonic_ms, |(_, at)| *at);
            {
                let _guard = lock(&self.configuration).map_err(|_| IoError::Resource)?;
                if delivery.as_ref().is_some_and(|d| !d.current(source)) {
                    return Err(IoError::Closed);
                }
                self.observe_dimensions(source, next.width, next.height);
                if let Some(d) = delivery.as_mut() {
                    d.config = source.config_generation.load(Ordering::Acquire);
                }
            }
            // 序号仍随解码帧推进，不能因省略颜色转换而隐藏采样间隙。
            let seq = sequence(&self.frame_sequence)?;
            if delivery
                .as_ref()
                .is_some_and(|d| sampling::superseded(subs, d))
            {
                cpu.discard_frame().map_err(|_| IoError::Invalid)?;
                source
                    .state
                    .metrics
                    .frames_dropped
                    .fetch_add(1, Ordering::AcqRel);
                continue;
            }
            if EARLY_SAMPLING && !sampling::ready_soon(subs, self.now(), delivery.is_some()) {
                cpu.discard_frame().map_err(|_| IoError::Invalid)?;
                source.received(received);
                continue;
            }
            let _charge = match source.state.charge(next.rgba_bytes as u64, false) {
                Ok(charge) => charge,
                Err(_) => {
                    cpu.discard_frame().map_err(|_| IoError::Invalid)?;
                    source
                        .state
                        .metrics
                        .frames_dropped
                        .fetch_add(1, Ordering::AcqRel);
                    source.report(
                        AM_SRC_LIVE as u32,
                        AM_FAULT_RESOURCE as u32,
                        c"camera.outputFrameBudgetExceeded",
                    );
                    continue;
                }
            };
            let frame =
                match crate::decoder::budget::copy_pixels(&source.state, cpu, next.rgba_bytes, 0) {
                    Ok(Some(frame)) => frame,
                    Ok(None) => break,
                    Err(AM_ERR_SDK_RESOURCE_EXHAUSTED) => {
                        cpu.discard_frame().map_err(|_| IoError::Invalid)?;
                        source
                            .state
                            .metrics
                            .frames_dropped
                            .fetch_add(1, Ordering::AcqRel);
                        source.report(
                            AM_SRC_LIVE as u32,
                            AM_FAULT_RESOURCE as u32,
                            c"camera.readbackBudgetExceeded",
                        );
                        continue;
                    }
                    Err(_) => return Err(IoError::Invalid),
                };
            if let Some(d) = delivery.as_ref() {
                if !sampling::wait_due(subs, d)? {
                    continue;
                }
            }
            let _guard = lock(&self.configuration).map_err(|_| IoError::Resource)?;
            if !self.current(source, stamp) || delivery.as_ref().is_some_and(|d| !d.current(source))
            {
                return Err(IoError::Closed);
            }
            pixels::fanout(
                source,
                subs,
                frame.data,
                frame.info.width,
                frame.info.height,
                frame.info.pts,
                info,
                received,
                seq,
                stamp.generation,
                source.config_generation.load(Ordering::Acquire),
                self.now(),
            )
            .map_err(|_| IoError::Resource)?;
        }
        Ok(())
    }
    fn current(&self, source: &SharedSource, stamp: SessionStamp) -> bool {
        !source.stop.load(Ordering::Acquire)
            && stamp.revision == self.revision.load(Ordering::Acquire)
            && stamp.generation == source.generation.load(Ordering::Acquire)
            && stamp.credentials == source.credential_version()
    }
}
fn sequence(counter: &AtomicU64) -> std::result::Result<u64, IoError> {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
        .map(|n| n + 1)
        .map_err(|_| IoError::Resource)
}
fn configuration(codec: VideoCodec, info: &StreamInfo) -> Config {
    Config {
        abi: am_decoder_config_t {
            codec: info.codec,
            bitstream_format: AM_BSF_ANNEXB,
            output_pixel_format: AM_PF_RGBA8 as u32,
            time_base_num: info.time_base_num,
            time_base_den: info.time_base_den,
            ..Default::default()
        },
        extra: info.extradata.clone(),
        codec,
    }
}
fn is_random(codec: VideoCodec, bytes: &[u8]) -> bool {
    am_camera_core::media::codec::access_units(codec, bytes).is_ok_and(|units| {
        units.iter().any(|n| {
            if codec == VideoCodec::H264 {
                n[0] & 31 == 5
            } else {
                (16..=21).contains(&((n[0] >> 1) & 63))
            }
        })
    })
}
fn has_parameter_sets(codec: u32, bytes: &[u8]) -> bool {
    let codec = match codec {
        AM_CODEC_H264 => VideoCodec::H264,
        AM_CODEC_H265 => VideoCodec::H265,
        _ => return false,
    };
    am_camera_core::media::codec::access_units(codec, bytes).is_ok_and(|units| {
        let mut mask = 0;
        for n in units {
            mask |= if codec == VideoCodec::H264 {
                match n[0] & 31 {
                    7 => 2,
                    8 => 4,
                    _ => 0,
                }
            } else {
                match (n[0] >> 1) & 63 {
                    32 => 1,
                    33 => 2,
                    34 => 4,
                    _ => 0,
                }
            };
        }
        mask == if codec == VideoCodec::H264 { 6 } else { 7 }
    })
}
