//! MP4 存档会话导出：amc_archive_open/write/close（segment_frames 分段）。
use super::*;
use am_camera_core::media::encode::EncoderSettings;
use am_camera_core::media::mux::{MuxError, MuxFormat, Muxer};
use std::ffi::CString;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};

use super::encode::{
    encode_error_status, encoder_runtime, read_encoder_config, read_pixel_view, ENCODER_META_BYTES,
};
use super::frames::{camera_state, CameraState, ControlCharge};

pub(crate) fn segment_path(template: &str, index: u32) -> String {
    template.replacen("%d", &index.to_string(), 1)
}

/// 封装错误映射：Io 归档为 CAMERA_UNREACHABLE 之外的文件错误 → 内部错误；
/// 存档侧 Io 属文件系统问题，统一内部错误，配置类归 invalidArgument。
pub(crate) fn archive_error_status(error: MuxError) -> i32 {
    match error {
        MuxError::Encode(inner) => encode_error_status(inner),
        MuxError::Config(_) => AM_ERR_SDK_INVALID_ARGUMENT,
        MuxError::Ffmpeg(_) | MuxError::Io(_) => AM_ERR_SDK_INTERNAL_ERROR,
    }
}

struct Inner {
    mux: Option<Muxer>,
    frames_in_segment: u32,
    segment_index: u32,
}

struct ArchiveObject {
    inner: Mutex<Option<Inner>>,
    state: AtomicU32,
    ctx_state: Arc<CameraState>,
    settings: EncoderSettings,
    path_template: String,
    segment_frames: u32,
    _permit: Mutex<Option<ControlCharge>>,
}

impl am_sdk_support::handles::Managed for ArchiveObject {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn request_close(&self) {
        self.state
            .store(AM_LIFECYCLE_CLOSING as u32, Ordering::Release);
    }
    fn close_resources(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.take();
        }
        if let Ok(mut permit) = self._permit.lock() {
            permit.take();
        }
        self.state
            .store(AM_LIFECYCLE_CLOSED as u32, Ordering::Release);
    }
}

impl ArchiveObject {
    fn open_segment(&self, index: u32) -> Result<Muxer, i32> {
        let path = segment_path(&self.path_template, index);
        let mut mux = Muxer::create(
            encoder_runtime(&self.ctx_state)?,
            self.settings,
            MuxFormat::Mp4,
        )
        .map_err(archive_error_status)?;
        let target = CString::new(path).map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?;
        mux.connect_file(&target).map_err(archive_error_status)?;
        Ok(mux)
    }

    /// 冲刷 trailer 并关闭当前文件（close 导出与分段滚动共用）。
    fn flush_segment(inner: &mut Inner) {
        if let Some(mut mux) = inner.mux.take() {
            let _ = mux.finish();
        }
        inner.frames_in_segment = 0;
    }
}

#[no_mangle]
pub unsafe extern "C" fn amc_archive_open(
    ctx: *mut amc_context_t,
    path_utf8: *const core::ffi::c_char,
    enc_cfg: *const am_encoder_config_t,
    segment_frames: u32,
    out: *mut *mut amc_archive_t,
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
        let path = am_sdk_support::args::string(path_utf8)?;
        let (settings, _items, _bytes) = unsafe { read_encoder_config(enc_cfg)? };
        if segment_frames > 0 && !path.contains("%d") {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let mut object = ArchiveObject {
            inner: Mutex::new(None),
            state: AtomicU32::new(AM_LIFECYCLE_RUNNING as u32),
            ctx_state: state.clone(),
            settings,
            path_template: path,
            segment_frames,
            _permit: Mutex::new(Some(state.metadata(ENCODER_META_BYTES)?)),
        };
        let mux = object.open_segment(0)?;
        object.inner = Mutex::new(Some(Inner {
            mux: Some(mux),
            frames_in_segment: 0,
            segment_index: 0,
        }));
        let key = domain().insert(ctx as usize, Arc::new(object))?;
        unsafe {
            out.write(key as _);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_archive_write(
    arch: *mut amc_archive_t,
    frame: *const am_pixel_view_t,
) -> i32 {
    status(domain(), arch as usize, || {
        let entry = domain().active(arch as usize)?;
        let object = entry.data::<ArchiveObject>()?;
        if object.state.load(Ordering::Acquire) != AM_LIFECYCLE_RUNNING as u32 {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let (data, stride, _w, _h) = unsafe { read_pixel_view(frame)? };
        let mut guard = object.inner.try_lock().map_err(|_| AM_ERR_SDK_TRY_AGAIN)?;
        let inner = guard.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        if inner.mux.is_none() {
            let mux = object.open_segment(inner.segment_index)?;
            inner.mux = Some(mux);
        }
        let result = match inner.mux.as_mut() {
            Some(mux) => unsafe { mux.write(data, stride) }.map_err(archive_error_status),
            None => Err(AM_ERR_SDK_INTERNAL_ERROR),
        };
        result?;
        inner.frames_in_segment += 1;
        if object.segment_frames > 0 && inner.frames_in_segment >= object.segment_frames {
            ArchiveObject::flush_segment(inner);
            inner.segment_index += 1;
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_archive_close(
    arch: *mut amc_archive_t,
    wait: u32,
    out_state: *mut u32,
) -> i32 {
    status(domain(), arch as usize, || {
        am_sdk_support::args::pointer(out_state)?;
        let flush = {
            let entry = domain().get(arch as usize)?;
            let object = entry.data::<ArchiveObject>()?;
            let mut guard = object.inner.try_lock().map_err(|_| AM_ERR_SDK_TRY_AGAIN)?;
            if let Some(inner) = guard.as_mut() {
                ArchiveObject::flush_segment(inner);
            }
            Ok::<(), i32>(())
        };
        let state = domain().close(arch as usize, wait)?;
        unsafe {
            out_state.write(state);
        }
        flush
    })
}
