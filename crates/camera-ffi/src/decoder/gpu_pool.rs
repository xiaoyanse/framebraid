//! GPU 帧池与句柄导出（M2a）：有界、计费、租约语义与共享帧池一致。
use super::*;
use am_camera_core::media::codec::GpuFrameInfo;
use am_sdk_support::{args, Result};
use std::sync::atomic::AtomicBool;
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Weak,
    },
};

/// GPU 帧池内部计数（M2b 随 Vision 接线消费；首版由 pipeline 内部持有）。
#[allow(dead_code)]
pub(crate) struct GpuFramePoolInner {
    pub frames_in_use: AtomicU64,
    pub bytes_in_use: AtomicU64,
    pub max_frames: usize,
    pub max_bytes: u64,
    pub stats_delivered: AtomicU64,
    pub stats_cpu_fallbacks: AtomicU64,
    pub stats_bytes_est: AtomicU64,
    pub stopping: AtomicBool,
    /// SOFTWARE_ONLY 策略下 GPU 订阅返回 notSupported（§2.6a，不静默降级）。
    pub software_only: bool,
}

/// GPU 帧对象：持有纹理引用与元数据，Drop 归还池计数。
#[allow(dead_code)]
pub struct GpuFrame {
    pub pool: Weak<GpuFramePoolInner>,
    pub desc: am_gpu_frame_desc_t,
    /// D3D11 纹理指针（借用期有效；Release 由 decoder 的帧生命周期管理）。
    pub texture: *mut c_void,
    bytes: u64,
}
unsafe impl Send for GpuFrame {}
unsafe impl Sync for GpuFrame {}

#[allow(dead_code)]
impl GpuFramePoolInner {
    pub fn try_admit(&self, bytes: u64) -> Result<()> {
        let frames = self.frames_in_use.load(Ordering::Acquire);
        let current = self.bytes_in_use.load(Ordering::Acquire);
        if frames as usize >= self.max_frames || current.saturating_add(bytes) > self.max_bytes {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        Ok(())
    }

    pub fn publish(
        self: &Arc<Self>,
        info: &GpuFrameInfo,
        texture: *mut c_void,
    ) -> Result<GpuFrame> {
        if self.software_only {
            return Err(AM_ERR_SDK_NOT_SUPPORTED);
        }
        // 关闭排空：stopping 后拒绝新帧，语义与共享帧池关闭后 active() 拒绝一致。
        if self.stopping.load(Ordering::Acquire) {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let bytes = info.info.rgba_bytes as u64;
        self.try_admit(bytes)?;
        self.frames_in_use.fetch_add(1, Ordering::AcqRel);
        self.bytes_in_use.fetch_add(bytes, Ordering::AcqRel);
        self.stats_delivered.fetch_add(1, Ordering::AcqRel);
        self.stats_bytes_est.fetch_add(bytes, Ordering::AcqRel);
        // flags bit0：共享句柄不可得（信息性标注；纹理指针交付仍可用）。
        // 真正的 CPU 回读降级由 M2b 消费侧协商决定，届时才计入
        // stats_cpu_fallbacks——句柄缺失不等于回读。
        let handle_missing = info.shared_nt_handle == 0;
        let desc = am_gpu_frame_desc_t {
            struct_size: std::mem::size_of::<am_gpu_frame_desc_t>() as u32,
            width: info.info.width,
            height: info.info.height,
            d3d_format: info.d3d_format,
            subresource: info.subresource,
            shared_nt_handle: info.shared_nt_handle,
            flags: if handle_missing { 1 } else { 0 },
            assoc: Default::default(),
        };
        Ok(GpuFrame {
            pool: Arc::downgrade(self),
            desc,
            texture,
            bytes,
        })
    }
}

impl Drop for GpuFrame {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            pool.frames_in_use.fetch_sub(1, Ordering::AcqRel);
            pool.bytes_in_use.fetch_sub(self.bytes, Ordering::AcqRel);
        }
    }
}

// ============ NV12 纹理回读（§2.12 GPU 帧编码消费；upload_nv12_slice 反向） ============

/// 回读产物：完整映射内容（Y 平面后跟 UV 平面）与行距。
/// D3D11 规范 NV12 的 UV 平面行距等于 Y 行距。
pub(crate) struct Nv12Host {
    pub data: Vec<u8>,
    pub row_pitch: usize,
}

struct ComRelease(*mut c_void);
impl Drop for ComRelease {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { com_release(self.0) };
        }
    }
}

unsafe fn vtable(object: *mut c_void) -> *const *const c_void {
    unsafe { *object.cast::<*const *const c_void>() }
}

unsafe fn com_release(ptr: *mut c_void) {
    let release: unsafe extern "system" fn(*mut c_void) -> u32 =
        unsafe { std::mem::transmute(*vtable(ptr).add(2)) };
    unsafe { release(ptr) };
}

/// 纹理→所属设备（ID3D11DeviceChild::GetDevice 槽 3；借用指针，guard 保活）。
unsafe fn device_of_texture(texture: *mut c_void) -> Result<(ComRelease, *mut c_void)> {
    let get_device: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) =
        unsafe { std::mem::transmute(*vtable(texture).add(3)) };
    let mut device: *mut c_void = std::ptr::null_mut();
    unsafe { get_device(texture, &mut device) };
    if device.is_null() {
        return Err(AM_ERR_SDK_INTERNAL_ERROR);
    }
    Ok((ComRelease(device), device))
}

/// 立即上下文（借用指针归设备所有，不 AddRef；设备 guard 保活期间有效）。
unsafe fn immediate_context(device: *mut c_void) -> Result<*mut c_void> {
    let get_ctx: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) =
        unsafe { std::mem::transmute(*vtable(device).add(40)) };
    let mut context: *mut c_void = std::ptr::null_mut();
    unsafe { get_ctx(device, &mut context) };
    if context.is_null() {
        return Err(AM_ERR_SDK_INTERNAL_ERROR);
    }
    Ok(context)
}

#[repr(C)]
struct TexDesc {
    width: u32,
    height: u32,
    mip_levels: u32,
    array_size: u32,
    format: u32,
    sample_desc: (u32, u32),
    usage: u32,
    bind_flags: u32,
    cpu_access_flags: u32,
    misc_flags: u32,
}

#[repr(C)]
struct MappedSubresource {
    data: *mut c_void,
    row_pitch: u32,
    depth_pitch: u32,
}

/// NV12 纹理整 slice 回读到 host（staging 拷贝+CPU map，§2.12 CPU 回读合法
/// 实现路径）。槽位核对链同 vp_transit.rs（Windows SDK 10.0.26100 d3d11.h；
/// Map=14/Unmap=15 由已实证 End=28/GetData=29 按声明序偏移核对）。
///
/// # Safety
/// `texture` 须为租约借用期内的有效 D3D11 NV12 纹理指针。
pub(crate) unsafe fn readback_nv12(
    texture: *mut c_void,
    subresource: u32,
    width: u32,
    height: u32,
) -> Result<Nv12Host> {
    let (_device_guard, device) = unsafe { device_of_texture(texture)? };
    let context = unsafe { immediate_context(device)? };
    // GetDesc（Texture2D 槽 10）：尺寸/格式与引用声明不符即拒绝——宿主谎报
    // 尺寸会按错值切平面越界；subresource 越界同拒。
    let get_desc: unsafe extern "system" fn(*mut c_void, *mut TexDesc) =
        unsafe { std::mem::transmute(*vtable(texture).add(10)) };
    let mut desc = TexDesc {
        width: 0,
        height: 0,
        mip_levels: 0,
        array_size: 0,
        format: 0,
        sample_desc: (0, 0),
        usage: 0,
        bind_flags: 0,
        cpu_access_flags: 0,
        misc_flags: 0,
    };
    unsafe { get_desc(texture, &mut desc) };
    if desc.format != 103
        || desc.width != width
        || desc.height != height
        || subresource >= desc.mip_levels.saturating_mul(desc.array_size)
    {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let staging_desc = TexDesc {
        width,
        height,
        mip_levels: 1,
        array_size: 1,
        format: 103,
        sample_desc: (1, 0),
        usage: 3, // D3D11_USAGE_STAGING
        bind_flags: 0,
        cpu_access_flags: 0x2_0000, // D3D11_CPU_ACCESS_READ
        misc_flags: 0,
    };
    let create_tex2d: unsafe extern "system" fn(
        *mut c_void,
        *const TexDesc,
        *const c_void,
        *mut *mut c_void,
    ) -> i32 = unsafe { std::mem::transmute(*vtable(device).add(5)) };
    let mut staging: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { create_tex2d(device, &staging_desc, std::ptr::null(), &mut staging) };
    if rc < 0 || staging.is_null() {
        return Err(AM_ERR_SDK_INTERNAL_ERROR);
    }
    let _staging = ComRelease(staging);
    let copy: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u32,
        u32,
        u32,
        u32,
        *mut c_void,
        u32,
        *const c_void,
    ) = unsafe { std::mem::transmute(*vtable(context).add(46)) };
    unsafe {
        copy(
            context,
            staging,
            0,
            0,
            0,
            0,
            texture,
            subresource,
            std::ptr::null(),
        )
    };
    let map: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u32,
        u32,
        u32,
        *mut MappedSubresource,
    ) -> i32 = unsafe { std::mem::transmute(*vtable(context).add(14)) };
    let mut mapped = MappedSubresource {
        data: std::ptr::null_mut(),
        row_pitch: 0,
        depth_pitch: 0,
    };
    let rc = unsafe { map(context, staging, 0, 1, 0, &mut mapped) }; // D3D11_MAP_READ
    if rc < 0 || mapped.data.is_null() {
        return Err(AM_ERR_SDK_INTERNAL_ERROR);
    }
    let row_pitch = mapped.row_pitch as usize;
    // Y 平面行距不足以容纳整行即布局不符（UV 行距按规范等于 Y 行距）。
    if row_pitch < width as usize {
        unsafe { unmap_staging(context, staging) };
        return Err(AM_ERR_SDK_INTERNAL_ERROR);
    }
    let total = row_pitch * (height as usize + height as usize / 2);
    let mut data = vec![0u8; total];
    unsafe { std::ptr::copy_nonoverlapping(mapped.data.cast::<u8>(), data.as_mut_ptr(), total) };
    unsafe { unmap_staging(context, staging) };
    Ok(Nv12Host { data, row_pitch })
}

unsafe fn unmap_staging(context: *mut c_void, staging: *mut c_void) {
    let unmap: unsafe extern "system" fn(*mut c_void, *mut c_void, u32) =
        unsafe { std::mem::transmute(*vtable(context).add(15)) };
    unsafe { unmap(context, staging, 0) };
}

// ============ C 导出 ============

#[no_mangle]
pub unsafe extern "C" fn amc_gpu_frame_pool_create(
    _ctx: *mut amc_context_t,
    _cfg: *const std::ffi::c_void,
    _out: *mut *mut amc_gpu_frame_pool_t,
) -> i32 {
    // M2a 首版：GPU 帧池由解码 pipeline 内部管理，此导出返回 notSupported；
    // 正式池创建 API 在 M2b 随 Vision 侧接线一并开放。
    status(crate::domain(), 0, || Err(AM_ERR_SDK_NOT_SUPPORTED))
}

/// 无生产链路期间的导出守卫：不存在合法 GPU 帧句柄（审计 A1），导出面
/// 统一 notSupported（与 pool_create 一致），不裸解引用/释放任意指针；
/// 真实句柄体系随 GPU 闭环接线（issue #3 主体）一并交付。
fn gpu_frame_unavailable(frame: *const std::ffi::c_void) -> Result<()> {
    if frame.is_null() {
        return Err(AM_ERR_SDK_NULL_ARGUMENT);
    }
    Err(AM_ERR_SDK_NOT_SUPPORTED)
}

#[no_mangle]
pub unsafe extern "C" fn amc_gpu_frame_desc(
    frame: *const amc_gpu_frame_t,
    out: *mut am_gpu_frame_desc_t,
) -> i32 {
    status(crate::domain(), frame as usize, || {
        args::validate_output(out, std::mem::size_of::<am_gpu_frame_desc_t>())?;
        gpu_frame_unavailable(frame.cast())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_gpu_frame_texture(
    frame: *const amc_gpu_frame_t,
    out: *mut *mut std::ffi::c_void,
) -> i32 {
    status(crate::domain(), frame as usize, || {
        args::pointer(out)?;
        gpu_frame_unavailable(frame.cast())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_gpu_frame_borrow(frame: *const amc_gpu_frame_t) -> i32 {
    status(crate::domain(), frame as usize, || {
        gpu_frame_unavailable(frame.cast())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_gpu_frame_return(frame: *mut amc_gpu_frame_t) -> i32 {
    status(crate::domain(), frame as usize, || {
        gpu_frame_unavailable(frame.cast())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_gpu_frame_release(frame: *mut amc_gpu_frame_t) -> i32 {
    status(crate::domain(), frame as usize, || {
        gpu_frame_unavailable(frame.cast())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_gpu_frame_stats(
    frame: *const amc_gpu_frame_t,
    out: *mut am_gpu_frame_stats_t,
) -> i32 {
    status(crate::domain(), frame as usize, || {
        args::validate_output(out, std::mem::size_of::<am_gpu_frame_stats_t>())?;
        gpu_frame_unavailable(frame.cast())
    })
}
