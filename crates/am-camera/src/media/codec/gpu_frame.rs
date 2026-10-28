//! GPU 帧元数据提取（M2a）：硬解帧旁路 CPU 回读，返回 D3D11 纹理信息。
use super::{CpuDecoder, FrameInfo, Result};
use crate::media::ffmpeg_sys::AV_PIX_FMT_D3D11;

/// GPU 帧元数据（与 ABI `am_gpu_frame_desc_t` 字段一一对应）。
#[derive(Debug, Clone)]
pub struct GpuFrameInfo {
    pub info: FrameInfo,
    pub d3d_format: u32,
    pub subresource: u32,
    /// 共享 NT 句柄；0 = 不可共享（降级候选，由调用方决定回退）。
    pub shared_nt_handle: u64,
}

impl CpuDecoder {
    /// 硬解帧 GPU 元数据提取：不回读像素，只读 AVFrame 字段与纹理属性。
    /// 帧本身仍由 decoder 持有（`retain` 前调用方须先取完元数据）。
    pub fn gpu_frame_info(&self, info: FrameInfo) -> Result<GpuFrameInfo> {
        let frame =
            unsafe { self.frame.as_ref() }.ok_or(crate::media::codec::DecodeError::InvalidInput)?;
        if frame.format != AV_PIX_FMT_D3D11 {
            return Err(crate::media::codec::DecodeError::InvalidInput);
        }
        let texture = frame.data[0];
        if texture.is_null() {
            return Err(crate::media::codec::DecodeError::InvalidInput);
        }
        let subresource = frame.data[1] as usize as u32;
        let (d3d_format, shared_nt_handle) =
            unsafe { crate::media::hardware::texture_shared_info(texture.cast()) };
        Ok(GpuFrameInfo {
            info,
            d3d_format,
            subresource,
            shared_nt_handle,
        })
    }

    /// 硬解帧的原始纹理指针（借用，不转移所有权；retain 前有效）。
    pub fn gpu_texture(&self) -> Result<*mut std::ffi::c_void> {
        let frame =
            unsafe { self.frame.as_ref() }.ok_or(crate::media::codec::DecodeError::InvalidInput)?;
        if frame.format != AV_PIX_FMT_D3D11 {
            return Err(crate::media::codec::DecodeError::InvalidInput);
        }
        let texture = frame.data[0];
        if texture.is_null() {
            return Err(crate::media::codec::DecodeError::InvalidInput);
        }
        Ok(texture.cast())
    }
}
