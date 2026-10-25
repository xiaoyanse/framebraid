use super::*;
use crate::{
    decoder::config::{self, Config, Gate},
    sources::SharedSource,
};
use am_camera_core::media::{
    codec::{CpuDecoder, VideoCodec},
    sdk_io::{IoError, Reader, StreamInfo},
};
use std::thread::JoinHandle;
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
    interrupt: Mutex<Option<Arc<AtomicBool>>>,
    wait: Mutex<()>,
    alive: AtomicBool,
    handle: Mutex<Option<JoinHandle<()>>>,
}
impl Hub {
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
                    interrupt: Mutex::default(),
                    wait: Mutex::new(()),
                    alive: AtomicBool::new(false),
                    handle: Mutex::default(),
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
            let opened = Reader::open(
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
    fn consume(
        &self,
        source: &Arc<SharedSource>,
        reader: &mut Reader,
        stamp: SessionStamp,
    ) -> std::result::Result<(), IoError> {
        let old = reader.info();
        let _info_charge = source
            .state
            .charge(old.extradata.len() as u64, false)
            .map_err(|_| IoError::Resource)?;
        let info = StreamInfo {
            codec: old.codec,
            width: old.width,
            height: old.height,
            fps: old.fps,
            time_base_num: old.time_base_num,
            time_base_den: old.time_base_den,
            extradata: old.extradata.clone(),
        };
        let codec = match info.codec {
            AM_CODEC_H264 => Some(VideoCodec::H264),
            AM_CODEC_H265 => Some(VideoCodec::H265),
            AM_CODEC_MJPEG => None,
            _ => return Err(IoError::Unsupported),
        };
        let _initial_charge = source
            .state
            .charge(info.extradata.len() as u64, false)
            .map_err(|_| IoError::Resource)?;
        let mut cache_bytes = info.extradata.len() as u64;
        let mut cache_charge = source
            .state
            .charge(cache_bytes, false)
            .map_err(|_| IoError::Resource)?;
        let mut gate = if let Some(codec) = codec {
            Some(Gate::new(&configuration(codec, &info)).map_err(|_| IoError::Invalid)?)
        } else {
            None
        };
        if let Some(gate) = &mut gate {
            gate.dirty = false;
        }
        drop(_initial_charge);
        let mut decoder: Option<(CpuDecoder, crate::frames::DecoderPermit)> = None;
        let mut times = VecDeque::new();
        let _times_charge = source
            .state
            .charge(128 * 16, false)
            .map_err(|_| IoError::Resource)?;
        while self.current(source, stamp) {
            let Some(packet) = reader.read_packet()? else {
                return Err(IoError::Network);
            };
            if !self.current(source, stamp) {
                return Err(IoError::Closed);
            }
            let received = am_sdk_support::clock::monotonic_ms();
            let packet_sequence = sequence(&self.packet_sequence)?;
            let charge = source
                .state
                .charge(packet.data.len() as u64, false)
                .map_err(|_| IoError::Resource)?;
            let mut ready = true;
            let mut random = packet.keyframe;
            if let (Some(gate), Some(codec)) = (&mut gate, codec) {
                let _temporary = source
                    .state
                    .charge(packet.data.len() as u64, false)
                    .map_err(|_| IoError::Resource)?;
                ready = gate
                    .observe(codec, &packet.data)
                    .map_err(|_| IoError::Invalid)?;
                random = is_random(codec, &packet.data);
                if gate.dirty {
                    let _copy_charge = source
                        .state
                        .charge(cache_bytes.saturating_add(packet.data.len() as u64), false)
                        .map_err(|_| IoError::Resource)?;
                    let extra = gate.extra();
                    let new_charge = source
                        .state
                        .charge(extra.len() as u64, false)
                        .map_err(|_| IoError::Resource)?;
                    cache_charge = new_charge;
                    cache_bytes = extra.len() as u64;
                    source.config_generation.fetch_add(1, Ordering::AcqRel);
                    gate.dirty = false;
                    decoder.take();
                    self.reset_subscribers();
                    source.event(
                        AM_EVENT_SPATIAL_CHANGED,
                        AM_EVENT_SCOPE_ALL_CONSUMERS,
                        0,
                        AM_FAULT_KEYFRAME_WAIT as u32,
                        c"camera.codecConfigurationChanged",
                    );
                }
                if ready {
                    gate.started = true;
                }
            }
            let _ = &cache_charge;
            if !ready {
                continue;
            }
            let generation = stamp.generation;
            let config_generation = source.config_generation.load(Ordering::Acquire);
            let has_sets = has_parameter_sets(info.codec, &packet.data);
            let data = Arc::new(queue::Payload {
                abi: am_encoded_packet_t {
                    codec: info.codec,
                    bitstream_format: AM_BSF_ANNEXB,
                    flags: AM_PKT_AU_COMPLETE
                        | if random { AM_PKT_KEYFRAME } else { 0 }
                        | if has_sets { AM_PKT_PARAMSETS } else { 0 },
                    config_generation,
                    source_generation: generation,
                    source_id: source.source_id,
                    seq: packet_sequence,
                    pts: packet.pts,
                    dts: packet.dts,
                    time_base_num: info.time_base_num,
                    time_base_den: info.time_base_den,
                    size: packet.data.len() as u64,
                    ..Default::default()
                },
                bytes: packet.data,
                _charge: charge,
            });
            let subscribers = self.list();
            if !self.current(source, stamp) {
                return Err(IoError::Closed);
            }
            for sub in &subscribers {
                if matches!(sub.kind, Kind::Encoded { .. }) {
                    sub.enqueue(Item::Encoded(data.clone()));
                }
            }
            let frames = subscribers.iter().any(|s| s.wants_frames());
            if !frames {
                decoder.take();
                source.received(received);
                continue;
            }
            if let Some(codec) = codec {
                if decoder.is_none() {
                    if !random {
                        if source.fault.load(Ordering::Acquire) != AM_FAULT_RESOURCE as u32 {
                            source.report(
                                AM_SRC_LIVE as u32,
                                AM_FAULT_KEYFRAME_WAIT as u32,
                                c"camera.waitingKeyframe",
                            );
                        }
                        continue;
                    }
                    let permit = match source.state.decoder() {
                        Ok(permit) => permit,
                        Err(_) => {
                            source.report(
                                AM_SRC_LIVE as u32,
                                AM_FAULT_RESOURCE as u32,
                                c"camera.decoderBudgetExceeded",
                            );
                            continue;
                        }
                    };
                    let extra = gate.as_ref().map_or_else(Vec::new, Gate::extra);
                    let _extra_charge = source
                        .state
                        .charge(extra.len() as u64, false)
                        .map_err(|_| IoError::Resource)?;
                    let cpu = CpuDecoder::create(
                        config::runtime(&source.state).map_err(|_| IoError::Unsupported)?,
                        codec,
                        &extra,
                        (source
                            .state
                            .limits()
                            .map_err(|_| IoError::Resource)?
                            .frame_budget_bytes
                            / 4)
                        .min(8192 * 8192),
                    )
                    .map_err(|_| IoError::Unsupported)?;
                    decoder = Some((cpu, permit));
                }
                times.push_back((packet.pts, received));
                while times.len() > 128 {
                    times.pop_front();
                }
                if let Some((cpu, _permit)) = &mut decoder {
                    let mut sent = cpu
                        .send(&data.bytes, packet.pts, packet.dts)
                        .map_err(|_| IoError::Invalid)?;
                    self.emit_decoded(source, cpu, &info, &subscribers, &times, stamp)?;
                    if !sent {
                        sent = cpu
                            .send(&data.bytes, packet.pts, packet.dts)
                            .map_err(|_| IoError::Invalid)?;
                        self.emit_decoded(source, cpu, &info, &subscribers, &times, stamp)?;
                    }
                    if !sent {
                        return Err(IoError::Resource);
                    }
                }
            } else {
                let _encoded_charge = source
                    .state
                    .charge(data.bytes.len() as u64, false)
                    .map_err(|_| IoError::Resource)?;
                let max_pixels = (source
                    .state
                    .limits()
                    .map_err(|_| IoError::Resource)?
                    .frame_budget_bytes
                    / 4)
                .min(8192 * 8192);
                let header = am_camera_core::media::sdk_io::decode_image(
                    data.bytes.clone(),
                    max_pixels,
                    false,
                )?;
                let _raw_charge = match source
                    .state
                    .charge(header.width as u64 * header.height as u64 * 4, false)
                {
                    Ok(charge) => charge,
                    Err(_) => {
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
                let image =
                    am_camera_core::media::sdk_io::decode_image(header.encoded, max_pixels, true)?;
                if let Some(rgba) = image.rgba {
                    source.state.metrics.decoded.fetch_add(1, Ordering::AcqRel);
                    self.observe_dimensions(source, image.width, image.height);
                    if !self.current(source, stamp) {
                        return Err(IoError::Closed);
                    }
                    let frame_sequence = sequence(&self.frame_sequence)?;
                    pixels::fanout(
                        source,
                        &subscribers,
                        rgba,
                        image.width,
                        image.height,
                        packet.pts,
                        &info,
                        received,
                        frame_sequence,
                        stamp.generation,
                        source.config_generation.load(Ordering::Acquire),
                    )
                    .map_err(|_| IoError::Resource)?;
                }
            }
        }
        Err(IoError::Closed)
    }
    fn emit_decoded(
        &self,
        source: &Arc<SharedSource>,
        cpu: &mut CpuDecoder,
        info: &StreamInfo,
        subs: &[Arc<Subscription>],
        times: &VecDeque<(i64, i64)>,
        stamp: SessionStamp,
    ) -> std::result::Result<(), IoError> {
        loop {
            if !self.current(source, stamp) {
                return Err(IoError::Closed);
            }
            let Some(next) = cpu.peek().map_err(|_| IoError::Invalid)? else {
                break;
            };
            source.state.metrics.decoded.fetch_add(1, Ordering::AcqRel);
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
            let Some(frame) = cpu
                .copy_rgba(next.rgba_bytes)
                .map_err(|_| IoError::Invalid)?
            else {
                break;
            };
            let received = times
                .iter()
                .rev()
                .find(|(pts, _)| *pts == frame.info.pts)
                .map_or_else(am_sdk_support::clock::monotonic_ms, |(_, at)| *at);
            self.observe_dimensions(source, frame.info.width, frame.info.height);
            if !self.current(source, stamp) {
                return Err(IoError::Closed);
            }
            let seq = sequence(&self.frame_sequence)?;
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
