use super::frames::{camera_state, CameraState, Charge, DecoderPermit, Frame};
use super::*;
use am_camera_core::media::codec::{access_units, CpuDecoder, DecodeError, VideoCodec};
use am_sdk_support::{args, clock, context::Context, handles::Managed, Result};
use std::{
    any::Any,
    collections::VecDeque,
    mem::size_of,
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
pub(crate) mod budget;
pub(crate) mod config;
mod diagnostics;
use config::{convert, runtime, Config, Gate};

struct Packet {
    bytes: Vec<u8>,
    valid: usize,
    pts: i64,
    tag: i64,
    received: i64,
    _charge: Charge,
}
struct Inner {
    metrics: Arc<super::frames::CameraMetrics>,
    cpu: budget::BudgetedDecoder,
    config: Config,
    gate: Gate,
    queue: VecDeque<Packet>,
    queue_bytes: u64,
    flushed: bool,
    timings: VecDeque<Timing>,
}
struct Timing {
    tag: i64,
    pts: i64,
    received: i64,
    config: u32,
    delivered: bool,
}
struct Decoder {
    inner: Mutex<Option<Inner>>,
    state: AtomicU32,
    generation: AtomicU32,
    config_generation: AtomicU32,
    source: u64,
    sequence: AtomicU64,
    ctx: Arc<CameraState>,
    permit: Mutex<Option<DecoderPermit>>,
    codec_info: Mutex<am_decoder_info_t>,
}
// info中的backend_name为静态字符串，配置指针已清除，原生上下文只在inner锁内访问。
unsafe impl Send for Decoder {}
unsafe impl Sync for Decoder {}
impl Managed for Decoder {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn request_close(&self) {
        self.state.store(AM_DEC_CLOSING as u32, Ordering::Release);
    }
    fn close_resources(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.take();
        }
        if let Ok(mut permit) = self.permit.lock() {
            permit.take();
        }
        self.state.store(AM_DEC_CLOSED as u32, Ordering::Release);
    }
}
static SOURCE_ID: AtomicU64 = AtomicU64::new(0);
static PACKET_ID: AtomicU64 = AtomicU64::new(0);
fn next(sequence: &AtomicU64) -> Result<u64> {
    sequence
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
        .map(|n| n + 1)
        .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)
}
fn info(c: &Config, generation: u32) -> am_decoder_info_t {
    am_decoder_info_t {
        codec: c.abi.codec,
        bitstream_format: c.abi.bitstream_format,
        output_pixel_format: c.abi.output_pixel_format,
        hw_policy: c.abi.hw_policy,
        hw_accelerated: 0,
        backend_name_utf8: c"ffmpeg-cpu".as_ptr(),
        config_generation: generation,
        ..Default::default()
    }
}
fn make_inner(ctx: &CameraState, cfg: Config) -> Result<Inner> {
    let l = ctx.limits()?;
    let pixels = (l.frame_budget_bytes / 4).min(SDK_MAX_DYNAMIC_AXIS * SDK_MAX_DYNAMIC_AXIS);
    if pixels == 0 {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    let cpu = CpuDecoder::create(runtime(ctx)?, cfg.codec, &cfg.extra, pixels).map_err(convert)?;
    let gate = Gate::new(&cfg)?;
    Ok(Inner {
        metrics: ctx.metrics.clone(),
        cpu: cpu.into(),
        config: cfg,
        gate,
        queue: VecDeque::new(),
        queue_bytes: 0,
        flushed: false,
        timings: VecDeque::new(),
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_decoder_create(
    ctx: *mut amc_context_t,
    cfg: *const am_decoder_config_t,
    out: *mut *mut amc_decoder_t,
) -> i32 {
    status(domain(), ctx as usize, || {
        am_sdk_support::args::pointer(out)?;
        unsafe {
            out.write(std::ptr::null_mut());
        }
        if !cfg!(all(windows, target_arch = "x86_64")) {
            return Err(AM_ERR_SDK_NOT_SUPPORTED);
        }
        let _active = domain().active(ctx as usize)?;
        let state = camera_state(ctx as usize)?;
        let permit = state.decoder()?;
        let config = unsafe { Config::read(cfg) }?;
        let codec_info = Mutex::new(info(&config, 1));
        let inner = make_inner(&state, config)?;
        let object = Arc::new(Decoder {
            inner: Mutex::new(Some(inner)),
            state: AtomicU32::new(AM_DEC_READY as u32),
            generation: AtomicU32::new(1),
            config_generation: AtomicU32::new(1),
            source: next(&SOURCE_ID)?,
            sequence: AtomicU64::new(0),
            ctx: state,
            permit: Mutex::new(Some(permit)),
            codec_info,
        });
        let key = domain().insert(ctx as usize, object)?;
        unsafe {
            out.write(key as _);
        }
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_decoder_submit(
    dec: *mut amc_decoder_t,
    pkt: *const am_decoder_input_t,
) -> i32 {
    status(domain(), dec as usize, || {
        let entry = domain().active(dec as usize)?;
        let decoder = entry.data::<Decoder>()?;
        if decoder.state.load(Ordering::Acquire) != AM_DEC_READY as u32 {
            return Err(AM_ERR_CAMERA_DECODER_STATE_INVALID);
        }
        let packet = unsafe { args::read(pkt, size_of::<am_decoder_input_t>()) }?;
        if packet.flags & !7 != 0 || packet.flags & AM_PKT_AU_COMPLETE == 0 {
            return Err(AM_ERR_CAMERA_CODEC_UNSUPPORTED);
        }
        if packet.size == 0 || packet.size > SDK_DECODER_INPUT_BYTES - 64 {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        if packet.data.is_null() {
            return Err(AM_ERR_SDK_NULL_ARGUMENT);
        }
        let mut locked = decoder
            .inner
            .try_lock()
            .map_err(|_| AM_ERR_SDK_QUEUE_FULL)?;
        let inner = locked.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        if decoder.state.load(Ordering::Acquire) != AM_DEC_READY as u32 {
            return Err(AM_ERR_CAMERA_DECODER_STATE_INVALID);
        }
        if packet.pts != i64::MIN && inner.config.abi.time_base_den == 0 {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        if inner.queue.len() >= inner.config.abi.queue_max_items as usize
            || packet.size + 64
                > inner
                    .config
                    .abi
                    .queue_max_bytes
                    .saturating_sub(inner.queue_bytes)
        {
            return Err(AM_ERR_SDK_QUEUE_FULL);
        }
        let data = unsafe { std::slice::from_raw_parts(packet.data, packet.size as usize) };
        access_units(inner.config.codec, data).map_err(convert)?;
        let charge = decoder.ctx.charge(packet.size + 64, false)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(data.len() + 64)
            .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
        bytes.extend_from_slice(data);
        bytes.resize(data.len() + 64, 0);
        inner.queue.push_back(Packet {
            bytes,
            valid: data.len(),
            pts: packet.pts,
            tag: i64::try_from(next(&PACKET_ID)?).map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?,
            received: clock::monotonic_ms(),
            _charge: charge,
        });
        inner.queue_bytes += packet.size + 64;
        inner
            .metrics
            .decoder_queue_items
            .fetch_add(1, Ordering::AcqRel);
        inner
            .metrics
            .decoder_queue_bytes
            .fetch_add(packet.size + 64, Ordering::AcqRel);
        Ok(())
    })
}
fn receive(decoder: &Decoder, inner: &mut Inner) -> Result<Option<Arc<Frame>>> {
    loop {
        match inner.cpu.peek() {
            Ok(Some(f)) => {
                // 帧内保存内部唯一包号，再恢复原PTS；重复/未知PTS不能串到别帧。
                let timing = inner
                    .timings
                    .iter_mut()
                    .find(|t| t.tag == f.pts)
                    .ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
                let (pts, received, config) = (timing.pts, timing.received, timing.config);
                timing.delivered = true;
                let format = inner.config.abi.output_pixel_format;
                let bytes = super::frames::scale::bytes(f.width, f.height, format)?;
                let charge = decoder.ctx.charge(bytes as u64, true)?;
                // NV12经有预算的RGB中间帧规范化为601 limited，不冒充源流色彩元数据。
                let scratch = if format == AM_PF_NV12 as u32 {
                    Some(decoder.ctx.charge(f.rgba_bytes as u64, false)?)
                } else {
                    None
                };
                let decoded = budget::copy_pixels(
                    &decoder.ctx,
                    &mut inner.cpu,
                    if scratch.is_some() {
                        f.rgba_bytes
                    } else {
                        bytes
                    },
                    if scratch.is_some() {
                        AM_PF_RGBA8 as u32
                    } else {
                        format
                    },
                )?
                .ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
                let data = if scratch.is_some() {
                    am_camera_core::media::pixel_ops::rgba_to_nv12(&decoded.data, f.width, f.height)
                        .map_err(|_| AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED)?
                } else {
                    decoded.data
                };
                drop(scratch);
                let c = &inner.config.abi;
                let meta = am_frame_meta_t {
                    source_id: decoder.source,
                    frame_id: crate::frames::next_frame_id()?,
                    seq: next(&decoder.sequence)?,
                    source_generation: decoder.generation.load(Ordering::Acquire),
                    config_generation: config,
                    pts,
                    time_base_num: if pts != i64::MIN { c.time_base_num } else { 0 },
                    time_base_den: if pts != i64::MIN { c.time_base_den } else { 0 },
                    captured_at_ms: i64::MIN,
                    received_at_ms: received,
                    clock_quality: AM_CQ_LOCAL_RECEIPT as u32,
                    clock_mask: AM_CLK_RECEIVED | if pts != i64::MIN { AM_CLK_PTS } else { 0 },
                    transform: am_spatial_transform_t {
                        scale_x: 1.,
                        scale_y: 1.,
                        valid: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                return Ok(Some(Frame::with_format(
                    data, f.width, f.height, format, meta, charge,
                )?));
            }
            Err(DecodeError::Drained) => {
                decoder
                    .state
                    .store(AM_DEC_DRAINED as u32, Ordering::Release);
                return Err(AM_ERR_SDK_DRAINED);
            }
            Err(e) => return Err(convert(e)),
            Ok(None) => {}
        }
        if let Some(packet) = inner.queue.front() {
            let allowed = inner
                .gate
                .observe(inner.config.codec, &packet.bytes[..packet.valid])?;
            if !allowed {
                inner.pop_packet()?;
                continue;
            }
            if inner.gate.dirty {
                let config = inner.config_generation_checked(decoder)?;
                let cpu = CpuDecoder::create(
                    runtime(&decoder.ctx)?,
                    inner.config.codec,
                    &inner.gate.extra(),
                    (decoder.ctx.limits()?.frame_budget_bytes / 4)
                        .min(SDK_MAX_DYNAMIC_AXIS * SDK_MAX_DYNAMIC_AXIS),
                )
                .map_err(convert)?;
                inner.cpu = cpu.into();
                inner.timings.clear();
                inner.gate.dirty = false;
                decoder.config_generation.store(config, Ordering::Release);
                *decoder
                    .codec_info
                    .lock()
                    .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)? = info(&inner.config, config);
            }
            let packet = inner.queue.front().ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
            let picture = access_units(inner.config.codec, &packet.bytes[..packet.valid])
                .map_err(convert)?
                .iter()
                .any(|nal| {
                    if inner.config.codec == VideoCodec::H264 {
                        (1..=5).contains(&(nal[0] & 31))
                    } else {
                        (nal[0] >> 1) & 63 <= 31
                    }
                });
            if picture && inner.timings.len() >= 256 {
                if inner.timings.front().is_some_and(|t| t.delivered) {
                    inner.timings.pop_front();
                } else {
                    return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                }
            }
            if !inner
                .cpu
                .send_padded(&packet.bytes, packet.valid, packet.tag, packet.tag)
                .map_err(convert)?
            {
                return Err(AM_ERR_SDK_TRY_AGAIN);
            }
            if picture {
                inner.timings.push_back(Timing {
                    tag: packet.tag,
                    pts: packet.pts,
                    received: packet.received,
                    config: decoder.config_generation.load(Ordering::Acquire),
                    delivered: false,
                });
            }
            inner.gate.started = true;
            inner.pop_packet()?;
            continue;
        }
        if decoder.state.load(Ordering::Acquire) == AM_DEC_DRAINING as u32 && !inner.flushed {
            if inner.cpu.drain().map_err(convert)? {
                inner.flushed = true;
            }
            continue;
        }
        return Ok(None);
    }
}
impl Inner {
    fn pop_packet(&mut self) -> Result<()> {
        let removed = self.queue.pop_front().ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
        self.queue_bytes -= removed.bytes.len() as u64;
        self.metrics
            .decoder_queue_items
            .fetch_sub(1, Ordering::AcqRel);
        self.metrics
            .decoder_queue_bytes
            .fetch_sub(removed.bytes.len() as u64, Ordering::AcqRel);
        Ok(())
    }
    fn config_generation_checked(&self, d: &Decoder) -> Result<u32> {
        d.config_generation
            .load(Ordering::Acquire)
            .checked_add(1)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        // reset/close直接丢弃输入队列，也必须从域聚合出账。
        self.metrics
            .decoder_queue_items
            .fetch_sub(self.queue.len() as u64, Ordering::AcqRel);
        self.metrics
            .decoder_queue_bytes
            .fetch_sub(self.queue_bytes, Ordering::AcqRel);
    }
}
#[no_mangle]
pub unsafe extern "C" fn amc_decoder_receive(
    dec: *mut amc_decoder_t,
    wait: u32,
    out: *mut *mut amc_frame_t,
) -> i32 {
    status(domain(), dec as usize, || {
        am_sdk_support::args::pointer(out)?;
        unsafe {
            out.write(std::ptr::null_mut());
        }
        if wait as u64 > SDK_RECEIVE_TIMEOUT_MS {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        let entry = domain().active(dec as usize)?;
        let decoder = entry.data::<Decoder>()?;
        let until = Instant::now() + Duration::from_millis(wait as u64);
        loop {
            if entry.state.load(Ordering::Acquire) != AM_LIFECYCLE_RUNNING as u32 {
                return Err(AM_ERR_SDK_HANDLE_CLOSED);
            }
            if let Ok(mut locked) = decoder.inner.try_lock() {
                let inner = locked.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
                if let Some(frame) = receive(decoder, inner)? {
                    let pointer = Arc::as_ptr(&frame) as usize;
                    domain().buffers.insert(pointer, frame)?;
                    unsafe {
                        out.write(pointer as _);
                    }
                    return Ok(());
                }
            }
            if wait == 0 {
                return Err(AM_ERR_SDK_TRY_AGAIN);
            }
            if Instant::now() >= until {
                return Err(AM_ERR_SDK_DEADLINE_EXCEEDED);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_decoder_drain(dec: *mut amc_decoder_t) -> i32 {
    status(domain(), dec as usize, || {
        let entry = domain().active(dec as usize)?;
        let d = entry.data::<Decoder>()?;
        let _guard = d.inner.try_lock().map_err(|_| AM_ERR_SDK_TRY_AGAIN)?;
        let state = d.state.load(Ordering::Acquire);
        if state == AM_DEC_READY as u32 {
            d.state.store(AM_DEC_DRAINING as u32, Ordering::Release);
            Ok(())
        } else if state == AM_DEC_DRAINING as u32 || state == AM_DEC_DRAINED as u32 {
            Ok(())
        } else {
            Err(AM_ERR_CAMERA_DECODER_STATE_INVALID)
        }
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_decoder_reset(
    dec: *mut amc_decoder_t,
    cfg: *const am_decoder_config_t,
) -> i32 {
    status(domain(), dec as usize, || {
        let entry = domain().active(dec as usize)?;
        let d = entry.data::<Decoder>()?;
        let mut guard = d.inner.try_lock().map_err(|_| AM_ERR_SDK_TRY_AGAIN)?;
        let old = guard.as_ref().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        let config = if cfg.is_null() {
            old.config.clone()
        } else {
            unsafe { Config::read(cfg) }?
        };
        let generation = d
            .generation
            .load(Ordering::Acquire)
            .checked_add(1)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        let config_generation = d
            .config_generation
            .load(Ordering::Acquire)
            .checked_add(1)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        let previous = d.state.swap(AM_DEC_RESET_PENDING as u32, Ordering::AcqRel);
        let rebuilt = make_inner(&d.ctx, config);
        match rebuilt {
            Ok(inner) => {
                *d.codec_info.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)? =
                    info(&inner.config, config_generation);
                *guard = Some(inner);
                d.generation.store(generation, Ordering::Release);
                d.sequence.store(0, Ordering::Release);
                d.config_generation
                    .store(config_generation, Ordering::Release);
                d.state.store(AM_DEC_READY as u32, Ordering::Release);
                Ok(())
            }
            Err(e) => {
                d.state.store(previous, Ordering::Release);
                Err(e)
            }
        }
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_decoder_close(
    dec: *mut amc_decoder_t,
    wait: u32,
    out: *mut u32,
) -> i32 {
    status(domain(), dec as usize, || {
        am_sdk_support::args::pointer(out)?;
        {
            domain().get(dec as usize)?.data::<Decoder>()?;
        }
        let state = domain().close(dec as usize, wait)?;
        unsafe {
            out.write(state);
        }
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_decoder_get_status(
    dec: *mut amc_decoder_t,
    out: *mut am_decoder_status_t,
) -> i32 {
    status(domain(), dec as usize, || {
        let e = domain().get(dec as usize)?;
        let d = e.data::<Decoder>()?;
        let guard = d.inner.try_lock().map_err(|_| AM_ERR_SDK_TRY_AGAIN)?;
        let lifecycle = e.state.load(Ordering::Acquire);
        let state = if lifecycle == AM_LIFECYCLE_CLOSING as u32 {
            AM_DEC_CLOSING as u32
        } else {
            d.state.load(Ordering::Acquire)
        };
        let value = am_decoder_status_t {
            state: state as _,
            decoder_generation: d.generation.load(Ordering::Acquire),
            config_generation: d.config_generation.load(Ordering::Acquire),
            queue_items: guard.as_ref().map_or(0, |i| i.queue.len() as u32),
            queue_bytes: guard.as_ref().map_or(0, |i| i.queue_bytes),
            ..Default::default()
        };
        unsafe { args::write(out, value) }
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_decoder_get_info(
    dec: *mut amc_decoder_t,
    out: *mut am_decoder_info_t,
) -> i32 {
    status(domain(), dec as usize, || {
        let e = domain().get(dec as usize)?;
        let d = e.data::<Decoder>()?;
        let info = *d.codec_info.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        unsafe { args::write(out, info) }
    })
}
