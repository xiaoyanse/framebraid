//! 与网络源无关的同步解码原语；默认CPU，显式设备路径验证后才能报告硬解。
use super::{ffmpeg_lib::FfmpegApi, ffmpeg_sys::*};
use std::{ptr, sync::Arc};
mod convert;
mod gpu_frame;
mod hardware;
mod readback;
pub use gpu_frame::GpuFrameInfo;
pub mod selection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
}
impl VideoCodec {
    fn name(self) -> &'static std::ffi::CStr {
        match self {
            Self::H264 => c"h264",
            Self::H265 => c"hevc",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum DecodeError {
    InvalidInput,
    AllocationFailed,
    Runtime(i32),
    FrameTooLarge,
    Drained,
}
type Result<T> = std::result::Result<T, DecodeError>;
#[derive(Clone, Copy, Debug)]
pub struct FrameInfo {
    pub width: u32,
    pub height: u32,
    pub pts: i64,
    pub rgba_bytes: usize,
}
pub struct DecodedRgba {
    pub info: FrameInfo,
    pub data: Vec<u8>,
}
enum HardwareInput<'a> {
    New(super::hardware::Backend, &'a super::hardware::Adapter),
    Existing(super::hardware::Backend, *mut AVBufferRef),
}

pub struct CpuDecoder {
    api: Arc<FfmpegApi>,
    ctx: *mut AVCodecContext,
    packet: *mut AVPacket,
    frame: *mut AVFrame,
    pending: bool,
    ended: bool,
    max_pixels: u64,
    hardware: Option<Box<hardware::Binding>>,
    transfer: *mut AVFrame,
    converter: Option<convert::Converter>,
}
// 原语可在调用线程间移动；上层Mutex保证同一解码器不会并发访问FFmpeg上下文。
unsafe impl Send for CpuDecoder {}
impl CpuDecoder {
    pub fn create(
        api: Arc<FfmpegApi>,
        codec: VideoCodec,
        extradata: &[u8],
        max_pixels: u64,
    ) -> Result<Self> {
        Self::create_inner(api, codec, extradata, max_pixels, None)
    }

    /// 显式设备创建；初始化不代表真实硬解成功，须在收到帧后查询hardware_backend。
    pub fn create_on_device(
        api: Arc<FfmpegApi>,
        codec: VideoCodec,
        extradata: &[u8],
        max_pixels: u64,
        backend: super::hardware::Backend,
        adapter: &super::hardware::Adapter,
    ) -> Result<Self> {
        Self::create_inner(
            api,
            codec,
            extradata,
            max_pixels,
            Some(HardwareInput::New(backend, adapter)),
        )
    }

    /// 重建参数上下文并引用已有设备；不重新探测，不转移待输出帧或已验证状态。
    pub fn reconfigure(&self, codec: VideoCodec, extra: &[u8], max_pixels: u64) -> Result<Self> {
        let hardware = self.hardware.as_ref().map(|binding| {
            HardwareInput::Existing(binding.backend(), unsafe { (*self.ctx).hw_device_ctx })
        });
        Self::create_inner(self.api.clone(), codec, extra, max_pixels, hardware)
    }

    /// 上层决定回退时创建独立CPU上下文；输入重同步和预算仍由上层负责。
    pub fn software_reconfigure(
        &self,
        codec: VideoCodec,
        extra: &[u8],
        max_pixels: u64,
    ) -> Result<Self> {
        Self::create(self.api.clone(), codec, extra, max_pixels)
    }

    fn create_inner(
        api: Arc<FfmpegApi>,
        codec: VideoCodec,
        extradata: &[u8],
        max_pixels: u64,
        hardware: Option<HardwareInput<'_>>,
    ) -> Result<Self> {
        if max_pixels == 0 || max_pixels > 8192 * 8192 || extradata.len() > 16 * 1024 * 1024 {
            return Err(DecodeError::InvalidInput);
        }
        if unsafe { (api.avutil_version)() } >> 16 != 61 {
            return Err(DecodeError::InvalidInput);
        }
        let decoder = unsafe { (api.avcodec_find_decoder_by_name)(codec.name().as_ptr()) };
        if decoder.is_null() {
            return Err(DecodeError::InvalidInput);
        }
        let ctx = unsafe { (api.avcodec_alloc_context3)(decoder) };
        let mut result = Self {
            api,
            ctx,
            packet: ptr::null_mut(),
            frame: ptr::null_mut(),
            pending: false,
            ended: false,
            max_pixels,
            hardware: None,
            transfer: ptr::null_mut(),
            converter: None,
        };
        if ctx.is_null() {
            return Err(DecodeError::AllocationFailed);
        }
        unsafe {
            (*ctx).thread_count = 1;
            let rc = (result.api.av_opt_set_int)(ctx.cast(), c"thread_type".as_ptr(), 0, 0);
            if rc < 0 {
                return Err(DecodeError::Runtime(rc));
            }
            let rc = (result.api.av_opt_set_int)(
                ctx.cast(),
                c"max_pixels".as_ptr(),
                max_pixels as i64,
                0,
            );
            if rc < 0 {
                return Err(DecodeError::Runtime(rc));
            }
            if !extradata.is_empty() {
                // FFmpeg比特读取器允许读取64字节尾填充，分配与释放均由FFmpeg负责。
                let mut padded = extradata.to_vec();
                padded.resize(extradata.len() + 64, 0);
                (*ctx).extradata =
                    (result.api.av_memdup)(padded.as_ptr().cast(), padded.len()).cast();
                if (*ctx).extradata.is_null() {
                    return Err(DecodeError::AllocationFailed);
                }
                (*ctx).extradata_size = extradata.len() as i32;
            }
            if let Some(input) = hardware {
                result.hardware = Some(match input {
                    HardwareInput::New(backend, adapter) => hardware::Binding::attach(
                        result.api.clone(),
                        ctx,
                        decoder,
                        backend,
                        adapter,
                    )?,
                    HardwareInput::Existing(backend, device) => {
                        hardware::Binding::attach_reference(
                            &result.api,
                            ctx,
                            decoder,
                            backend,
                            device,
                        )?
                    }
                });
            }
            let rc = (result.api.avcodec_open2)(ctx, decoder, ptr::null_mut());
            if rc < 0 {
                return Err(DecodeError::Runtime(rc));
            }
            result.packet = (result.api.av_packet_alloc)();
            result.frame = (result.api.av_frame_alloc)();
        }
        if result.packet.is_null() || result.frame.is_null() {
            return Err(DecodeError::AllocationFailed);
        }
        Ok(result)
    }

    /// 只在实际收到硬件像素格式后返回Some，不能将设备创建成功冒充硬解运行中。
    pub fn hardware_backend(&self) -> Option<super::hardware::Backend> {
        self.hardware.as_ref().and_then(|h| h.verified_backend())
    }
    /// 仅作恢复预算提示，不将参数解析成功当真实出帧；非法尺寸不提供提示。
    pub fn dimensions_hint(&self) -> Option<(u32, u32)> {
        let (width, height) = unsafe { ((*self.ctx).width, (*self.ctx).height) };
        if width <= 0 || height <= 0 || width > 8192 || height > 8192 {
            return None;
        }
        let pixels = width as u64 * height as u64;
        (pixels <= self.max_pixels).then_some((width as u32, height as u32))
    }
    /// false表示FFmpeg暂未接受，可在receive后重试同一包；true后不可重发。
    pub fn send(&mut self, bytes: &[u8], pts: i64, dts: i64) -> Result<bool> {
        if bytes.is_empty() || bytes.len() > 16 * 1024 * 1024 || self.ended {
            return Err(DecodeError::InvalidInput);
        }
        let mut padded = Vec::with_capacity(bytes.len() + 64);
        padded.extend_from_slice(bytes);
        padded.resize(bytes.len() + 64, 0);
        self.send_padded(&padded, bytes.len(), pts, dts)
    }
    /// 已有有界队列可复用带尾填充的包，避免再做一份SDK中间拷贝。
    pub fn send_padded(&mut self, padded: &[u8], valid: usize, pts: i64, dts: i64) -> Result<bool> {
        if valid == 0
            || valid > 16 * 1024 * 1024
            || padded.len() < valid + 64
            || self.ended
            || padded[valid..valid + 64].iter().any(|b| *b != 0)
        {
            return Err(DecodeError::InvalidInput);
        }
        let rc = unsafe {
            (self.api.av_packet_unref)(self.packet);
            (*self.packet).data = padded.as_ptr() as *mut u8;
            (*self.packet).size = valid as i32;
            (*self.packet).pts = pts;
            (*self.packet).dts = dts;
            let rc = (self.api.avcodec_send_packet)(self.ctx, self.packet);
            // buf=NULL，FFmpeg接受后自持数据；清除借用地址再归还包对象。
            (*self.packet).data = ptr::null_mut();
            (*self.packet).size = 0;
            (self.api.av_packet_unref)(self.packet);
            rc
        };
        if rc == AVERROR_EAGAIN {
            return Ok(false);
        }
        if rc < 0 {
            return Err(DecodeError::Runtime(rc));
        }
        Ok(true)
    }
    pub fn drain(&mut self) -> Result<bool> {
        let rc = unsafe { (self.api.avcodec_send_packet)(self.ctx, ptr::null()) };
        if rc == AVERROR_EAGAIN {
            return Ok(false);
        }
        if rc < 0 && rc != AVERROR_EOF {
            return Err(DecodeError::Runtime(rc));
        }
        Ok(true)
    }
    /// peek保持FFmpeg帧，预算不足时可以稍后重试copy_rgba而不丢该帧。
    pub fn peek(&mut self) -> Result<Option<FrameInfo>> {
        if self.ended {
            return Err(DecodeError::Drained);
        }
        if !self.pending {
            let rc = unsafe { (self.api.avcodec_receive_frame)(self.ctx, self.frame) };
            if rc == AVERROR_EAGAIN {
                return Ok(None);
            }
            if rc == AVERROR_EOF {
                self.ended = true;
                return Err(DecodeError::Drained);
            }
            if rc < 0 {
                return Err(DecodeError::Runtime(rc));
            }
            self.pending = true;
        }
        let frame = unsafe { &*self.frame };
        if let Some(hardware) = &self.hardware {
            hardware.observe(frame.format);
        }
        let w = u32::try_from(frame.width).map_err(|_| DecodeError::FrameTooLarge)?;
        let h = u32::try_from(frame.height).map_err(|_| DecodeError::FrameTooLarge)?;
        let pixels = u64::from(w) * u64::from(h);
        if w == 0 || h == 0 || w > 8192 || h > 8192 || pixels > self.max_pixels {
            return Err(DecodeError::FrameTooLarge);
        }
        Ok(Some(FrameInfo {
            width: w,
            height: h,
            pts: frame.pts,
            rgba_bytes: (pixels * 4) as usize,
        }))
    }
    /// 不分配输出即可释放当前帧，继续维护内部解码参考。
    pub fn discard_frame(&mut self) -> Result<()> {
        if self.peek()?.is_some() {
            self.pending = false;
            unsafe {
                (self.api.av_frame_unref)(self.frame);
            }
        }
        self.release_readback_cache();
        Ok(())
    }
    pub fn copy_rgba(&mut self, max_bytes: usize) -> Result<Option<DecodedRgba>> {
        self.copy_pixels(max_bytes, 0)
    }
    /// 直接写目标平面，避免转换格式时再经RGBA中转。
    pub fn copy_pixels(&mut self, max_bytes: usize, format: u32) -> Result<Option<DecodedRgba>> {
        self.copy_pixels_reusing_readback(max_bytes, format, usize::MAX)
    }
    /// 待输出硬件帧所需CPU暂存；上层先取得字节许可，再调用有预算的copy入口。
    pub fn hardware_readback_bytes(&mut self) -> Result<usize> {
        let Some(info) = self.peek()? else {
            return Ok(0);
        };
        Ok(self.readback_plan(info)?.map_or(0, |plan| plan.bytes))
    }
    pub fn copy_pixels_with_readback_budget(
        &mut self,
        max_bytes: usize,
        format: u32,
        max_readback_bytes: usize,
    ) -> Result<Option<DecodedRgba>> {
        let result = self.copy_pixels_inner(max_bytes, format, max_readback_bytes);
        // 原硬件帧出错时仍保留；CPU目标帧在所有退出路径归还，不越过Charge寿命。
        if !self.transfer.is_null() {
            unsafe { (self.api.av_frame_unref)(self.transfer) };
        }
        result
    }
    /// 复用最多一张CPU回读帧；上层许可须保留到release_readback_cache或解码器销毁。
    pub fn copy_pixels_reusing_readback(
        &mut self,
        max_bytes: usize,
        format: u32,
        max_readback_bytes: usize,
    ) -> Result<Option<DecodedRgba>> {
        let result = self.copy_pixels_inner(max_bytes, format, max_readback_bytes);
        if result.is_err() {
            self.release_readback_cache();
        }
        result
    }
    pub fn release_readback_cache(&mut self) {
        if !self.transfer.is_null() {
            unsafe { (self.api.av_frame_unref)(self.transfer) };
        }
    }
    fn copy_pixels_inner(
        &mut self,
        max_bytes: usize,
        format: u32,
        max_readback_bytes: usize,
    ) -> Result<Option<DecodedRgba>> {
        let Some(info) = self.peek()? else {
            return Ok(None);
        };
        let (bytes, stride, pixel_format) = match format {
            0 => (info.rgba_bytes, info.width * 4, AV_PIX_FMT_RGBA),
            1 => (
                info.width as usize * info.height as usize * 3,
                info.width * 3,
                AV_PIX_FMT_RGB24,
            ),
            2 => (
                info.width as usize * info.height as usize * 3,
                info.width * 3,
                AV_PIX_FMT_BGR24,
            ),
            _ => return Err(DecodeError::InvalidInput),
        };
        if bytes > max_bytes {
            return Err(DecodeError::FrameTooLarge);
        }
        let source = self.pixel_source(info, max_readback_bytes)?;
        let frame = unsafe { &*source };
        let mut data = vec![0u8; bytes];
        let key = convert::Key {
            width: frame.width,
            height: frame.height,
            source: frame.format,
            target: pixel_format,
        };
        if self
            .converter
            .as_ref()
            .is_none_or(|converter| converter.key != key)
        {
            self.converter = Some(convert::Converter::create(self.api.clone(), key)?);
        }
        let sws = self
            .converter
            .as_ref()
            .ok_or(DecodeError::AllocationFailed)?
            .pointer;
        let mut planes = [ptr::null_mut(); 4];
        planes[0] = data.as_mut_ptr();
        let strides = [stride as i32, 0, 0, 0];
        let rows = unsafe {
            (self.api.sws_scale)(
                sws,
                frame.data.as_ptr().cast(),
                frame.linesize.as_ptr(),
                0,
                frame.height,
                planes.as_ptr(),
                strides.as_ptr(),
            )
        };
        if rows != frame.height {
            self.converter.take();
            return Err(DecodeError::Runtime(rows));
        }
        self.pending = false;
        unsafe {
            (self.api.av_frame_unref)(self.frame);
        }
        Ok(Some(DecodedRgba { info, data }))
    }

    fn readback_plan(&self, info: FrameInfo) -> Result<Option<readback::Plan>> {
        if !self
            .hardware
            .as_ref()
            .is_some_and(|h| h.matches(unsafe { (*self.frame).format }))
        {
            return Ok(None);
        }
        unsafe { readback::Plan::query(&self.api, (*self.ctx).hw_frames_ctx, info).map(Some) }
    }
    fn pixel_source(&mut self, info: FrameInfo, max_readback_bytes: usize) -> Result<*mut AVFrame> {
        let Some(plan) = self.readback_plan(info)? else {
            return Ok(self.frame);
        };
        if plan.bytes > max_readback_bytes {
            return Err(DecodeError::FrameTooLarge);
        }
        if self.transfer.is_null() {
            self.transfer = unsafe { (self.api.av_frame_alloc)() };
            if self.transfer.is_null() {
                return Err(DecodeError::AllocationFailed);
            }
        }
        let reusable = unsafe {
            (*self.transfer).format == plan.format
                && (*self.transfer).width == plan.width
                && (*self.transfer).height == plan.height
                && !(self.api.av_frame_get_plane_buffer)(self.transfer, 0).is_null()
        };
        if !reusable {
            unsafe { (self.api.av_frame_unref)(self.transfer) };
            unsafe { plan.allocate(&self.api, self.transfer) }?;
        }
        let rc = unsafe { (self.api.av_hwframe_transfer_data)(self.transfer, self.frame, 0) };
        if rc < 0 {
            return Err(DecodeError::Runtime(rc));
        }
        Ok(self.transfer)
    }
}
impl Drop for CpuDecoder {
    fn drop(&mut self) {
        unsafe {
            if !self.packet.is_null() {
                (self.api.av_packet_free)(&mut self.packet);
            }
            if !self.frame.is_null() {
                (self.api.av_frame_free)(&mut self.frame);
            }
            if !self.transfer.is_null() {
                (self.api.av_frame_free)(&mut self.transfer);
            }
            if !self.ctx.is_null() {
                (self.api.avcodec_free_context)(&mut self.ctx);
            }
        }
    }
}

mod nal;
pub use nal::{access_units, visit_access_units};

#[cfg(test)]
mod tests;
