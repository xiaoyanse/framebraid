//! 与网络源无关的同步解码原语；默认CPU，显式设备路径验证后才能报告硬解。
use super::{ffmpeg_lib::FfmpegApi, ffmpeg_sys::*};
use std::{ptr, sync::Arc};
mod hardware;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        Self::create_inner(api, codec, extradata, max_pixels, Some((backend, adapter)))
    }

    fn create_inner(
        api: Arc<FfmpegApi>,
        codec: VideoCodec,
        extradata: &[u8],
        max_pixels: u64,
        hardware: Option<(super::hardware::Backend, &super::hardware::Adapter)>,
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
            if let Some((backend, adapter)) = hardware {
                result.hardware = Some(hardware::Binding::attach(
                    result.api.clone(),
                    ctx,
                    decoder,
                    backend,
                    adapter,
                )?);
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
                if !self.transfer.is_null() {
                    (self.api.av_frame_unref)(self.transfer);
                }
            }
        }
        Ok(())
    }
    pub fn copy_rgba(&mut self, max_bytes: usize) -> Result<Option<DecodedRgba>> {
        self.copy_pixels(max_bytes, 0)
    }
    /// 直接写目标平面，避免转换格式时再经RGBA中转。
    pub fn copy_pixels(&mut self, max_bytes: usize, format: u32) -> Result<Option<DecodedRgba>> {
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
        let source = self.pixel_source()?;
        let frame = unsafe { &*source };
        let mut data = vec![0u8; bytes];
        let sws = unsafe {
            (self.api.sws_get_context)(
                frame.width,
                frame.height,
                frame.format,
                frame.width,
                frame.height,
                pixel_format,
                2,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if sws.is_null() {
            return Err(DecodeError::AllocationFailed);
        }
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
        unsafe {
            (self.api.sws_free_context)(sws);
        }
        if rows != frame.height {
            return Err(DecodeError::Runtime(rows));
        }
        self.pending = false;
        unsafe {
            (self.api.av_frame_unref)(self.frame);
            if !self.transfer.is_null() {
                (self.api.av_frame_unref)(self.transfer);
            }
        }
        Ok(Some(DecodedRgba { info, data }))
    }

    fn pixel_source(&mut self) -> Result<*mut AVFrame> {
        if !self
            .hardware
            .as_ref()
            .is_some_and(|h| h.matches(unsafe { (*self.frame).format }))
        {
            return Ok(self.frame);
        }
        if self.transfer.is_null() {
            self.transfer = unsafe { (self.api.av_frame_alloc)() };
            if self.transfer.is_null() {
                return Err(DecodeError::AllocationFailed);
            }
        }
        unsafe { (self.api.av_frame_unref)(self.transfer) };
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

/// 对完整Annex B包做边界检查，不把keyframe标志当真实随机访问点。
pub fn access_units(codec: VideoCodec, data: &[u8]) -> Result<Vec<&[u8]>> {
    if data.is_empty() || data.len() > 16 * 1024 * 1024 {
        return Err(DecodeError::InvalidInput);
    }
    let start = |at: usize| -> usize {
        if data.get(at..at + 4) == Some(&[0, 0, 0, 1]) {
            4
        } else if data.get(at..at + 3) == Some(&[0, 0, 1]) {
            3
        } else {
            0
        }
    };
    let first = start(0);
    if first == 0 {
        return Err(DecodeError::InvalidInput);
    }
    let mut units = Vec::new();
    let mut begin = first;
    let mut at = begin;
    while at < data.len() {
        let n = start(at);
        if n != 0 {
            if at == begin {
                return Err(DecodeError::InvalidInput);
            }
            units.push(&data[begin..at]);
            if units.len() > 4096 {
                return Err(DecodeError::InvalidInput);
            }
            begin = at + n;
            at = begin;
        } else {
            at += 1;
        }
    }
    if begin == data.len() {
        return Err(DecodeError::InvalidInput);
    }
    units.push(&data[begin..]);
    for nal in &units {
        match codec {
            VideoCodec::H264 => {
                if nal[0] & 128 != 0 || nal[0] & 31 == 0 || nal[0] & 31 > 23 {
                    return Err(DecodeError::InvalidInput);
                }
            }
            VideoCodec::H265 => {
                if nal.len() < 2 || nal[0] & 128 != 0 || nal[1] & 7 == 0 {
                    return Err(DecodeError::InvalidInput);
                }
            }
        }
    }
    Ok(units)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_split_private_and_invalid_nal_headers() {
        assert!(access_units(VideoCodec::H264, &[0, 0, 1, 0x65, 3]).is_ok());
        assert!(access_units(VideoCodec::H264, &[0, 0, 1, 0xe5, 3]).is_err());
        assert!(access_units(VideoCodec::H264, &[0x65, 3]).is_err());
        assert!(access_units(VideoCodec::H265, &[0, 0, 1, 0x26, 1, 3]).is_ok());
        assert!(access_units(VideoCodec::H265, &[0, 0, 1, 0x26, 0, 3]).is_err());
        assert!(access_units(VideoCodec::H265, &[0, 0, 1, 0x26]).is_err());
        assert!(access_units(VideoCodec::H264, &[0, 0, 1, 0, 0, 1]).is_err());
    }
    #[test]
    #[ignore = "显式设置AM_SDK_FFMPEG_DIR和AM_SDK_CODEC_SAMPLES，使用隔离运行库及合成视频"]
    fn real_h264_and_h265_decode_keep_pts_and_pending_frame() {
        let runtime = std::env::var_os("AM_SDK_FFMPEG_DIR").expect("缺少隔离运行库目录");
        let samples = std::env::var_os("AM_SDK_CODEC_SAMPLES").expect("缺少合成样本目录");
        let api = Arc::new(FfmpegApi::load_from(std::path::Path::new(&runtime)).unwrap());
        for (codec, file) in [
            (VideoCodec::H264, "one.h264"),
            (VideoCodec::H265, "one.hevc"),
        ] {
            let data = std::fs::read(std::path::Path::new(&samples).join(file)).unwrap();
            assert!(access_units(codec, &data).is_ok());
            let mut decoder = CpuDecoder::create(api.clone(), codec, &[], 256 * 144).unwrap();
            assert!(decoder.send(&data, 90000, 90000).unwrap());
            // H.265允许延迟输出，排空后仍应能取得该帧。
            if decoder.peek().unwrap().is_none() {
                assert!(decoder.drain().unwrap());
            }
            let info = decoder.peek().unwrap().unwrap();
            assert_eq!((info.width, info.height, info.pts), (256, 144, 90000));
            assert!(matches!(
                decoder.copy_rgba(1),
                Err(DecodeError::FrameTooLarge)
            ));
            assert_eq!(decoder.peek().unwrap().unwrap().pts, 90000);
            let frame = decoder.copy_rgba(info.rgba_bytes).unwrap().unwrap();
            for pixel in frame.data.chunks_exact(4) {
                assert!(pixel[0] >= 250 && pixel[1] <= 3 && pixel[2] <= 3 && pixel[3] == 255);
            }
            decoder.drain().unwrap();
            assert!(matches!(decoder.peek(), Err(DecodeError::Drained)));
        }
    }
}
