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
    /// 编码器时基下的原始时间戳（issue #7）：贯通到输出结构，不丢时间线。
    pub(crate) pts: i64,
    pub(crate) dts: i64,
    pub(crate) _charge: Charge,
}

pub(crate) struct Inner {
    pub(crate) encoder: MediaEncoder,
    pub(crate) queue: VecDeque<QueuedPacket>,
    pub(crate) queue_bytes: u64,
    queue_budget_items: u32,
    queue_budget_bytes: u64,
    pub(crate) drained: bool,
    /// 队满时的溢出包暂存（审计 A5）：投递失败不丢包也不部分入队，
    /// receive 释放空间后由下一次 encode/drain 自动补投。
    pub(crate) pending: Vec<EncodedPacket>,
}

pub(crate) struct EncoderObject {
    pub(crate) inner: Mutex<Option<Inner>>,
    pub(crate) state: AtomicU32,
    pub(crate) ctx_state: Arc<CameraState>,
    /// 编码配置尺寸：输入视图必须与之一致，否则底层按配置行数读取会越界。
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// 编码器时基（1/fps）：输出包 PTS/DTS 的单位（issue #7 时间线贯通）。
    pub(crate) time_base_num: u32,
    pub(crate) time_base_den: u32,
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

/// 整批入队：任一包放不下即整批拒收（事务性，杜绝"部分入队后报错"的中间
/// 态——审计 A5）。`stash_on_full` 仅 drain 尾包为真：编码器已 flush、包不可
/// 再生，必须暂存待补投；编码路径报错即整帧拒收，调用方保留重发权。
fn enqueue_packets(
    inner: &mut Inner,
    state: &Arc<CameraState>,
    packets: Vec<EncodedPacket>,
    stash_on_full: bool,
) -> Result<usize, i32> {
    if packets.is_empty() {
        return Ok(0);
    }
    let fits = packets.iter().all(|packet| {
        let bytes = packet.data.len() as u64 + 64;
        inner.queue.len() < inner.queue_budget_items as usize
            && bytes <= inner.queue_budget_bytes.saturating_sub(inner.queue_bytes)
    });
    if !fits {
        if stash_on_full {
            inner.pending.extend(packets);
        }
        return Err(AM_ERR_SDK_QUEUE_FULL);
    }
    let baseline = inner.queue.len();
    let mut enqueued = 0usize;
    for packet in packets {
        let bytes = packet.data.len() as u64 + 64;
        let charge = match state.charge(bytes, false) {
            Ok(charge) => charge,
            Err(error) => {
                // 计费失败回滚本轮已入队项，保持整批事务性。
                while inner.queue.len() > baseline {
                    if let Some(queued) = inner.queue.pop_back() {
                        inner.queue_bytes = inner
                            .queue_bytes
                            .saturating_sub(queued.data.len() as u64 + 64);
                    }
                }
                return Err(error);
            }
        };
        inner.queue_bytes += bytes;
        inner.queue.push_back(QueuedPacket {
            data: packet.data,
            keyframe: packet.keyframe,
            pts: packet.pts,
            dts: packet.dts,
            _charge: charge,
        });
        enqueued += 1;
    }
    Ok(enqueued)
}

/// 补投 drain 暂存尾包：队满溢出的尾包在下一次 drain 时优先投递，投不下
/// 即未完成（QUEUE_FULL），receive 腾出空间后重试。
fn deliver_pending(inner: &mut Inner, state: &Arc<CameraState>) -> Result<(), i32> {
    if inner.pending.is_empty() {
        return Ok(());
    }
    let pending = std::mem::take(&mut inner.pending);
    enqueue_packets(inner, state, pending, true).map(|_| ())
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
        let (width, height) = (settings.width, settings.height);
        let (time_base_num, time_base_den) = (settings.fps_den, settings.fps_num);
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
                pending: Vec::new(),
            })),
            state: AtomicU32::new(AM_LIFECYCLE_RUNNING as u32),
            ctx_state: state,
            width,
            height,
            time_base_num,
            time_base_den,
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
        let (data, stride, w, h) = unsafe { read_pixel_view(frame)? };
        // 视图自洽不等于可用：底层按创建配置的尺寸读取，宽高不一致直接拒绝。
        if w != object.width || h != object.height {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let mut guard = object.inner.try_lock().map_err(|_| AM_ERR_SDK_QUEUE_FULL)?;
        let inner = guard.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        if inner.drained {
            return Err(AM_ERR_SDK_DRAINED);
        }
        let packets =
            unsafe { inner.encoder.encode_rgba(data, stride) }.map_err(encode_error_status)?;
        enqueue_packets(inner, &object.ctx_state, packets, false)?;
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
                // 队列空且暂存非空时先补投（审计 F7 配套）：上次移交失败的包
                // 经 pending 暂存，资源释放后重试 receive 可取回同一包。
                if inner.queue.is_empty() && !inner.pending.is_empty() {
                    deliver_pending(inner, &object.ctx_state)?;
                }
                if let Some(queued) = inner.queue.pop_front() {
                    let accounted = queued.data.len() as u64 + 64;
                    inner.queue_bytes = inner.queue_bytes.saturating_sub(accounted);
                    // F7 预备：data 即将 move 进移交 Payload，失败暂存副本先行备好
                    // （每包一次小拷贝，占编码耗时 <1%）；成功路径副本随作用域丢弃。
                    let stash = EncodedPacket {
                        data: queued.data.clone(),
                        keyframe: queued.keyframe,
                        pts: queued.pts,
                        dts: queued.dts,
                    };
                    let packet = crate::subscriptions::queue::Item::Encoded(std::sync::Arc::new(
                        crate::subscriptions::queue::Payload {
                            abi: am_encoded_packet_t {
                                codec: AM_CODEC_H264,
                                bitstream_format: AM_BSF_ANNEXB,
                                flags: AM_PKT_AU_COMPLETE
                                    | if queued.keyframe { AM_PKT_KEYFRAME } else { 0 },
                                // 时间线贯通（issue #7）：编码器时基 1/fps 下的原始
                                // PTS/DTS（帧序号语义，next_pts 逐帧递增）；当前
                                // 公开入口无输入时间参数，为固定帧率合同，保留
                                // 输入 PTS 属后续合同演进。
                                pts: queued.pts,
                                dts: queued.dts,
                                time_base_num: object.time_base_num,
                                time_base_den: object.time_base_den,
                                ..Default::default()
                            },
                            bytes: queued.data,
                            _charge: queued._charge,
                        },
                    ));
                    let key = match packet.transfer(&object.ctx_state) {
                        Ok(key) => key,
                        Err(error) => {
                            // 移交失败（审计 F7）：包转入 pending 暂存——原 charge 随
                            // Payload 归还，暂存期不占队列计数；调用方释放结果缓冲后
                            // 重试 receive 会先补投 pending 取回同一包，不静默丢失。
                            inner.pending.push(stash);
                            return Err(error);
                        }
                    };
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
        // 先补投上次队满暂存的尾包；投不下即未完成（QUEUE_FULL），调用方
        // receive 后重试，不会把尾包静默丢失再报空成功（审计 A5）。
        deliver_pending(inner, &object.ctx_state)?;
        let packets = inner.encoder.drain().map_err(encode_error_status)?;
        inner.drained = true;
        enqueue_packets(inner, &object.ctx_state, packets, true)?;
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

/// 全格式像素视图校验（§2.11 单帧导出）：RGBA8/RGB8/BGR8 单平面、NV12 双平面
/// （偶数宽高）；返回 (平面切片集, 步长集, 宽, 高, 格式, matrix, range)。
///
/// # Safety
/// frame 须指向有效 am_pixel_view_t；平面数据在本次调用期间保持可读。
/// 全格式帧提取产物：平面切片集/步长集/宽/高/格式/矩阵/范围（§2.11）。
type AnyView<'a> = (Vec<&'a [u8]>, Vec<usize>, u32, u32, u32, u32, u32);
unsafe fn read_pixel_view_any(frame: *const am_pixel_view_t) -> Result<AnyView<'static>, i32> {
    am_sdk_support::args::pointer(frame)?;
    let value = unsafe { am_sdk_support::args::read(frame, size_of::<am_pixel_view_t>()) }?;
    if value.magic != AM_PIXEL_VIEW_MAGIC {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    am_sdk_support::args::pointer(value.view)?;
    let view = unsafe { &*value.view };
    let (w, h) = (view.width, view.height);
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let planes = match view.format as i32 {
        AM_PF_RGBA8 | AM_PF_RGB8 | AM_PF_BGR8 => vec![(
            view.planes[0],
            view.strides[0] as usize,
            h as usize,
            if view.format as i32 == AM_PF_RGBA8 {
                4
            } else {
                3
            },
        )],
        AM_PF_NV12 if w.is_multiple_of(2) && h.is_multiple_of(2) => vec![
            (view.planes[0], view.strides[0] as usize, h as usize, 1),
            (view.planes[1], view.strides[1] as usize, h as usize / 2, 2),
        ],
        _ => return Err(AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED),
    };
    let mut slices = Vec::with_capacity(planes.len());
    let mut strides = Vec::with_capacity(planes.len());
    for (ptr, stride, rows, bpp) in planes {
        if ptr.is_null() || stride < w as usize * bpp {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let needed = (stride
            .checked_mul(rows - 1)
            .and_then(|tail| tail.checked_add(w as usize * bpp)))
        .ok_or(AM_ERR_SDK_INVALID_ARGUMENT)? as u64;
        if view.plane_size_bytes[planes_index(&slices)] < needed {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        slices.push(unsafe { std::slice::from_raw_parts(ptr, needed as usize) });
        strides.push(stride);
    }
    Ok((
        slices,
        strides,
        w,
        h,
        view.format,
        view.color_matrix,
        view.color_range,
    ))
}

fn planes_index(slices: &[&[u8]]) -> usize {
    slices.len()
}

#[no_mangle]
pub unsafe extern "C" fn amc_frame_encode(
    ctx: *mut amc_context_t,
    frame: *const am_pixel_view_t,
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
        am_sdk_support::args::pointer(opts)?;
        let opts =
            unsafe { am_sdk_support::args::read(opts, size_of::<am_frame_encode_opts_t>()) }?;
        if opts.format != am_camera_core::frame_export::FORMAT_JPEG
            && opts.format != am_camera_core::frame_export::FORMAT_PNG
        {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        if opts.quality > 100 || opts.reserved != [0; 5] {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let (planes, strides, w, h, format, matrix, range) = unsafe { read_pixel_view_any(frame)? };
        let input = am_camera_core::frame_export::FrameInput {
            planes: &planes,
            strides: &strides,
            width: w,
            height: h,
            format,
            color_matrix: matrix,
            color_range: range,
        };
        let quality = if opts.quality == 0 {
            am_camera_core::frame_export::QUALITY_JPEG_DEFAULT
        } else {
            opts.quality
        };
        let bytes = am_camera_core::frame_export::encode(&input, opts.format, quality).map_err(
            |e| match e {
                am_camera_core::frame_export::ExportError::FormatInvalid
                | am_camera_core::frame_export::ExportError::PixelInvalid => {
                    AM_ERR_SDK_INVALID_ARGUMENT
                }
                am_camera_core::frame_export::ExportError::OutputTooLarge => {
                    AM_ERR_SDK_LIMIT_OUT_OF_RANGE
                }
                am_camera_core::frame_export::ExportError::EncodeFailed => {
                    AM_ERR_SDK_INTERNAL_ERROR
                }
            },
        )?;
        unsafe {
            out_size.write(bytes.len() as u64);
        }
        if (capacity as usize) < bytes.len() {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        am_sdk_support::args::pointer(out)?;
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests;
