//! GPU 帧引用单帧导出（§2.12 兼容新增）：D3D11 staging 回读 NV12 后走既有
//! frame_export 全链；租约借用至返回，不跨调用持有纹理。
use super::*;

/// GPU 引用合法性（判定复用推理 GPU 帧输入先例：NV12(103)/纹理非空/尺寸界；
/// 编码另需 NV12 偶数宽高）。
pub(crate) fn validate_gpu_ref(gpu: &am_gpu_frame_ref_t) -> Result<(), i32> {
    if gpu.d3d_format != 103
        || gpu.texture.is_null()
        || gpu.width == 0
        || gpu.height == 0
        || gpu.width > 8192
        || gpu.height > 8192
        || !gpu.width.is_multiple_of(2)
        || !gpu.height.is_multiple_of(2)
    {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    Ok(())
}

#[no_mangle]
pub unsafe extern "C" fn amc_frame_encode_gpu(
    ctx: *mut amc_context_t,
    gpu: *const am_gpu_frame_ref_t,
    opts: *const am_frame_encode_opts_t,
    out: *mut u8,
    capacity: u64,
    out_size: *mut u64,
) -> i32 {
    status(domain(), ctx as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        am_sdk_support::args::pointer(out_size)?;
        unsafe {
            out_size.write(0);
        }
        let _ = domain().active(ctx as usize)?;
        let (format, quality) = unsafe { validate_encode_opts(opts)? };
        let gpu: am_gpu_frame_ref_t =
            unsafe { am_sdk_support::args::read(gpu, size_of::<am_gpu_frame_ref_t>()) }?;
        validate_gpu_ref(&gpu)?;
        let host = unsafe {
            crate::decoder::gpu_pool::readback_nv12(
                gpu.texture,
                gpu.subresource,
                gpu.width,
                gpu.height,
            )?
        };
        let (w, h) = (gpu.width as usize, gpu.height as usize);
        let pitch = host.row_pitch;
        let uv_rows = h / 2;
        let y_len = pitch * (h - 1) + w;
        let uv_len = pitch * (uv_rows - 1) + w;
        let y_plane = &host.data[..y_len];
        let uv_plane = &host.data[pitch * h..pitch * h + uv_len];
        // am_gpu_frame_ref_t 无色彩字段：固定 BT.601 limited，与解码器 NV12 主
        // 形态及推理线 VP 固定 BT.601 同域。
        let input = am_camera_core::frame_export::FrameInput {
            planes: &[y_plane, uv_plane],
            strides: &[pitch, pitch],
            width: gpu.width,
            height: gpu.height,
            format: am_camera_core::frame_export::PF_NV12,
            color_matrix: 0,
            color_range: 0,
        };
        // 回读与编码同为无状态单次瞬态，不入域账（与 amc_frame_encode 同口径）。
        encode_frame_to_sink(&input, format, quality, out, capacity, out_size)
    })
}

#[cfg(test)]
mod tests;
