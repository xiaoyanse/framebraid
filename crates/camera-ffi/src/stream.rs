//! 推流会话导出：amc_stream_open/write/close（FLV/RTMP，断线重连）。
use super::*;
use am_camera_core::media::mux::MuxError;
use am_camera_core::media::stream::StreamSession;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};

use super::encode::{
    encode_error_status, encoder_runtime, read_encoder_config, read_pixel_view, ENCODER_META_BYTES,
};
use super::frames::{camera_state, ControlCharge};

/// 推流错误映射：Io 类（可重连）归 CAMERA_UNREACHABLE，其余同存档。
pub(crate) fn stream_error_status(error: MuxError) -> i32 {
    match error {
        MuxError::Encode(inner) => encode_error_status(inner),
        MuxError::Config(_) => AM_ERR_SDK_INVALID_ARGUMENT,
        MuxError::Io(_) => AM_ERR_CAMERA_UNREACHABLE,
        MuxError::Ffmpeg(_) => AM_ERR_SDK_INTERNAL_ERROR,
    }
}

pub(crate) struct StreamObject {
    pub(crate) session: Mutex<Option<StreamSession>>,
    pub(crate) state: AtomicU32,
    _permit: Mutex<Option<ControlCharge>>,
}

impl am_sdk_support::handles::Managed for StreamObject {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn request_close(&self) {
        self.state
            .store(AM_LIFECYCLE_CLOSING as u32, Ordering::Release);
    }
    fn close_resources(&self) {
        if let Ok(mut guard) = self.session.lock() {
            if let Some(session) = guard.take() {
                session.close();
            }
        }
        if let Ok(mut permit) = self._permit.lock() {
            permit.take();
        }
        self.state
            .store(AM_LIFECYCLE_CLOSED as u32, Ordering::Release);
    }
}

#[no_mangle]
pub unsafe extern "C" fn amc_stream_open(
    ctx: *mut amc_context_t,
    rtmp_url_utf8: *const core::ffi::c_char,
    enc_cfg: *const am_encoder_config_t,
    out: *mut *mut amc_stream_t,
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
        let url = am_sdk_support::args::string(rtmp_url_utf8)?;
        let (settings, _items, _bytes) = unsafe { read_encoder_config(enc_cfg)? };
        if !url.to_ascii_lowercase().starts_with("rtmp://") {
            return Err(AM_ERR_CAMERA_URL_INVALID);
        }
        let api = encoder_runtime(&state)?;
        let permit = state.metadata(ENCODER_META_BYTES)?;
        let session = StreamSession::new(
            api,
            settings,
            std::ffi::CString::new(url)
                .map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?
                .as_c_str(),
        )
        .map_err(stream_error_status)?;
        let object = StreamObject {
            session: Mutex::new(Some(session)),
            state: AtomicU32::new(AM_LIFECYCLE_RUNNING as u32),
            _permit: Mutex::new(Some(permit)),
        };
        let key = domain().insert(ctx as usize, Arc::new(object))?;
        unsafe {
            out.write(key as _);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_stream_write(
    stream: *mut amc_stream_t,
    frame: *const am_pixel_view_t,
) -> i32 {
    status(domain(), stream as usize, || {
        let entry = domain().active(stream as usize)?;
        let object = entry.data::<StreamObject>()?;
        if object.state.load(Ordering::Acquire) != AM_LIFECYCLE_RUNNING as u32 {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let (data, stride, _w, _h) = unsafe { read_pixel_view(frame)? };
        let mut guard = object
            .session
            .try_lock()
            .map_err(|_| AM_ERR_SDK_TRY_AGAIN)?;
        let session = guard.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        unsafe { session.write(data, stride) }.map_err(stream_error_status)?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_stream_close(
    stream: *mut amc_stream_t,
    wait: u32,
    out_state: *mut u32,
) -> i32 {
    status(domain(), stream as usize, || {
        am_sdk_support::args::pointer(out_state)?;
        {
            domain().get(stream as usize)?.data::<StreamObject>()?;
        }
        let state = domain().close(stream as usize, wait)?;
        unsafe {
            out_state.write(state);
        }
        Ok(())
    })
}
