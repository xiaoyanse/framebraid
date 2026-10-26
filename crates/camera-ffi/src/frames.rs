use super::*;
#[cfg(test)]
mod budget_tests;
#[cfg(test)]
pub(crate) fn isolated_test_state() -> (am_sdk_support::handles::Domain, Arc<CameraState>) {
    let domain = am_sdk_support::handles::Domain::new(false);
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    let key = unsafe { am_sdk_support::context::initialize_camera(&domain, &params) }.unwrap();
    let state = Arc::new(CameraState {
        ctx_key: key,
        ctx: domain.get(key).unwrap(),
        metrics: Arc::default(),
        usage: Arc::default(),
    });
    (domain, state)
}
use am_sdk_support::{args, buffers::Buffer, context::Context, handles::Entry, Result};
use std::{
    any::Any,
    collections::HashMap,
    ffi::c_void,
    sync::{atomic::Ordering, Arc, Mutex, Weak},
};

#[derive(Default)]
struct Usage {
    bytes: u64,
    external_bytes: u64,
    external_frames: u32,
    decoders: u32,
    controls: usize,
    control_bytes: usize,
}
#[derive(Default)]
pub struct CameraMetrics {
    pub threads: Arc<crate::threads::Budget>,
    pub decoder_queue_items: std::sync::atomic::AtomicU64,
    pub decoder_queue_bytes: std::sync::atomic::AtomicU64,
    pub accepted: std::sync::atomic::AtomicU64,
    pub rejected: std::sync::atomic::AtomicU64,
    pub dropped: std::sync::atomic::AtomicU64,
    pub decoded: std::sync::atomic::AtomicU64,
    pub frames_dropped: std::sync::atomic::AtomicU64,
    pub reconnects: std::sync::atomic::AtomicU64,
    pub resyncs: std::sync::atomic::AtomicU64,
    pub newest_received: std::sync::atomic::AtomicI64,
    pub capture_fps_bits: std::sync::atomic::AtomicU64,
}
pub struct CameraState {
    pub ctx_key: usize,
    pub ctx: Arc<Entry>,
    pub metrics: Arc<CameraMetrics>,
    usage: Arc<Mutex<Usage>>,
}
type States = Mutex<HashMap<usize, Weak<CameraState>>>;
static STATES: am_sdk_support::unload::UnloadCell<States> =
    am_sdk_support::unload::UnloadCell::new();
#[cfg(all(windows, not(test)))]
pub fn unload_ready() -> bool {
    STATES.get().is_none_or(|states| {
        states
            .try_lock()
            .is_ok_and(|values| values.values().all(|state| state.strong_count() == 0))
    })
}
#[cfg(all(windows, not(test)))]
pub unsafe fn unload_static() {
    let _ = unsafe {
        STATES.clear_if(|states| {
            states
                .try_lock()
                .is_ok_and(|values| values.values().all(|state| state.strong_count() == 0))
        })
    };
}
pub fn camera_state(key: usize) -> Result<Arc<CameraState>> {
    let mut states = STATES
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    states.retain(|_, s| s.strong_count() != 0);
    if let Some(state) = states.get(&key).and_then(Weak::upgrade) {
        return Ok(state);
    }
    let ctx = domain().get(key)?;
    ctx.data::<Context>()?;
    let usage = {
        let mut services = ctx
            .data::<Context>()?
            .services
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        let id = std::any::TypeId::of::<Mutex<Usage>>();
        if let Some(value) = services.get(&id) {
            value
                .clone()
                .downcast::<Mutex<Usage>>()
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
        } else {
            let usage = Arc::new(Mutex::<Usage>::default());
            services.insert(id, usage.clone());
            usage
        }
    };
    let metrics = {
        let mut services = ctx
            .data::<Context>()?
            .services
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        let id = std::any::TypeId::of::<CameraMetrics>();
        if let Some(value) = services.get(&id) {
            value
                .clone()
                .downcast::<CameraMetrics>()
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
        } else {
            let metrics = Arc::new(CameraMetrics::default());
            services.insert(id, metrics.clone());
            metrics
        }
    };
    let state = Arc::new(CameraState {
        ctx,
        ctx_key: key,
        usage,
        metrics,
    });
    states.insert(key, Arc::downgrade(&state));
    Ok(state)
}
impl CameraState {
    pub fn limits(&self) -> Result<am_limits_t> {
        Ok(*self
            .ctx
            .data::<Context>()?
            .limits
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?)
    }
    pub fn usage_snapshot(&self) -> Result<(u64, u64, u32)> {
        let usage = self.usage.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        Ok((usage.bytes, usage.external_bytes, usage.external_frames))
    }
    pub(crate) fn control_usage_snapshot(&self) -> Result<(usize, usize)> {
        let usage = self.usage.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        Ok((usage.controls, usage.control_bytes))
    }
    pub fn control(self: &Arc<Self>, bytes: usize, external: bool) -> Result<ControlCharge> {
        self.reserve_control(bytes, external, true)
    }
    pub(crate) fn metadata(self: &Arc<Self>, bytes: usize) -> Result<ControlCharge> {
        self.reserve_control(bytes, false, false)
    }
    fn reserve_control(
        self: &Arc<Self>,
        bytes: usize,
        external: bool,
        counted: bool,
    ) -> Result<ControlCharge> {
        let l = self.limits()?;
        let ctx = self.ctx.data::<Context>()?;
        let q = ctx.queues.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?[5];
        let mut u = self.usage.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if counted && u.controls >= (l.control_max_items.min(q.max_items)) as usize
            || bytes > (q.max_bytes as usize).saturating_sub(u.control_bytes)
        {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        u.controls += usize::from(counted);
        u.control_bytes += bytes;
        if external {
            ctx.buffers.fetch_add(1, Ordering::AcqRel);
        }
        Ok(ControlCharge {
            ctx: Arc::downgrade(&self.ctx),
            usage: self.usage.clone(),
            bytes,
            external,
            counted,
        })
    }
    pub fn decoder(self: &Arc<Self>) -> Result<DecoderPermit> {
        let l = self.limits()?;
        let mut u = self.usage.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if u.decoders >= l.max_decoders {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        u.decoders += 1;
        Ok(DecoderPermit(self.clone()))
    }
    pub fn charge(self: &Arc<Self>, bytes: u64, external: bool) -> Result<Charge> {
        let l = self.limits()?;
        let mut u = self.usage.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if bytes > l.frame_budget_bytes.saturating_sub(u.bytes)
            || external
                && (u.external_frames >= l.external_hold_max_frames
                    || bytes > l.external_hold_max_bytes.saturating_sub(u.external_bytes))
        {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        u.bytes += bytes;
        if external {
            u.external_frames += 1;
            u.external_bytes += bytes;
            self.ctx
                .data::<Context>()?
                .buffers
                .fetch_add(1, Ordering::AcqRel);
        }
        Ok(Charge {
            state: self.clone(),
            bytes,
            external,
        })
    }
}
pub struct DecoderPermit(Arc<CameraState>);
impl Drop for DecoderPermit {
    fn drop(&mut self) {
        if let Ok(mut u) = self.0.usage.lock() {
            u.decoders -= 1;
        }
    }
}
pub struct Charge {
    state: Arc<CameraState>,
    bytes: u64,
    external: bool,
}
impl Charge {
    pub(crate) fn shrink_to(&mut self, bytes: u64) -> Result<()> {
        if self.external || bytes > self.bytes {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let mut usage = self
            .state
            .usage
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        usage.bytes -= self.bytes - bytes;
        self.bytes = bytes;
        Ok(())
    }
    pub(crate) fn split_internal(&mut self, bytes: u64) -> Result<Self> {
        if self.external || bytes > self.bytes {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        self.bytes -= bytes;
        Ok(Self {
            state: self.state.clone(),
            bytes,
            external: false,
        })
    }
    /// 将既有暂存额度转交外部帧，原子核持帧限制，不重复占总字节。
    pub(crate) fn take_external(&mut self, bytes: u64) -> Result<Self> {
        if self.external || bytes == 0 || bytes > self.bytes {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let limits = self.state.limits()?;
        let ctx = self.state.ctx.data::<Context>()?;
        let mut usage = self
            .state
            .usage
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if usage.external_frames >= limits.external_hold_max_frames
            || bytes
                > limits
                    .external_hold_max_bytes
                    .saturating_sub(usage.external_bytes)
        {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        self.bytes -= bytes;
        usage.external_bytes += bytes;
        usage.external_frames += 1;
        ctx.buffers.fetch_add(1, Ordering::AcqRel);
        Ok(Self {
            state: self.state.clone(),
            bytes,
            external: true,
        })
    }
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }
}
impl Drop for Charge {
    fn drop(&mut self) {
        if let Ok(mut u) = self.state.usage.lock() {
            u.bytes -= self.bytes;
            if self.external {
                u.external_frames -= 1;
                u.external_bytes -= self.bytes;
            }
        }
        if self.external {
            if let Ok(ctx) = self.state.ctx.data::<Context>() {
                ctx.buffers.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }
}
pub struct Frame {
    pub data: Vec<u8>,
    pub view: Box<am_frame_view_t>,
    pub meta: Box<am_frame_meta_t>,
    pub assoc: Box<am_frame_assoc_t>,
    pub(crate) _charge: Charge,
}
unsafe impl Send for Frame {}
unsafe impl Sync for Frame {}
impl Buffer for Frame {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl Frame {
    pub fn with_format(
        data: Vec<u8>,
        width: u32,
        height: u32,
        format: u32,
        meta: am_frame_meta_t,
        charge: Charge,
    ) -> Result<Arc<Self>> {
        let channels = if format == AM_PF_RGBA8 as u32 {
            4
        } else if format == AM_PF_NV12 as u32 {
            1
        } else {
            3
        };
        let luma = (width as usize * height as usize) as u64;
        let nv12 = format == AM_PF_NV12 as u32;
        let view = am_frame_view_t {
            width,
            height,
            format,
            plane_count: if nv12 { 2 } else { 1 },
            planes: [
                data.as_ptr(),
                if nv12 {
                    unsafe { data.as_ptr().add(luma as usize) }
                } else {
                    std::ptr::null()
                },
                std::ptr::null(),
                std::ptr::null(),
            ],
            strides: [width * channels, if nv12 { width } else { 0 }, 0, 0],
            plane_size_bytes: [
                if nv12 { luma } else { data.len() as u64 },
                if nv12 { luma / 2 } else { 0 },
                0,
                0,
            ],
            color_matrix: AM_CM_BT601 as u32,
            color_range: if nv12 {
                AM_CR_LIMITED as u32
            } else {
                AM_CR_FULL as u32
            },
            ..Default::default()
        };
        let assoc = am_frame_assoc_t {
            source_id: meta.source_id,
            frame_id: meta.frame_id,
            source_generation: meta.source_generation,
            config_generation: meta.config_generation,
            pts: meta.pts,
            time_base_num: meta.time_base_num,
            time_base_den: meta.time_base_den,
            captured_at_ms: meta.captured_at_ms,
            received_at_ms: meta.received_at_ms,
            clock_mask: meta.clock_mask,
            clock_quality: meta.clock_quality,
            valid: AM_ASSOC_SOURCE
                | AM_ASSOC_FRAME
                | AM_ASSOC_GENERATION
                | if meta.clock_mask & AM_CLK_PTS != 0 {
                    AM_ASSOC_PTS
                } else {
                    0
                },
            ..Default::default()
        };
        let mut owned_view = Box::new(am_frame_view_t::default());
        let mut owned_meta = Box::new(am_frame_meta_t::default());
        let mut owned_assoc = Box::new(am_frame_assoc_t::default());
        unsafe {
            args::write(&mut *owned_view, view)?;
            args::write(&mut *owned_meta, meta)?;
            args::write(&mut *owned_assoc, assoc)?;
        }
        Ok(Arc::new(Self {
            data,
            view: owned_view,
            meta: owned_meta,
            assoc: owned_assoc,
            _charge: charge,
        }))
    }
}
pub fn frame(key: usize) -> Result<Arc<dyn Buffer>> {
    let body = domain().buffers.get(key)?;
    if body.as_any().downcast_ref::<Frame>().is_none() {
        return Err(AM_ERR_SDK_HANDLE_INVALID);
    }
    Ok(body)
}
#[no_mangle]
pub unsafe extern "C" fn amc_frame_retain(p: *mut amc_frame_t) -> i32 {
    status(domain(), 0, || {
        frame(p as usize)?;
        domain().buffers.retain(p as usize)
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_frame_release(p: *mut amc_frame_t) -> i32 {
    status(domain(), 0, || {
        frame(p as usize)?;
        domain().buffers.release(p as usize)
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_frame_get_view(
    p: *const amc_frame_t,
    out: *mut am_frame_view_t,
) -> i32 {
    status(domain(), 0, || {
        let body = frame(p as usize)?;
        let f = body
            .as_any()
            .downcast_ref::<Frame>()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
        debug_assert_eq!(
            f.data.len() as u64,
            f.view.plane_size_bytes.iter().sum::<u64>()
        );
        unsafe { args::write(out, *f.view) }
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_frame_get_meta(
    p: *const amc_frame_t,
    out: *mut am_frame_meta_t,
) -> i32 {
    status(domain(), 0, || {
        let body = frame(p as usize)?;
        let f = body
            .as_any()
            .downcast_ref::<Frame>()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
        unsafe { args::write(out, *f.meta) }
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_frame_as_pixel_view(
    p: *mut amc_frame_t,
    out: *mut am_pixel_view_t,
) -> i32 {
    status(domain(), 0, || {
        let body = frame(p as usize)?;
        let f = body
            .as_any()
            .downcast_ref::<Frame>()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
        let value = am_pixel_view_t {
            magic: AM_PIXEL_VIEW_MAGIC,
            owner_handle: p.cast(),
            retain_owner: Some(amc_frame_retain_bridge),
            release_owner: Some(amc_frame_release_bridge),
            view: &*f.view,
            assoc: &*f.assoc,
            ..Default::default()
        };
        unsafe { args::write(out, value) }
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_frame_retain_bridge(p: *mut c_void) {
    let _ = unsafe { amc_frame_retain(p.cast()) };
}
#[no_mangle]
pub unsafe extern "C" fn amc_frame_release_bridge(p: *mut c_void) {
    let _ = unsafe { amc_frame_release(p.cast()) };
}

pub(crate) mod scale;

pub struct ControlCharge {
    ctx: Weak<Entry>,
    usage: Arc<Mutex<Usage>>,
    bytes: usize,
    external: bool,
    counted: bool,
}
impl ControlCharge {
    pub(crate) fn shrink_to(&mut self, bytes: usize) -> Result<()> {
        if bytes > self.bytes {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let mut usage = self.usage.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        usage.control_bytes -= self.bytes - bytes;
        self.bytes = bytes;
        Ok(())
    }
}
impl Drop for ControlCharge {
    fn drop(&mut self) {
        if let Ok(mut usage) = self.usage.lock() {
            usage.controls -= usize::from(self.counted);
            usage.control_bytes -= self.bytes;
        }
        if self.external {
            if let Some(ctx) = self.ctx.upgrade() {
                if let Ok(c) = ctx.data::<Context>() {
                    c.buffers.fetch_sub(1, Ordering::AcqRel);
                }
            }
        }
    }
}

static FRAME_IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn next_frame_id() -> Result<u64> {
    FRAME_IDS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
        .map(|n| n + 1)
        .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)
}
