use super::{
    nal::{Framing, MAX_PACKET},
    validate_url, IoError,
};
use crate::{
    credentials::Secret,
    media::{ffmpeg_lib::FfmpegApi, ffmpeg_sys::*},
};
use std::{
    cell::Cell,
    ffi::CString,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub struct StreamInfo {
    pub codec: u32,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub time_base_num: u32,
    pub time_base_den: u32,
    /// 全部编码统一为 Annex B 参数集；MJPEG 为空。
    pub extradata: Vec<u8>,
}
pub struct Packet {
    pub data: Vec<u8>,
    pub pts: i64,
    pub dts: i64,
    pub keyframe: bool,
}
struct Interrupt {
    stop: Arc<AtomicBool>,
    deadline: Cell<Instant>,
}
unsafe extern "C" fn interrupt(ptr: *mut core::ffi::c_void) -> core::ffi::c_int {
    // SAFETY：Box 地址固定，FFmpeg 仅在 Reader 当前同步调用中访问。
    let state = unsafe { &*ptr.cast::<Interrupt>() };
    i32::from(state.stop.load(Ordering::Acquire) || Instant::now() >= state.deadline.get())
}
// FFmpeg 63 AVStream：公开前缀的 codecpar 后依次为 priv_data/time_base。
#[repr(C)]
struct StreamPrefix {
    av_class: *const core::ffi::c_void,
    index: i32,
    id: i32,
    codecpar: *mut AVCodecParameters,
    priv_data: *mut core::ffi::c_void,
    time_base: AVRational,
}
pub struct Reader {
    api: Arc<FfmpegApi>,
    fmt: *mut AVFormatContext,
    packet: *mut AVPacket,
    interrupt: Box<Interrupt>,
    timeout: Duration,
    stream: i32,
    info: StreamInfo,
    framing: Framing,
    _quiet: super::quiet::Guard,
}
// SAFETY：Reader 只在唯一所有者线程操作 FFmpeg；stop 为独立原子信号。
unsafe impl Send for Reader {}
impl Drop for Reader {
    fn drop(&mut self) {
        // SAFETY：指针来自本实例 api，关闭期间保留 interrupt 与运行库。
        unsafe {
            if !self.packet.is_null() {
                (self.api.av_packet_free)(&mut self.packet);
            }
            if !self.fmt.is_null() {
                (self.api.avformat_close_input)(&mut self.fmt);
            }
        }
    }
}
impl Reader {
    pub fn open(
        api: Arc<FfmpegApi>,
        url: &str,
        secret: &Secret,
        stop: Arc<AtomicBool>,
        timeout: Duration,
    ) -> Result<Self, IoError> {
        validate_url(url, false)?;
        if timeout.is_zero()
            || secret.username.len() + secret.password.len() > 8192
            || secret
                .username
                .chars()
                .chain(secret.password.chars())
                .any(char::is_control)
        {
            return Err(IoError::Invalid);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(IoError::Invalid)?;
        let mut parsed = reqwest::Url::parse(url).map_err(|_| IoError::Invalid)?;
        parsed
            .set_username(&secret.username)
            .map_err(|_| IoError::Invalid)?;
        parsed
            .set_password(Some(&secret.password))
            .map_err(|_| IoError::Invalid)?;
        let address = CString::new(parsed.as_str()).map_err(|_| IoError::Invalid)?;
        let quiet = super::quiet::Guard::new(api.clone())?;
        let mut reader = Self {
            api,
            fmt: std::ptr::null_mut(),
            packet: std::ptr::null_mut(),
            interrupt: Box::new(Interrupt {
                stop,
                deadline: Cell::new(deadline),
            }),
            timeout,
            stream: -1,
            info: StreamInfo {
                codec: 0,
                width: 0,
                height: 0,
                fps: -1.0,
                time_base_num: 0,
                time_base_den: 0,
                extradata: Vec::new(),
            },
            framing: Framing::new(1, &[])?,
            _quiet: quiet,
        };
        reader.check()?;
        // SAFETY：原生指针全部由本实例运行库创建，失败也由 Drop 回收。
        unsafe {
            if (reader.api.avformat_network_init)() < 0 {
                return Err(IoError::Network);
            }
            reader.fmt = (reader.api.avformat_alloc_context)();
            if reader.fmt.is_null() {
                return Err(IoError::Resource);
            }
            (*reader.fmt).interrupt_callback = AVIOInterruptCB {
                callback: Some(interrupt),
                opaque: (&mut *reader.interrupt as *mut Interrupt).cast(),
            };
            let mut options = std::ptr::null_mut();
            for (key, value) in [
                (c"rtsp_transport", c"tcp"),
                (c"fflags", c"nobuffer"),
                (c"probesize", c"32768"),
                (c"analyzeduration", c"500000"),
                (c"max_delay", c"0"),
                (c"reorder_queue_size", c"0"),
            ] {
                if (reader.api.av_dict_set)(&mut options, key.as_ptr(), value.as_ptr(), 0) < 0 {
                    (reader.api.av_dict_free)(&mut options);
                    return Err(IoError::Resource);
                }
            }
            let rc = (reader.api.avformat_open_input)(
                &mut reader.fmt,
                address.as_ptr(),
                std::ptr::null(),
                (&mut options as *mut *mut AVDictionary).cast(),
            );
            (reader.api.av_dict_free)(&mut options);
            reader.result(rc)?;
            reader.result((reader.api.avformat_find_stream_info)(
                reader.fmt,
                std::ptr::null_mut(),
            ))?;
            let index = (reader.api.av_find_best_stream)(
                reader.fmt,
                AVMEDIA_TYPE_VIDEO,
                -1,
                -1,
                std::ptr::null_mut(),
                0,
            );
            if index < 0
                || index as u32 >= (*reader.fmt).nb_streams
                || (*reader.fmt).streams.is_null()
            {
                return Err(IoError::Unsupported);
            }
            let stream = *(*reader.fmt).streams.add(index as usize);
            if stream.is_null() || (*stream).codecpar.is_null() {
                return Err(IoError::Invalid);
            }
            let params = &*(*stream).codecpar;
            let name = (reader.api.avcodec_get_name)(params.codec_id);
            if name.is_null() {
                return Err(IoError::Unsupported);
            }
            // FFmpeg大版本可能删除枚举项，不能把旧版HEVC数值173当永久合同。
            let codec = match std::ffi::CStr::from_ptr(name).to_bytes() {
                b"h264" => 1,
                b"hevc" => 2,
                b"mjpeg" => 4,
                _ => return Err(IoError::Unsupported),
            };
            if params.width <= 0 || params.height <= 0 {
                return Err(IoError::Invalid);
            }
            let extra = if params.extradata_size > 0 {
                if params.extradata.is_null() || params.extradata_size as usize > MAX_PACKET {
                    return Err(IoError::Resource);
                }
                std::slice::from_raw_parts(params.extradata, params.extradata_size as usize)
            } else {
                &[]
            };
            reader.framing = Framing::new(codec, extra)?;
            let timebase = (*stream.cast::<StreamPrefix>()).time_base;
            if timebase.num <= 0 || timebase.den <= 0 {
                return Err(IoError::Unsupported);
            }
            let rate = (reader.api.av_guess_frame_rate)(reader.fmt, stream, std::ptr::null_mut());
            reader.info = StreamInfo {
                codec,
                width: params.width as u32,
                height: params.height as u32,
                fps: if rate.num > 0 && rate.den > 0 {
                    f64::from(rate.num) / f64::from(rate.den)
                } else {
                    -1.0
                },
                time_base_num: timebase.num as u32,
                time_base_den: timebase.den as u32,
                extradata: reader.framing.parameters.clone(),
            };
            reader.stream = index;
            reader.packet = (reader.api.av_packet_alloc)();
            if reader.packet.is_null() {
                return Err(IoError::Resource);
            }
        }
        reader.check()?;
        Ok(reader)
    }
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }
    pub fn read_packet(&mut self) -> Result<Option<Packet>, IoError> {
        self.interrupt.deadline.set(
            Instant::now()
                .checked_add(self.timeout)
                .ok_or(IoError::Invalid)?,
        );
        loop {
            self.check()?;
            // SAFETY：packet 为有效独占 AVPacket，每个分支在再次读取前 unref。
            let rc = unsafe { (self.api.av_read_frame)(self.fmt, self.packet) };
            if rc == AVERROR_EOF {
                unsafe {
                    (self.api.av_packet_unref)(self.packet);
                }
                return Ok(None);
            }
            if rc == AVERROR_EAGAIN {
                unsafe {
                    (self.api.av_packet_unref)(self.packet);
                }
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            if let Err(error) = self.result(rc) {
                unsafe {
                    (self.api.av_packet_unref)(self.packet);
                }
                return Err(error);
            }
            let result = unsafe {
                let packet = &*self.packet;
                if packet.stream_index != self.stream {
                    None
                } else if packet.size <= 0 || packet.data.is_null() {
                    Some(Err(IoError::Invalid))
                } else if packet.size as usize > MAX_PACKET {
                    Some(Err(IoError::Resource))
                } else {
                    let bytes = std::slice::from_raw_parts(packet.data, packet.size as usize);
                    let keyframe = packet.flags & AV_PKT_FLAG_KEY != 0;
                    let converted = self.framing.packet(bytes, keyframe, self.info.codec);
                    if self.info.extradata != self.framing.parameters {
                        self.info.extradata.clone_from(&self.framing.parameters);
                    }
                    Some(converted.map(|data| Packet {
                        data,
                        pts: packet.pts,
                        dts: packet.dts,
                        keyframe,
                    }))
                }
            };
            unsafe {
                (self.api.av_packet_unref)(self.packet);
            }
            if let Some(result) = result {
                self.check()?;
                return result.map(Some);
            }
        }
    }
    fn check(&self) -> Result<(), IoError> {
        if self.interrupt.stop.load(Ordering::Acquire) {
            Err(IoError::Closed)
        } else if Instant::now() >= self.interrupt.deadline.get() {
            Err(IoError::Timeout)
        } else {
            Ok(())
        }
    }
    fn result(&self, rc: i32) -> Result<(), IoError> {
        self.check()?;
        if rc >= 0 {
            return Ok(());
        }
        // FFmpeg HTTP/RTSP 状态采用 AVERROR_HTTP_*；不回传可能含凭据的原生日志。
        if rc == -825242872 || rc == -858797304 {
            Err(IoError::Auth)
        } else if rc == -110 || rc == -10060 {
            Err(IoError::Timeout)
        } else {
            Err(IoError::Network)
        }
    }
}
