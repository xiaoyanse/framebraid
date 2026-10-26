//! 显式CPU目标缓冲避免原生transfer pool在Charge归还后继续缓存图片。
use super::*;

pub(super) struct Plan {
    pub format: i32,
    pub width: i32,
    pub height: i32,
    pub bytes: usize,
}
struct Formats<'a> {
    api: &'a FfmpegApi,
    pointer: *mut i32,
}
impl Drop for Formats<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.av_free)(self.pointer.cast()) };
    }
}
impl Plan {
    pub(super) unsafe fn query(
        api: &FfmpegApi,
        frames: *mut AVBufferRef,
        info: FrameInfo,
    ) -> Result<Self> {
        if frames.is_null() {
            return Err(DecodeError::InvalidInput);
        }
        let mut formats = Formats {
            api,
            pointer: ptr::null_mut(),
        };
        // FROM=0，返回列表由同一libavutil释放；失败留下的非空列表也须释放。
        let rc =
            unsafe { (api.av_hwframe_transfer_get_formats)(frames, 0, &mut formats.pointer, 0) };
        if rc < 0 {
            return Err(DecodeError::Runtime(rc));
        }
        if formats.pointer.is_null() {
            return Err(DecodeError::InvalidInput);
        }
        let mut selected = None;
        for index in 0..64 {
            let format = unsafe { *formats.pointer.add(index) };
            if format == -1 {
                break;
            }
            // 固定libavutil61：NV12=23、P010LE=158；不猜其他GPU表面布局。
            if format == AV_PIX_FMT_NV12 || format == 158 {
                selected = Some(format);
                break;
            }
        }
        let format = selected.ok_or(DecodeError::InvalidInput)?;
        let height = info
            .height
            .checked_add(31)
            .ok_or(DecodeError::FrameTooLarge)?
            & !31;
        let bytes =
            unsafe { (api.av_image_get_buffer_size)(format, info.width as i32, height as i32, 64) };
        if bytes <= 0 {
            return Err(DecodeError::Runtime(bytes));
        }
        let bytes = (bytes as usize)
            .checked_add(512)
            .ok_or(DecodeError::FrameTooLarge)?;
        Ok(Self {
            format,
            width: info.width as i32,
            height: info.height as i32,
            bytes,
        })
    }
    pub(super) unsafe fn allocate(&self, api: &FfmpegApi, frame: *mut AVFrame) -> Result<()> {
        unsafe {
            (*frame).format = self.format;
            (*frame).width = self.width;
            (*frame).height = self.height;
        }
        let rc = unsafe { (api.av_frame_get_buffer)(frame, 64) };
        if rc < 0 {
            return Err(DecodeError::Runtime(rc));
        }
        let buffer = unsafe { (api.av_frame_get_plane_buffer)(frame, 0).as_ref() }
            .ok_or(DecodeError::AllocationFailed)?;
        if buffer.size > self.bytes {
            return Err(DecodeError::FrameTooLarge);
        }
        Ok(())
    }
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::*;
    #[test]
    fn buffer_reference_layout_matches_libavutil_61() {
        assert_eq!(std::mem::size_of::<AVBufferRef>(), 24);
        assert_eq!(std::mem::offset_of!(AVBufferRef, data), 8);
        assert_eq!(std::mem::offset_of!(AVBufferRef, size), 16);
    }
}
