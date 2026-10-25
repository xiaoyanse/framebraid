use super::*;

pub fn bytes(w: u32, h: u32, format: u32) -> Result<usize> {
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    let n = w as usize * h as usize;
    match format as i32 {
        AM_PF_RGBA8 => Ok(n * 4),
        AM_PF_RGB8 | AM_PF_BGR8 => Ok(n * 3),
        AM_PF_NV12 if w.is_multiple_of(2) && h.is_multiple_of(2) => Ok(n * 3 / 2),
        _ => Err(AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED),
    }
}
#[no_mangle]
pub unsafe extern "C" fn amc_frame_get_scaled(
    input: *const amc_frame_t,
    w: u32,
    h: u32,
    method: u32,
    out: *mut *mut amc_frame_t,
) -> i32 {
    status(domain(), 0, || {
        args::pointer(out)?;
        unsafe {
            out.write(std::ptr::null_mut());
        }
        if method > 1 {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let body = frame(input as usize)?;
        let f = body
            .as_any()
            .downcast_ref::<Frame>()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
        let size = bytes(w, h, f.view.format)?;
        let state = &f._charge.state;
        if state.ctx.state.load(Ordering::Acquire) != AM_LIFECYCLE_RUNNING as u32 {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let _active = state.ctx.readable()?;
        if state.ctx.state.load(Ordering::Acquire) != AM_LIFECYCLE_RUNNING as u32 {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let charge = state.charge(size as u64, true)?;
        let mut data = vec![0; size];
        let ow = f.view.width;
        let oh = f.view.height;
        let nv12 = f.view.format == AM_PF_NV12 as u32;
        let channels = if nv12 {
            1
        } else if f.view.format == AM_PF_RGBA8 as u32 {
            4
        } else {
            3
        };
        am_camera_core::media::pixel_ops::resize_plane(
            &f.data[..f.view.plane_size_bytes[0] as usize],
            (ow, oh),
            &mut data[..if nv12 { (w * h) as usize } else { size }],
            (w, h),
            channels,
            method,
        )
        .map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?;
        if nv12 {
            am_camera_core::media::pixel_ops::resize_plane(
                &f.data[(ow * oh) as usize..],
                (ow / 2, oh / 2),
                &mut data[(w * h) as usize..],
                (w / 2, h / 2),
                2,
                method,
            )
            .map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?;
        }
        let mut meta = *f.meta;
        let t = &mut meta.transform;
        if t.valid != 0 && t.rotation == 0 && t.offset_x == 0 && t.offset_y == 0 {
            t.scale_x *= w as f32 / ow as f32;
            t.scale_y *= h as f32 / oh as f32;
        } else {
            t.valid = 0;
        }
        let result = Frame::with_format(data, w, h, f.view.format, meta, charge)?;
        let pointer = Arc::as_ptr(&result) as usize;
        domain().buffers.insert(pointer, result)?;
        unsafe {
            out.write(pointer as _);
        }
        Ok(())
    })
}
