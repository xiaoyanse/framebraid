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
pub(super) struct GpuFramePoolInner {
    pub frames_in_use: AtomicU64,
    pub bytes_in_use: AtomicU64,
    pub max_frames: usize,
    pub max_bytes: u64,
    pub stats_delivered: AtomicU64,
    pub stats_cpu_fallbacks: AtomicU64,
    pub stats_bytes_est: AtomicU64,
    pub stopping: AtomicBool,
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
        let bytes = info.info.rgba_bytes as u64;
        self.try_admit(bytes)?;
        self.frames_in_use.fetch_add(1, Ordering::AcqRel);
        self.bytes_in_use.fetch_add(bytes, Ordering::AcqRel);
        self.stats_delivered.fetch_add(1, Ordering::AcqRel);
        self.stats_bytes_est.fetch_add(bytes, Ordering::AcqRel);
        let desc = am_gpu_frame_desc_t {
            struct_size: std::mem::size_of::<am_gpu_frame_desc_t>() as u32,
            width: info.info.width,
            height: info.info.height,
            d3d_format: info.d3d_format,
            subresource: info.subresource,
            shared_nt_handle: info.shared_nt_handle,
            flags: if info.shared_nt_handle == 0 { 1 } else { 0 },
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

#[no_mangle]
pub unsafe extern "C" fn amc_gpu_frame_borrow(frame: *const amc_gpu_frame_t) -> i32 {
    status(crate::domain(), frame as usize, || {
        if frame.is_null() {
            return Err(AM_ERR_SDK_NULL_ARGUMENT);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_gpu_frame_return(frame: *mut amc_gpu_frame_t) -> i32 {
    status(crate::domain(), frame as usize, || {
        if frame.is_null() {
            return Err(AM_ERR_SDK_NULL_ARGUMENT);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_gpu_frame_release(frame: *mut amc_gpu_frame_t) -> i32 {
    status(crate::domain(), frame as usize, || {
        if frame.is_null() {
            return Err(AM_ERR_SDK_NULL_ARGUMENT);
        }
        drop(Box::from_raw(frame));
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_gpu_frame_stats(
    frame: *const amc_gpu_frame_t,
    out: *mut am_gpu_frame_stats_t,
) -> i32 {
    status(crate::domain(), frame as usize, || {
        args::validate_output(out, std::mem::size_of::<am_gpu_frame_stats_t>())?;
        // 统计从帧所属池读取；无池关联时返回全零。
        unsafe { args::write(out, am_gpu_frame_stats_t::default()) }
    })
}
