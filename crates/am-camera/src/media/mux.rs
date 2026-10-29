//! avformat 输出封装：MP4 存档与 FLV/RTMP 推流共用同一 muxer 封装。
use super::encode::{EncodeError, Encoder, EncoderSettings};
use super::ffmpeg_lib::FfmpegApi;
use super::ffmpeg_sys::*;
use std::ffi::{c_int, CStr};
use std::sync::{Arc, OnceLock};

/// 输出格式：MP4 存档或 FLV 推流（RTMP 承载）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MuxFormat {
    Mp4,
    Flv,
}

impl MuxFormat {
    fn name(self) -> &'static CStr {
        match self {
            Self::Mp4 => c"mp4",
            Self::Flv => c"flv",
        }
    }
}

/// 封装失败分类：Io 表示底层连接可重建（推流重连语义），其余为终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuxError {
    Encode(EncodeError),
    Config(&'static str),
    Ffmpeg(c_int),
    Io(c_int),
}

impl From<EncodeError> for MuxError {
    fn from(value: EncodeError) -> Self {
        Self::Encode(value)
    }
}

/// 幂等网络初始化（RTMP 需要）。
fn network_init(api: &FfmpegApi) {
    static NETWORK: OnceLock<()> = OnceLock::new();
    NETWORK.get_or_init(|| {
        // SAFETY：幂等初始化。
        unsafe { (api.avformat_network_init)() };
    });
}

/// 重连退避：1s×2^n 封顶 30s。
pub fn backoff_delay_ms(attempt: u32) -> u64 {
    (1000u64 << attempt.min(5)).min(30_000)
}

/// 推流最大重连次数（超过后会话报错，合同语义）。
pub const STREAM_MAX_RETRIES: u32 = 5;

pub struct Muxer {
    api: Arc<FfmpegApi>,
    fmt: *mut AVFormatContext,
    stream: *mut AVStream,
    enc: Encoder,
    stream_tb: AVRational,
    connected: bool,
    finished: bool,
}

// SAFETY：FFmpeg 上下文只在 &mut self 方法内访问。
unsafe impl Send for Muxer {}

impl Muxer {
    /// 阶段一（无 IO）：输出上下文 + 视频流 + 编码器参数拷贝。
    pub fn create(
        api: Arc<FfmpegApi>,
        settings: EncoderSettings,
        format: MuxFormat,
    ) -> Result<Self, MuxError> {
        let settings = settings.normalized().map_err(MuxError::Config)?;
        network_init(&api);
        let mut fmt: *mut AVFormatContext = std::ptr::null_mut();
        // SAFETY：fmt 归本对象所有；格式名与目标仅作输出格式解析。
        let rc = unsafe {
            (api.avformat_alloc_output_context2)(
                &mut fmt,
                std::ptr::null(),
                format.name().as_ptr(),
                std::ptr::null(),
            )
        };
        if rc < 0 || fmt.is_null() {
            return Err(MuxError::Ffmpeg(rc));
        }
        let global_header =
            unsafe { (*(*fmt).oformat.cast::<AVOutputFormat>()).flags & AVFMT_GLOBALHEADER != 0 };
        let enc = Encoder::new(api.clone(), settings, global_header).map_err(|error| unsafe {
            (api.avformat_free_context)(fmt);
            MuxError::Encode(error)
        })?;
        // SAFETY：流归 fmt 所有；codec 指针来自运行库注册表。
        let stream = unsafe { (api.avformat_new_stream)(fmt, enc.raw_codec()) };
        if stream.is_null() {
            unsafe { (api.avformat_free_context)(fmt) };
            return Err(MuxError::Ffmpeg(12));
        }
        unsafe {
            let rc = (api.avcodec_parameters_from_context)((*stream).codecpar, enc.raw_ctx());
            if rc < 0 {
                (api.avformat_free_context)(fmt);
                return Err(MuxError::Ffmpeg(rc));
            }
        }
        Ok(Self {
            api,
            fmt,
            stream,
            enc,
            stream_tb: AVRational { num: 0, den: 0 },
            connected: false,
            finished: false,
        })
    }

    /// 阶段二：打开文件并写头。
    pub fn connect_file(&mut self, path: &CStr) -> Result<(), MuxError> {
        self.connect(path)
    }

    /// 阶段二：连接网络端点并写头（RTMP 握手发生在此步）。
    pub fn connect_stream(&mut self, url: &CStr) -> Result<(), MuxError> {
        self.connect(url)
    }

    fn connect(&mut self, target: &CStr) -> Result<(), MuxError> {
        if self.connected {
            return Err(MuxError::Config("camera.muxAlreadyConnected"));
        }
        unsafe {
            let rc =
                (self.api.avio_open)(&raw mut (*self.fmt).pb, target.as_ptr(), AVIO_FLAG_WRITE);
            if rc < 0 {
                return Err(MuxError::Io(rc));
            }
            let rc = (self.api.avformat_write_header)(self.fmt, std::ptr::null_mut());
            if rc < 0 {
                (self.api.avio_closep)(&raw mut (*self.fmt).pb);
                return Err(if self.is_network() {
                    MuxError::Io(rc)
                } else {
                    MuxError::Ffmpeg(rc)
                });
            }
            self.stream_tb = (*self.stream).time_base;
            self.connected = true;
        }
        Ok(())
    }

    fn is_network(&self) -> bool {
        let name = unsafe { (*(*self.fmt).oformat.cast::<AVOutputFormat>()).name };
        unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes() == b"flv"
    }

    /// 编码一帧并写入封装器。
    ///
    /// # Safety
    /// `rgba` 须指向 `stride*(height-1) + width*4` 字节可读内存。
    pub unsafe fn write(&mut self, rgba: *const u8, stride: usize) -> Result<(), MuxError> {
        if !self.connected || self.finished {
            return Err(MuxError::Config("camera.muxNotConnected"));
        }
        let packets = unsafe { self.enc.encode_rgba(rgba, stride) }?;
        let enc_tb = self.enc.time_base();
        for packet in packets {
            if packet.data.is_empty() {
                continue;
            }
            // SAFETY：packet 归本次循环所有；data 借用 Vec，write_frame 内部按需拷贝。
            let mut pkt = unsafe { (self.api.av_packet_alloc)() };
            if pkt.is_null() {
                return Err(MuxError::Ffmpeg(12));
            }
            unsafe {
                (*pkt).data = packet.data.as_ptr() as *mut u8;
                (*pkt).size = packet.data.len() as c_int;
                (*pkt).pts = packet.pts;
                (*pkt).dts = packet.dts;
                (*pkt).duration = 1;
                if packet.keyframe {
                    (*pkt).flags |= AV_PKT_FLAG_KEY;
                }
                (*pkt).stream_index = (*self.stream).index;
                (self.api.av_packet_rescale_ts)(pkt, enc_tb, self.stream_tb);
                let rc = (self.api.av_interleaved_write_frame)(self.fmt, pkt);
                (self.api.av_packet_free)(&mut pkt);
                if rc < 0 {
                    return Err(if self.is_network() {
                        MuxError::Io(rc)
                    } else {
                        MuxError::Ffmpeg(rc)
                    });
                }
            }
        }
        Ok(())
    }

    /// 写 trailer 并关闭目标（存档文件此后可播放）。
    pub fn finish(&mut self) -> Result<(), MuxError> {
        if !self.connected || self.finished {
            return Err(MuxError::Config("camera.muxNotConnected"));
        }
        self.finished = true;
        unsafe {
            let rc = (self.api.av_write_trailer)(self.fmt);
            let closed = (self.api.avio_closep)(&raw mut (*self.fmt).pb);
            if rc < 0 {
                return Err(MuxError::Ffmpeg(rc));
            }
            if closed < 0 {
                return Err(MuxError::Io(closed));
            }
        }
        Ok(())
    }

    /// 关闭但不写 trailer（断线丢弃/推流会话终止）。
    pub fn abort(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        unsafe {
            if !(*self.fmt).pb.is_null() {
                (self.api.avio_closep)(&raw mut (*self.fmt).pb);
            }
        }
    }
}

impl Drop for Muxer {
    fn drop(&mut self) {
        if !self.finished {
            self.abort();
        }
        unsafe { (self.api.avformat_free_context)(self.fmt) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_caps_at_thirty_seconds() {
        assert_eq!(backoff_delay_ms(0), 1_000);
        assert_eq!(backoff_delay_ms(1), 2_000);
        assert_eq!(backoff_delay_ms(2), 4_000);
        assert_eq!(backoff_delay_ms(3), 8_000);
        assert_eq!(backoff_delay_ms(4), 16_000);
        assert_eq!(backoff_delay_ms(5), 30_000);
        assert_eq!(backoff_delay_ms(6), 30_000);
        assert_eq!(backoff_delay_ms(u32::MAX), 30_000);
    }

    fn stream_settings() -> EncoderSettings {
        EncoderSettings {
            width: 320,
            height: 240,
            bitrate_kbps: 1500,
            gop_size: 10,
            fps_num: 30,
            fps_den: 1,
        }
    }

    #[test]
    #[ignore = "需要显式FFmpeg运行库（AM_RT_FFMPEG_DIR）；CI无运行库环境跳过"]
    fn muxer_context_creation_for_mp4_and_rtmp_flv() {
        let dir = std::env::var("AM_RT_FFMPEG_DIR").expect("AM_RT_FFMPEG_DIR 未设置");
        let api = Arc::new(
            crate::media::ffmpeg_lib::FfmpegApi::load_from(std::path::Path::new(&dir))
                .expect("运行库装载失败"),
        );
        for format in [MuxFormat::Mp4, MuxFormat::Flv] {
            let mut muxer = Muxer::create(api.clone(), stream_settings(), format)
                .expect("muxer 上下文创建（无 IO 阶段）");
            assert_eq!(unsafe { (*muxer.stream).index }, 0);
            let par = unsafe { (*muxer.stream).codecpar };
            assert_eq!(unsafe { (*par).width }, 320);
            assert_eq!(unsafe { (*par).height }, 240);
            assert_eq!(unsafe { (*par).format }, AV_PIX_FMT_YUV420P);
            assert_ne!(unsafe { (*par).codec_id }, 0, "codec_id 应为 H264 非 NONE");
            assert!(
                unsafe { (*par).extradata_size } > 0,
                "GLOBAL_HEADER 格式下参数集应进 extradata"
            );
            let error = unsafe { muxer.write(std::ptr::null(), 0) }.unwrap_err();
            assert_eq!(error, MuxError::Config("camera.muxNotConnected"));
            assert_eq!(
                muxer.finish().unwrap_err(),
                MuxError::Config("camera.muxNotConnected")
            );
        }
    }
}
