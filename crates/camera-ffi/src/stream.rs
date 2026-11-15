//! 推流会话导出：amc_stream_open/write/close（FLV/RTMP，断线重连）。
use super::*;
use am_camera_core::media::mux::MuxError;
use am_camera_core::media::stream::StreamSession;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};

use super::encode::{
    encode_error_status, encoder_runtime, read_encoder_config, read_pixel_view, ENCODER_META_BYTES,
};
use super::frames::{camera_state, ControlCharge};

#[cfg(test)]
mod tests;

/// 推流错误映射：Io 类（可重连）归 CAMERA_UNREACHABLE，其余同存档。
pub(crate) fn stream_error_status(error: MuxError) -> i32 {
    match error {
        MuxError::Encode(inner) => encode_error_status(inner),
        MuxError::Config(_) => AM_ERR_SDK_INVALID_ARGUMENT,
        MuxError::Io(_) => AM_ERR_CAMERA_UNREACHABLE,
        MuxError::Timeout => AM_ERR_CAMERA_TIMEOUT,
        MuxError::Ffmpeg(_) => AM_ERR_SDK_INTERNAL_ERROR,
    }
}

pub(crate) struct StreamObject {
    pub(crate) session: Mutex<Option<StreamSession>>,
    pub(crate) state: AtomicU32,
    /// 编码配置尺寸：输入视图必须与之一致，否则底层按配置行数读取会越界。
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// 停止信号：close/关闭即置位，唤醒 write 重连退避并中断连接 IO。
    pub(crate) stop: Arc<AtomicBool>,
    _permit: Mutex<Option<ControlCharge>>,
}

impl am_sdk_support::handles::Managed for StreamObject {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn request_close(&self) {
        // 先置停止信号再标记关闭：阻塞在重连退避的 write 能立即退出并让出会话锁。
        self.stop.store(true, Ordering::Release);
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
        let (width, height) = (settings.width, settings.height);
        if !url.to_ascii_lowercase().starts_with("rtmp://") {
            return Err(AM_ERR_CAMERA_URL_INVALID);
        }
        let api = encoder_runtime(&state)?;
        let permit = state.metadata(ENCODER_META_BYTES)?;
        let stop = Arc::new(AtomicBool::new(false));
        let session = StreamSession::new(
            api,
            settings,
            std::ffi::CString::new(url)
                .map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?
                .as_c_str(),
            stop.clone(),
        )
        .map_err(stream_error_status)?;
        let object = StreamObject {
            session: Mutex::new(Some(session)),
            state: AtomicU32::new(AM_LIFECYCLE_RUNNING as u32),
            width,
            height,
            stop,
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
        let (data, stride, w, h) = unsafe { read_pixel_view(frame)? };
        // 视图自洽不等于可用：底层按创建配置的尺寸读取，宽高不一致直接拒绝。
        if w != object.width || h != object.height {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
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

/// 推流状态只读快照（ABI 1.7）：无状态迁移；write（含重连）进行中并发查询
/// 返回 sdk.tryAgain（同 write 并发语义，可重试非错误）。
#[no_mangle]
pub unsafe extern "C" fn amc_stream_get_status(
    stream: *mut amc_stream_t,
    out_status: *mut am_stream_status_t,
) -> i32 {
    status(domain(), stream as usize, || {
        // 输出指针先校验（NULL/对齐/struct_size 版本协商），再取会话快照。
        am_sdk_support::args::validate_output(
            out_status,
            std::mem::size_of::<am_stream_status_t>(),
        )?;
        let entry = domain().get(stream as usize)?;
        let object = entry.data::<StreamObject>()?;
        // 与 decoder get_status 同款 try_lock：write/重连持锁期间不阻塞状态查询调用方。
        let guard = object
            .session
            .try_lock()
            .map_err(|_| AM_ERR_SDK_TRY_AGAIN)?;
        let session = guard.as_ref().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        let snapshot = session.status();
        let value = am_stream_status_t {
            connection_state: if snapshot.connected {
                AM_STREAM_CONNECTED
            } else {
                AM_STREAM_DISCONNECTED
            },
            reconnect_attempts: snapshot.reconnect_attempts,
            bytes_written: snapshot.bytes_written,
            backoff_ms: snapshot.last_backoff_ms as u32,
            last_error_status: snapshot
                .last_error
                .map(stream_error_status)
                .unwrap_or(AM_OK as am_status_t),
            ..Default::default()
        };
        unsafe { am_sdk_support::args::write(out_status, value) }
    })
}
