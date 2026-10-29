//! H.264 软编码器（libx264 经 avcodec）。
use super::ffmpeg_lib::FfmpegApi;
use super::ffmpeg_sys::*;
use std::ffi::c_int;
use std::sync::Arc;

pub const DEFAULT_BITRATE_KBPS: u32 = 2500;
pub const DEFAULT_GOP_SIZE: u32 = 60;
pub const BITRATE_KBPS_RANGE: (u32, u32) = (100, 200_000);
pub const GOP_RANGE: (u32, u32) = (1, 10_000);
pub const AXIS_RANGE: (u32, u32) = (16, 8_192);
pub const FPS_RATIO_RANGE: (u32, u32) = (1, 240);

/// 编码配置（界与合同 am_encoder_config_t 一致）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncoderSettings {
    pub width: u32,
    pub height: u32,
    pub bitrate_kbps: u32,
    pub gop_size: u32,
    pub fps_num: u32,
    pub fps_den: u32,
}

impl EncoderSettings {
    /// 0 值展开为默认并校验界；越界返回静态原因。
    pub fn normalized(mut self) -> Result<Self, &'static str> {
        if self.bitrate_kbps == 0 {
            self.bitrate_kbps = DEFAULT_BITRATE_KBPS;
        }
        if self.gop_size == 0 {
            self.gop_size = DEFAULT_GOP_SIZE;
        }
        if self.width == 0 || self.height == 0 {
            return Err("camera.encoder.axisMissing");
        }
        if !(AXIS_RANGE.0..=AXIS_RANGE.1).contains(&self.width)
            || !(AXIS_RANGE.0..=AXIS_RANGE.1).contains(&self.height)
        {
            return Err("camera.encoder.axisRange");
        }
        if !(BITRATE_KBPS_RANGE.0..=BITRATE_KBPS_RANGE.1).contains(&self.bitrate_kbps) {
            return Err("camera.encoder.bitrateRange");
        }
        if !(GOP_RANGE.0..=GOP_RANGE.1).contains(&self.gop_size) {
            return Err("camera.encoder.gopRange");
        }
        let (n, d) = (self.fps_num, self.fps_den);
        if n == 0 || d == 0 || n > 1_000_000 || d > 1_000_000 || n < d || n > FPS_RATIO_RANGE.1 * d {
            return Err("camera.encoder.fpsRange");
        }
        Ok(self)
    }
}

/// 编码失败分类：FFmpeg 返回码原样携带，装载/配置给静态原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    Library(&'static str),
    Config(&'static str),
    Ffmpeg(c_int),
    PixelInvalid,
    InputAfterFlush,
}

/// 编码输出包：数据已拷贝出 FFmpeg 缓冲，关键帧标志随包。
pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub keyframe: bool,
    pub pts: i64,
    pub dts: i64,
}

pub struct Encoder {
    api: Arc<FfmpegApi>,
    codec: *const core::ffi::c_void,
    ctx: *mut AVCodecContext,
    frame: *mut AVFrame,
    sws: *mut SwsContext,
    settings: EncoderSettings,
    next_pts: i64,
    flushed: bool,
}
// SAFETY：FFmpeg 上下文指针只在 &mut self 方法内访问，跨线程经 Mutex 串行化。
unsafe impl Send for Encoder {}

impl Encoder {
    pub fn new(
        api: Arc<FfmpegApi>,
        settings: EncoderSettings,
        global_header: bool,
    ) -> Result<Self, EncodeError> {
        let settings = settings.normalized().map_err(EncodeError::Config)?;
        let codec = unsafe { (api.avcodec_find_encoder_by_name)(c"libx264".as_ptr()) };
        if codec.is_null() {
            return Err(EncodeError::Library("media.ffmpegEncoderMissing"));
        }
        // SAFETY：codec 指针来自运行库注册表，alloc 返回归本对象所有。
        let mut ctx = unsafe { (api.avcodec_alloc_context3)(codec) };
        if ctx.is_null() {
            return Err(EncodeError::Ffmpeg(12));
        }
        let mut options: *mut AVDictionary = std::ptr::null_mut();
        unsafe {
            (*ctx).bit_rate = i64::from(settings.bitrate_kbps) * 1000;
            if global_header {
                (*ctx).flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
            }
            (*ctx).pix_fmt = AV_PIX_FMT_YUV420P;
            (*ctx).width = settings.width as c_int;
            (*ctx).height = settings.height as c_int;
            (*ctx).time_base = AVRational {
                num: settings.fps_den as c_int,
                den: settings.fps_num as c_int,
            };
            (*ctx).framerate = AVRational {
                num: settings.fps_num as c_int,
                den: settings.fps_den as c_int,
            };
            (*ctx).gop_size = settings.gop_size as c_int;
            (*ctx).max_b_frames = 0;
            (api.av_dict_set)(&mut options, c"preset".as_ptr(), c"veryfast".as_ptr(), 0);
            // zerolatency 关掉 lookahead 缓冲：每帧确定性出包，队列预算语义可预测。
            (api.av_dict_set)(&mut options, c"tune".as_ptr(), c"zerolatency".as_ptr(), 0);
            let rc =
                (api.avcodec_open2)(ctx, codec, &mut options as *mut _ as *mut core::ffi::c_void);
            (api.av_dict_free)(&mut options);
            if rc < 0 {
                (api.avcodec_free_context)(&mut ctx);
                return Err(EncodeError::Ffmpeg(rc));
            }
        }
        // SAFETY：av_frame_alloc/sws_getContext 归本对象所有；失败路径全量释放。
        let (frame, sws) = unsafe {
            let mut frame = (api.av_frame_alloc)();
            if frame.is_null() {
                (api.avcodec_free_context)(&mut ctx);
                return Err(EncodeError::Ffmpeg(12));
            }
            (*frame).format = AV_PIX_FMT_YUV420P;
            (*frame).width = settings.width as c_int;
            (*frame).height = settings.height as c_int;
            if (api.av_frame_get_buffer)(frame, 32) < 0 {
                (api.av_frame_free)(&mut frame);
                (api.avcodec_free_context)(&mut ctx);
                return Err(EncodeError::Ffmpeg(12));
            }
            let sws = (api.sws_get_context)(
                settings.width as c_int,
                settings.height as c_int,
                AV_PIX_FMT_RGBA,
                settings.width as c_int,
                settings.height as c_int,
                AV_PIX_FMT_YUV420P,
                SWS_AREA,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            if sws.is_null() {
                (api.av_frame_free)(&mut frame);
                (api.avcodec_free_context)(&mut ctx);
                return Err(EncodeError::Ffmpeg(22));
            }
            (frame, sws)
        };
        Ok(Self {
            api,
            codec,
            ctx,
            frame,
            sws,
            settings,
            next_pts: 0,
            flushed: false,
        })
    }

    pub fn settings(&self) -> EncoderSettings {
        self.settings
    }

    /// 一帧 RGBA 进入编码器，返回本次产出的全部包。
    ///
    /// # Safety
    /// `rgba` 须指向 `stride*(height-1) + width*4` 字节可读内存。
    pub unsafe fn encode_rgba(
        &mut self,
        rgba: *const u8,
        stride: usize,
    ) -> Result<Vec<EncodedPacket>, EncodeError> {
        if self.flushed {
            return Err(EncodeError::InputAfterFlush);
        }
        let (w, h) = (self.settings.width as usize, self.settings.height as usize);
        if stride < w * 4 {
            return Err(EncodeError::PixelInvalid);
        }
        let needed = stride * (h - 1) + w * 4;
        if needed > isize::MAX as usize {
            return Err(EncodeError::PixelInvalid);
        }
        // SAFETY：调用方合同保证 rgba 至少 needed 字节可读；长度已核对不溢出。
        let view: &[u8] = unsafe { std::slice::from_raw_parts(rgba, needed) };
        let _ = view;
        let src: [*const u8; 2] = [rgba, std::ptr::null()];
        let src_strides = [stride as c_int, 0];
        unsafe {
            if (self.api.av_frame_make_writable)(self.frame) < 0 {
                return Err(EncodeError::Ffmpeg(22));
            }
            (self.api.sws_scale)(
                self.sws,
                src.as_ptr().cast(),
                src_strides.as_ptr(),
                0,
                h as c_int,
                (*self.frame).data.as_ptr(),
                (*self.frame).linesize.as_ptr(),
            );
            (*self.frame).pts = self.next_pts;
            self.next_pts += 1;
            let rc = (self.api.avcodec_send_frame)(self.ctx, self.frame);
            if rc < 0 {
                return Err(EncodeError::Ffmpeg(rc));
            }
        }
        self.receive_packets()
    }

    /// 冲刷编码器尾包；之后 encode_rgba 拒绝输入。
    pub fn drain(&mut self) -> Result<Vec<EncodedPacket>, EncodeError> {
        if self.flushed {
            return Ok(Vec::new());
        }
        unsafe {
            let rc = (self.api.avcodec_send_frame)(self.ctx, std::ptr::null());
            if rc < 0 {
                return Err(EncodeError::Ffmpeg(rc));
            }
        }
        let packets = self.receive_packets()?;
        self.flushed = true;
        Ok(packets)
    }

    fn receive_packets(&mut self) -> Result<Vec<EncodedPacket>, EncodeError> {
        let mut packets = Vec::new();
        loop {
            // SAFETY：packet 归本次循环所有；EAGAIN/EOF 路径立即释放。
            let mut packet = unsafe { (self.api.av_packet_alloc)() };
            if packet.is_null() {
                return Err(EncodeError::Ffmpeg(12));
            }
            let rc = unsafe { (self.api.avcodec_receive_packet)(self.ctx, packet) };
            if rc == AVERROR_EAGAIN {
                unsafe { (self.api.av_packet_free)(&mut packet) };
                return Ok(packets);
            }
            if rc == AVERROR_EOF {
                unsafe { (self.api.av_packet_free)(&mut packet) };
                self.flushed = true;
                return Ok(packets);
            }
            if rc < 0 {
                unsafe { (self.api.av_packet_free)(&mut packet) };
                return Err(EncodeError::Ffmpeg(rc));
            }
            let data = unsafe {
                let size = (*packet).size.max(0) as usize;
                let mut data = Vec::new();
                if data.try_reserve_exact(size).is_err() {
                    (self.api.av_packet_free)(&mut packet);
                    return Err(EncodeError::Ffmpeg(12));
                }
                data.extend_from_slice(std::slice::from_raw_parts((*packet).data, size));
                let keyframe = (*packet).flags & AV_PKT_FLAG_KEY != 0;
                (data, keyframe, (*packet).pts, (*packet).dts)
            };
            unsafe { (self.api.av_packet_unref)(packet) };
            unsafe { (self.api.av_packet_free)(&mut packet) };
            packets.push(EncodedPacket {
                data: data.0,
                keyframe: data.1,
                pts: data.2,
                dts: data.3,
            });
        }
    }

    /// 供 muxer 会话读编码器时间基，把包时间戳换算到流时间基。
    pub fn time_base(&self) -> AVRational {
        unsafe { (*self.ctx).time_base }
    }

    /// 供 muxer 直接取编码器上下文（参数拷贝到流后 write_header）。
    pub fn raw_ctx(&self) -> *mut AVCodecContext {
        self.ctx
    }

    /// 供 muxer 在 avformat_new_stream 传入编码器。
    pub fn raw_codec(&self) -> *const core::ffi::c_void {
        self.codec
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe {
            let (api, ctx, frame, sws) = (self.api.clone(), self.ctx, self.frame, self.sws);
            (api.avcodec_free_context)(&mut { ctx });
            (api.av_frame_free)(&mut { frame });
            (api.sws_free_context)(sws);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_bounds_and_defaults() {
        let make = |w, h, kbps, gop, n, d| EncoderSettings {
            width: w,
            height: h,
            bitrate_kbps: kbps,
            gop_size: gop,
            fps_num: n,
            fps_den: d,
        };
        let ok = make(16, 16, 0, 0, 30, 1).normalized().unwrap();
        assert_eq!(ok.bitrate_kbps, DEFAULT_BITRATE_KBPS);
        assert_eq!(ok.gop_size, DEFAULT_GOP_SIZE);
        assert!(make(15, 16, 2500, 60, 30, 1).normalized().is_err());
        assert!(make(16, 15, 2500, 60, 30, 1).normalized().is_err());
        assert!(make(8192, 8192, 2500, 60, 30, 1).normalized().is_ok());
        assert!(make(8193, 16, 2500, 60, 30, 1).normalized().is_err());
        assert!(make(16, 8193, 2500, 60, 30, 1).normalized().is_err());
        assert!(make(64, 64, 99, 60, 30, 1).normalized().is_err());
        assert!(make(64, 64, 200_001, 60, 30, 1).normalized().is_err());
        assert!(make(64, 64, 100, 0, 30, 1).normalized().is_ok());
        assert!(make(64, 64, 200_000, 0, 30, 1).normalized().is_ok());
        assert!(make(64, 64, 2500, 10_000, 30, 1).normalized().is_ok());
        assert!(make(64, 64, 2500, 10_001, 30, 1).normalized().is_err());
        assert!(make(64, 64, 2500, 60, 241, 1).normalized().is_err());
        assert!(make(64, 64, 2500, 60, 0, 1).normalized().is_err());
        assert!(make(64, 64, 2500, 60, 30, 0).normalized().is_err());
        assert!(make(64, 64, 2500, 60, 1000, 1000).normalized().is_ok());
        assert!(make(64, 64, 2500, 60, 1001, 1).normalized().is_err());
        assert!(make(64, 64, 2500, 60, 1000, 1001).normalized().is_err());
        assert!(make(64, 64, 2500, 60, 240, 1).normalized().is_ok());
    }
}
