use super::frames::{camera_state, CameraState, Charge, DecoderPermit, Frame};
use super::*;
use am_camera_core::media::codec::selection::{HardwarePolicy, SelectionReport};
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
mod receive;
mod recovery;
use config::{convert, runtime, Config, Gate};
use receive::receive;
#[cfg(test)]
mod tests;

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
    selection: SelectionReport,
    hardware_failed: bool,
    startup: Option<Packet>,
    replay: Option<Packet>,
    recovery_charge: Option<Charge>,
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
fn info(c: &Config, generation: u32, hardware_pending: bool) -> am_decoder_info_t {
    am_decoder_info_t {
        codec: c.abi.codec,
        bitstream_format: c.abi.bitstream_format,
        output_pixel_format: c.abi.output_pixel_format,
        hw_policy: c.abi.hw_policy,
        hw_accelerated: 0,
        backend_name_utf8: if hardware_pending {
            std::ptr::null()
        } else {
            c"ffmpeg-cpu".as_ptr()
        },
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
    let selected = CpuDecoder::create_with_policy(
        runtime(ctx)?,
        cfg.codec,
        &cfg.extra,
        pixels,
        if cfg.abi.hw_policy == AM_DHW_SOFTWARE_ONLY as u32 {
            HardwarePolicy::SoftwareOnly
        } else {
            HardwarePolicy::PreferHardware
        },
        Duration::from_millis(1500),
    )
    .map_err(convert)?;
    let gate = Gate::new(&cfg)?;
    Ok(Inner {
        metrics: ctx.metrics.clone(),
        cpu: selected.decoder.into(),
        selection: selected.report,
        hardware_failed: false,
        startup: None,
        replay: None,
        recovery_charge: None,
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
        let inner = make_inner(&state, config)?;
        let codec_info = Mutex::new(info(&inner.config, 1, inner.selection.selected.is_some()));
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
impl Inner {
    fn front_packet(&self) -> Option<&Packet> {
        self.replay.as_ref().or_else(|| self.queue.front())
    }
    fn pop_packet(&mut self) -> Result<Packet> {
        if let Some(packet) = self.replay.take() {
            return Ok(packet);
        }
        let removed = self.queue.pop_front().ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
        self.queue_bytes -= removed.bytes.len() as u64;
        self.metrics
            .decoder_queue_items
            .fetch_sub(1, Ordering::AcqRel);
        self.metrics
            .decoder_queue_bytes
            .fetch_sub(removed.bytes.len() as u64, Ordering::AcqRel);
        Ok(removed)
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
                if let Some(frame) = receive(dec as usize, decoder, inner)? {
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
                *d.codec_info.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)? = info(
                    &inner.config,
                    config_generation,
                    inner.selection.selected.is_some(),
                );
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
