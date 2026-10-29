//! 编码会话导出：amc_encoder_create/encode/drain/close。
use super::*;
use am_camera_core::media::encode::{
    EncodeError, EncodedPacket, Encoder as MediaEncoder, EncoderSettings,
};
use am_camera_core::media::ffmpeg_lib::FfmpegApi;
use std::collections::VecDeque;
use std::mem::size_of;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};

use super::frames::{camera_state, CameraState, Charge, ControlCharge};

pub(crate) const ENCODER_QUEUE_DEFAULT_ITEMS: u32 = 64;
pub(crate) const ENCODER_QUEUE_DEFAULT_BYTES: u64 = SDK_DECODER_INPUT_BYTES;
pub(crate) const ENCODER_META_BYTES: usize = 256;

/// 像素视图提取（RGBA8 限定）：返回数据指针、步长、宽、高。
///
/// # Safety
/// frame 须指向有效 am_pixel_view_t；帧数据在本次调用期间保持可读。
pub(crate) unsafe fn read_pixel_view(
    frame: *const am_pixel_view_t,
) -> Result<(*const u8, usize, u32, u32), i32> {
    am_sdk_support::args::pointer(frame)?;
    let value = unsafe { am_sdk_support::args::read(frame, size_of::<am_pixel_view_t>()) }?;
    if value.magic != AM_PIXEL_VIEW_MAGIC {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    am_sdk_support::args::pointer(value.view)?;
    let view = unsafe { &*value.view };
    if view.format != AM_PF_RGBA8 as u32 {
        return Err(AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED);
    }
    let (w, h) = (view.width, view.height);
    if w == 0
        || h == 0
        || (view.strides[0] as usize) < w as usize * 4
        || (view.strides[0] as usize)
            .checked_mul(h as usize - 1)
            .and_then(|tail| tail.checked_add(w as usize * 4))
            .is_none_or(|needed| view.plane_size_bytes[0] < needed as u64)
    {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    if am_sdk_support::args::pointer(view.planes[0]).is_err() {
        return Err(AM_ERR_SDK_NULL_ARGUMENT);
    }
    Ok((view.planes[0], view.strides[0] as usize, w, h))
}

/// 编码错误映射到状态码。
pub(crate) fn encode_error_status(error: EncodeError) -> i32 {
    match error {
        EncodeError::Library(_) => AM_ERR_SDK_NOT_SUPPORTED,
        EncodeError::Config(_) => AM_ERR_SDK_INVALID_ARGUMENT,
        EncodeError::Ffmpeg(_) => AM_ERR_SDK_INTERNAL_ERROR,
        EncodeError::PixelInvalid => AM_ERR_SDK_INVALID_ARGUMENT,
        EncodeError::InputAfterFlush => AM_ERR_SDK_DRAINED,
    }
}

pub(crate) struct QueuedPacket {
    pub(crate) data: Vec<u8>,
    pub(crate) keyframe: bool,
    pub(crate) _charge: Charge,
}

pub(crate) struct Inner {
    pub(crate) encoder: MediaEncoder,
    pub(crate) queue: VecDeque<QueuedPacket>,
    pub(crate) queue_bytes: u64,
    queue_budget_items: u32,
    queue_budget_bytes: u64,
    pub(crate) drained: bool,
}

pub(crate) struct EncoderObject {
    pub(crate) inner: Mutex<Option<Inner>>,
    pub(crate) state: AtomicU32,
    pub(crate) ctx_state: Arc<CameraState>,
    _permit: Mutex<Option<ControlCharge>>,
}

impl am_sdk_support::handles::Managed for EncoderObject {
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

/// 队列预算展开：0 取默认，超上限拒绝。
fn normalize_queue_budget(items: u32, bytes: u64) -> Result<(u32, u64), i32> {
    let items = if items == 0 {
        ENCODER_QUEUE_DEFAULT_ITEMS
    } else {
        items
    };
    let bytes = if bytes == 0 {
        ENCODER_QUEUE_DEFAULT_BYTES
    } else {
        bytes
    };
    if items > SDK_DECODER_INPUT_ITEMS as u32 || bytes > SDK_DECODER_INPUT_BYTES {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    Ok((items, bytes))
}

/// 配置读取与校验（界检查经 media 层 normalized）。
pub(crate) unsafe fn read_encoder_config(
    cfg: *const am_encoder_config_t,
) -> Result<(EncoderSettings, u32, u64), i32> {
    let abi = unsafe { am_sdk_support::args::read(cfg, size_of::<am_encoder_config_t>()) }?;
    let settings = EncoderSettings {
        width: abi.width,
        height: abi.height,
        bitrate_kbps: abi.bitrate_kbps,
        gop_size: abi.gop_size,
        fps_num: abi.fps_num,
        fps_den: abi.fps_den,
    }
    .normalized()
    .map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?;
    let (items, bytes) =
        normalize_queue_budget(abi.queue_max_items, u64::from(abi.queue_max_bytes))?;
    Ok((settings, items, bytes))
}

/// FFmpeg 运行库装载（复用解码器运行库绑定，同进程唯一目录）。
pub(crate) fn encoder_runtime(state: &CameraState) -> Result<Arc<FfmpegApi>, i32> {
    crate::decoder::config::runtime(state)
}

fn enqueue_packets(
    inner: &mut Inner,
    state: &Arc<CameraState>,
    packets: Vec<EncodedPacket>,
) -> Result<usize, i32> {
    let mut enqueued = 0usize;
    for packet in packets {
        let bytes = packet.data.len() as u64 + 64;
        let charge = state.charge(bytes, false)?;
        let (max_items, max_bytes) = (inner.queue_budget_items, inner.queue_budget_bytes);
        if inner.queue.len() >= max_items as usize
            || bytes > max_bytes.saturating_sub(inner.queue_bytes)
        {
            drop(charge);
            return Err(AM_ERR_SDK_QUEUE_FULL);
        }
        inner.queue_bytes += bytes;
        inner.queue.push_back(QueuedPacket {
            data: packet.data,
            keyframe: packet.keyframe,
            _charge: charge,
        });
        enqueued += 1;
    }
    Ok(enqueued)
}

#[no_mangle]
pub unsafe extern "C" fn amc_encoder_create(
    ctx: *mut amc_context_t,
    cfg: *const am_encoder_config_t,
    out_enc: *mut *mut amc_encoder_t,
) -> i32 {
    status(domain(), ctx as usize, || {
        am_sdk_support::args::pointer(out_enc)?;
        unsafe {
            out_enc.write(std::ptr::null_mut());
        }
        if !cfg!(all(windows, target_arch = "x86_64")) {
            return Err(AM_ERR_SDK_NOT_SUPPORTED);
        }
        let _active = domain().active(ctx as usize)?;
        let state = camera_state(ctx as usize)?;
        let (settings, items, bytes) = unsafe { read_encoder_config(cfg)? };
        let api = encoder_runtime(&state)?;
        let encoder = MediaEncoder::new(api, settings, false).map_err(encode_error_status)?;
        let permit = state.metadata(ENCODER_META_BYTES)?;
        let object = Arc::new(EncoderObject {
            inner: Mutex::new(Some(Inner {
                encoder,
                queue: VecDeque::new(),
                queue_bytes: 0,
                queue_budget_items: items,
                queue_budget_bytes: bytes,
                drained: false,
            })),
            state: AtomicU32::new(AM_LIFECYCLE_RUNNING as u32),
            ctx_state: state,
            _permit: Mutex::new(Some(permit)),
        });
        let key = domain().insert(ctx as usize, object)?;
        unsafe {
            out_enc.write(key as _);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_encoder_encode(
    enc: *mut amc_encoder_t,
    frame: *const am_pixel_view_t,
) -> i32 {
    status(domain(), enc as usize, || {
        let entry = domain().active(enc as usize)?;
        let object = entry.data::<EncoderObject>()?;
        if object.state.load(Ordering::Acquire) != AM_LIFECYCLE_RUNNING as u32 {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let (data, stride, _w, _h) = unsafe { read_pixel_view(frame)? };
        let mut guard = object.inner.try_lock().map_err(|_| AM_ERR_SDK_QUEUE_FULL)?;
        let inner = guard.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        if inner.drained {
            return Err(AM_ERR_SDK_DRAINED);
        }
        let packets =
            unsafe { inner.encoder.encode_rgba(data, stride) }.map_err(encode_error_status)?;
        enqueue_packets(inner, &object.ctx_state, packets)?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_encoder_receive(
    enc: *mut amc_encoder_t,
    wait: u32,
    out_pkt: *mut *mut am_encoded_packet_t,
) -> i32 {
    status(domain(), enc as usize, || {
        am_sdk_support::args::pointer(out_pkt)?;
        unsafe {
            out_pkt.write(std::ptr::null_mut());
        }
        let entry = domain().active(enc as usize)?;
        let object = entry.data::<EncoderObject>()?;
        let until = std::time::Instant::now() + std::time::Duration::from_millis(wait as u64);
        loop {
            if entry.state.load(std::sync::atomic::Ordering::Acquire) != AM_LIFECYCLE_RUNNING as u32
            {
                return Err(AM_ERR_SDK_HANDLE_CLOSED);
            }
            if let Ok(mut guard) = object.inner.try_lock() {
                let inner = guard.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
                if let Some(queued) = inner.queue.pop_front() {
                    let accounted = queued.data.len() as u64 + 64;
                    inner.queue_bytes = inner.queue_bytes.saturating_sub(accounted);
                    let packet = crate::subscriptions::queue::Item::Encoded(std::sync::Arc::new(
                        crate::subscriptions::queue::Payload {
                            abi: am_encoded_packet_t {
                                codec: AM_CODEC_H264,
                                bitstream_format: AM_BSF_ANNEXB,
                                flags: AM_PKT_AU_COMPLETE
                                    | if queued.keyframe { AM_PKT_KEYFRAME } else { 0 },
                                ..Default::default()
                            },
                            bytes: queued.data,
                            _charge: queued._charge,
                        },
                    ));
                    let key = packet.transfer(&object.ctx_state)?;
                    unsafe {
                        out_pkt.write(key as _);
                    }
                    return Ok(());
                }
            }
            if wait == 0 {
                return Err(AM_ERR_SDK_TRY_AGAIN);
            }
            if std::time::Instant::now() >= until {
                return Err(AM_ERR_SDK_DEADLINE_EXCEEDED);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_encoder_drain(enc: *mut amc_encoder_t) -> i32 {
    status(domain(), enc as usize, || {
        let entry = domain().active(enc as usize)?;
        let object = entry.data::<EncoderObject>()?;
        if object.state.load(Ordering::Acquire) != AM_LIFECYCLE_RUNNING as u32 {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let mut guard = object.inner.try_lock().map_err(|_| AM_ERR_SDK_TRY_AGAIN)?;
        let inner = guard.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        let packets = inner.encoder.drain().map_err(encode_error_status)?;
        inner.drained = true;
        enqueue_packets(inner, &object.ctx_state, packets)?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_encoder_close(
    enc: *mut amc_encoder_t,
    wait: u32,
    out_state: *mut u32,
) -> i32 {
    status(domain(), enc as usize, || {
        am_sdk_support::args::pointer(out_state)?;
        {
            domain().get(enc as usize)?.data::<EncoderObject>()?;
        }
        let state = domain().close(enc as usize, wait)?;
        unsafe {
            out_state.write(state);
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests;
