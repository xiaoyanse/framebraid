//! FFmpeg LGPL 共享库手写最小 FFI（P08 1.1 技术门）。
//! 布局依据随包 include/ 头文件（BtbN N-127941-g303510f9d1-20261022，libavcodec 63 / libavutil 61.16），
//! 升级大版本须按新头文件复核（见 vendor/ffmpeg/win-x64/README.md）。
//! 仅声明本项目用到的符号；AVCodecContext 抄至 hw_device_ctx/thread_count（thread_count 作布局哨兵），
//! AVFrame/AVPacket/AVCodecParameters/AVFormatContext/AVStream 只抄访问到的前缀字段。

#![allow(non_snake_case)]
#![allow(non_camel_case_types)]

use core::ffi::{c_char, c_int, c_uint, c_void};

/// AVRational（libavutil/rational.h）。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AVRational {
    pub num: c_int,
    pub den: c_int,
}

/// AVChannelLayout（libavutil/channel_layout.h，24 字节）。
#[repr(C)]
pub struct AVChannelLayout {
    pub order: c_int,
    pub nb_channels: c_int,
    pub u: AVChannelLayoutUnion,
    pub opaque: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union AVChannelLayoutUnion {
    pub mask: u64,
    pub map: *mut c_void,
}

/// get_format 回调（返回选中的像素格式）。
pub type GetFormatFn = unsafe extern "C" fn(*mut AVCodecContext, *const c_int) -> c_int;

/// AVCodecContext 前缀：抄至 thread_count（哨兵）为止，其后字段不访问。
/// 字段顺序与 avcodec.h 一一对应（含 libavcodec 63 保留的废弃字段）。
#[repr(C)]
pub struct AVCodecContext {
    pub av_class: *const c_void,
    pub log_level_offset: c_int,
    pub codec_type: c_int,
    pub codec: *const c_void,
    pub codec_id: c_int,
    pub codec_tag: c_uint,
    pub priv_data: *mut c_void,
    pub internal: *mut c_void,
    pub opaque: *mut c_void,
    pub bit_rate: i64,
    pub flags: c_int,
    pub flags2: c_int,
    pub extradata: *mut u8,
    pub extradata_size: c_int,
    pub time_base: AVRational,
    pub pkt_timebase: AVRational,
    pub framerate: AVRational,
    pub delay: c_int,
    pub width: c_int,
    pub height: c_int,
    pub coded_width: c_int,
    pub coded_height: c_int,
    pub sample_aspect_ratio: AVRational,
    pub pix_fmt: c_int,
    pub sw_pix_fmt: c_int,
    pub color_primaries: c_int,
    pub color_trc: c_int,
    pub colorspace: c_int,
    pub color_range: c_int,
    pub chroma_sample_location: c_int,
    pub field_order: c_int,
    pub refs: c_int,
    pub has_b_frames: c_int,
    pub slice_flags: c_int,
    pub draw_horiz_band: *const c_void,
    pub get_format: Option<GetFormatFn>,
    pub max_b_frames: c_int,
    pub b_quant_factor: f32,
    pub b_quant_offset: f32,
    pub i_quant_factor: f32,
    pub i_quant_offset: f32,
    pub lumi_masking: f32,
    pub temporal_cplx_masking: f32,
    pub spatial_cplx_masking: f32,
    pub p_masking: f32,
    pub dark_masking: f32,
    pub nsse_weight: c_int,
    pub me_cmp: c_int,
    pub me_sub_cmp: c_int,
    pub mb_cmp: c_int,
    pub ildct_cmp: c_int,
    pub dia_size: c_int,
    pub last_predictor_count: c_int,
    pub me_pre_cmp: c_int,
    pub pre_dia_size: c_int,
    pub me_subpel_quality: c_int,
    pub me_range: c_int,
    pub mb_decision: c_int,
    pub intra_matrix: *mut u16,
    pub inter_matrix: *mut u16,
    pub chroma_intra_matrix: *mut u16,
    pub intra_dc_precision: c_int,
    pub mb_lmin: c_int,
    pub mb_lmax: c_int,
    pub bidir_refine: c_int,
    pub keyint_min: c_int,
    pub gop_size: c_int,
    pub mv0_threshold: c_int,
    pub slices: c_int,
    pub sample_rate: c_int,
    pub sample_fmt: c_int,
    pub ch_layout: AVChannelLayout,
    pub frame_size: c_int,
    pub block_align: c_int,
    pub cutoff: c_int,
    pub audio_service_type: c_int,
    pub request_sample_fmt: c_int,
    pub initial_padding: c_int,
    pub trailing_padding: c_int,
    pub seek_preroll: c_int,
    pub get_buffer2: *const c_void,
    pub bit_rate_tolerance: c_int,
    pub global_quality: c_int,
    pub compression_level: c_int,
    pub qcompress: f32,
    pub qblur: f32,
    pub qmin: c_int,
    pub qmax: c_int,
    pub max_qdiff: c_int,
    pub rc_buffer_size: c_int,
    pub rc_override_count: c_int,
    pub rc_override: *mut c_void,
    pub rc_max_rate: i64,
    pub rc_min_rate: i64,
    pub rc_max_available_vbv_use: f32,
    pub rc_min_vbv_overflow_use: f32,
    pub rc_initial_buffer_occupancy: c_int,
    pub trellis: c_int,
    pub stats_out: *mut c_char,
    pub stats_in: *mut c_char,
    pub workaround_bugs: c_int,
    pub strict_std_compliance: c_int,
    pub error_concealment: c_int,
    pub debug: c_int,
    pub err_recognition: c_int,
    pub hwaccel: *const c_void,
    pub hwaccel_context: *mut c_void,
    pub hw_frames_ctx: *mut AVBufferRef,
    pub hw_device_ctx: *mut AVBufferRef,
    pub hwaccel_flags: c_int,
    pub extra_hw_frames: c_int,
    pub error: [u64; 8],
    pub dct_algo: c_int,
    pub idct_algo: c_int,
    pub bits_per_coded_sample: c_int,
    pub bits_per_raw_sample: c_int,
    /// 布局哨兵：av_opt_set_int(ctx,"threads",N) 后读回应一致，用于验证前缀无偏移。
    pub thread_count: c_int,
}

/// AVFrame 前缀：帧平面与基本属性（libavutil/frame.h）。
#[repr(C)]
pub struct AVFrame {
    pub data: [*mut u8; 8],
    pub linesize: [c_int; 8],
    pub extended_data: *mut *mut u8,
    pub width: c_int,
    pub height: c_int,
    pub nb_samples: c_int,
    pub format: c_int,
    pub pict_type: c_int,
    pub sample_aspect_ratio: AVRational,
    pub pts: i64,
    pub pkt_dts: i64,
}

/// AVPacket（libavcodec/packet.h）。
#[repr(C)]
pub struct AVPacket {
    pub buf: *mut AVBufferRef,
    pub pts: i64,
    pub dts: i64,
    pub data: *mut u8,
    pub size: c_int,
    pub stream_index: c_int,
    pub flags: c_int,
    pub side_data: *mut c_void,
    pub side_data_elems: c_int,
    pub duration: i64,
    pub pos: i64,
}

/// AVCodecHWConfig（libavcodec/codec.h）。
#[repr(C)]
pub struct AVCodecHWConfig {
    pub pix_fmt: c_int,
    pub methods: c_int,
    pub device_type: c_int,
}

/// AVHWFramesContext 前缀（libavutil/hwcontext.h，本运行库 61.16 字段序）：
/// 仅访问到 hwctx；hwctx 指向 AVD3D11VAFramesContext。
#[repr(C)]
pub struct AVHWFramesContext {
    pub av_class: *const c_void,
    pub device_ref: *mut AVBufferRef,
    pub device_ctx: *mut c_void,
    pub hwctx: *mut c_void,
    pub free: *mut c_void,
    pub user_opaque: *mut c_void,
    pub pool: *mut c_void,
    pub initial_pool_size: c_int,
    pub format: c_int,
    pub sw_format: c_int,
    pub width: c_int,
    pub height: c_int,
}
const _: () = assert!(std::mem::offset_of!(AVHWFramesContext, hwctx) == 24);

/// AVD3D11VAFramesContext（libavutil/hwcontext_d3d11va.h，本运行库 61.16 字段序）。
#[repr(C)]
pub struct AVD3D11VAFramesContext {
    pub texture: *mut c_void,
    pub bind_flags: c_uint,
    pub misc_flags: c_uint,
    pub texture_infos: *mut c_void,
}
const _: () = assert!(std::mem::offset_of!(AVD3D11VAFramesContext, misc_flags) == 12);

/// AVCodecParameters 前缀（libavcodec/codec_par.h）。
#[repr(C)]
pub struct AVCodecParameters {
    pub codec_type: c_int,
    pub codec_id: c_int,
    pub codec_tag: c_uint,
    pub extradata: *mut u8,
    pub extradata_size: c_int,
    pub coded_side_data: *mut c_void,
    pub nb_coded_side_data: c_int,
    pub format: c_int,
    pub bit_rate: i64,
    pub bits_per_coded_sample: c_int,
    pub bits_per_raw_sample: c_int,
    pub profile: c_int,
    pub level: c_int,
    pub width: c_int,
    pub height: c_int,
}

/// AVIOInterruptCB（libavformat/avio.h）：返回非 0 中断阻塞 IO。
#[repr(C)]
pub struct AVIOInterruptCB {
    pub callback: Option<AvioInterruptFn>,
    pub opaque: *mut c_void,
}

pub type AvioInterruptFn = unsafe extern "C" fn(*mut c_void) -> c_int;

/// AVFormatContext 前缀（libavformat/avformat.h，布局核对至 interrupt_callback）：
/// 2026-10-23 按 FFmpeg master avformat.h 全字段核对（streams 后为 stream_groups/
/// chapters/url 等，url 已由 filename[1024] 改为 char*）；不再向后扩展。
#[repr(C)]
pub struct AVFormatContext {
    pub av_class: *const c_void,
    pub iformat: *const c_void,
    pub oformat: *const c_void,
    pub priv_data: *mut c_void,
    pub pb: *mut c_void,
    pub ctx_flags: c_int,
    pub nb_streams: c_uint,
    pub streams: *mut *mut AVStream,
    pub nb_stream_groups: c_uint,
    pub stream_groups: *mut *mut c_void,
    pub nb_chapters: c_uint,
    pub chapters: *mut *mut c_void,
    pub url: *mut c_char,
    pub start_time: i64,
    pub duration: i64,
    pub bit_rate: i64,
    pub packet_size: c_uint,
    pub max_delay: c_int,
    pub flags: c_int,
    pub probesize: i64,
    pub max_analyze_duration: i64,
    pub key: *const u8,
    pub keylen: c_int,
    pub nb_programs: c_uint,
    pub programs: *mut *mut c_void,
    pub video_codec_id: c_int,
    pub audio_codec_id: c_int,
    pub subtitle_codec_id: c_int,
    pub data_codec_id: c_int,
    pub metadata: *mut AVDictionary,
    pub start_time_realtime: i64,
    pub fps_probe_size: c_int,
    pub error_recognition: c_int,
    pub interrupt_callback: AVIOInterruptCB,
}

/// AVStream 前缀（libavformat/avformat.h）：输出 muxing 需读 time_base。
#[repr(C)]
pub struct AVStream {
    pub av_class: *const c_void,
    pub index: c_int,
    pub id: c_int,
    pub codecpar: *mut AVCodecParameters,
    pub priv_data: *mut c_void,
    pub time_base: AVRational,
}

/// AVOutputFormat 前缀（libavformat/avformat.h，本运行库无 av_class 首字段）：
/// 仅读 flags 判定 AVFMT_GLOBALHEADER。
#[repr(C)]
pub struct AVOutputFormat {
    pub name: *const c_char,
    pub long_name: *const c_char,
    pub mime_type: *const c_char,
    pub extensions: *const c_char,
    pub audio_codec: c_int,
    pub video_codec: c_int,
    pub subtitle_codec: c_int,
    pub flags: c_int,
}

/// AVBufferRef（libavutil/buffer.h）；用于核对实际CPU回读分配大小。
#[repr(C)]
pub struct AVBufferRef {
    pub buffer: *mut c_void,
    pub data: *mut u8,
    pub size: usize,
}

#[repr(C)]
pub struct SwsContext {
    _opaque: [u8; 0],
}

/// AVDictionary 按不透明指针使用（open_input 选项传递）。
#[repr(C)]
pub struct AVDictionary {
    _opaque: [u8; 0],
}

/// AVDictionaryEntry（libavutil/dict.h）：key/value NUL 结尾。
#[repr(C)]
pub struct AVDictionaryEntry {
    pub key: *mut c_char,
    pub value: *mut c_char,
}

// 像素格式（libavutil/pixfmt.h 枚举序号，随包头文件实测）
pub const AV_PIX_FMT_YUV420P: c_int = 0;
pub const AV_PIX_FMT_RGB24: c_int = 2;
pub const AV_PIX_FMT_BGR24: c_int = 3;
pub const AV_PIX_FMT_NV12: c_int = 23;
pub const AV_PIX_FMT_RGBA: c_int = 26;
pub const AV_PIX_FMT_DXVA2_VLD: c_int = 51;
pub const AV_PIX_FMT_QSV: c_int = 114;
pub const AV_PIX_FMT_CUDA: c_int = 117;
pub const AV_PIX_FMT_RGB0: c_int = 119;
pub const AV_PIX_FMT_D3D11: c_int = 171;

// AVHWDeviceType（libavutil/hwcontext.h 枚举序号）
pub const AV_HWDEVICE_TYPE_NONE: c_int = 0;
pub const AV_HWDEVICE_TYPE_CUDA: c_int = 2;
pub const AV_HWDEVICE_TYPE_DXVA2: c_int = 4;
pub const AV_HWDEVICE_TYPE_QSV: c_int = 5;
pub const AV_HWDEVICE_TYPE_D3D11VA: c_int = 7;
pub const AV_HWDEVICE_TYPE_D3D12VA: c_int = 12;
pub const AV_HWDEVICE_TYPE_AMF: c_int = 13;

// 解码器硬件配置方式（libavcodec/codec.h）
pub const AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX: c_int = 0x01;

// 媒体类型
pub const AVMEDIA_TYPE_VIDEO: c_int = 0;

// AVFMT_FLAG_NOBUFFER（libavformat/avformat.h）：解复用前不缓冲包（低延迟）
pub const AVFMT_FLAG_NOBUFFER: c_int = 0x0040;
// AV_CODEC_FLAG_LOW_DELAY（libavcodec/avcodec.h）：解码器低延迟模式
pub const AV_CODEC_FLAG_LOW_DELAY: c_int = 0x0008_0000;
// AV_CODEC_FLAG_GLOBAL_HEADER（1<<22）：编码器把参数集存 extradata 供 muxer 写头
pub const AV_CODEC_FLAG_GLOBAL_HEADER: c_int = 1 << 22;
// AVFMT_GLOBALHEADER（avformat.h）：该输出格式要求全局头
pub const AVFMT_GLOBALHEADER: c_int = 0x0040;
// AVIO_FLAG_WRITE（libavformat/avio.h）：avio_open 写模式
pub const AVIO_FLAG_WRITE: c_int = 2;
// SWS_AREA（libswscale/swscale.h）：面积平均下采样，即参考项目 INTER_AREA 同算法
pub const SWS_AREA: c_int = 0x0020;

// 包标志（libavcodec/packet.h）：关键帧
pub const AV_PKT_FLAG_KEY: c_int = 0x0001;

// 错误码：AVERROR(EAGAIN)（Win32 EAGAIN=11）与 AVERROR_EOF（MKTAG('E','O','F',' ')）
pub const AVERROR_EAGAIN: c_int = -11;
pub const AVERROR_EOF: c_int = -0x20464F45;

// 日志级别
pub const AV_LOG_QUIET: c_int = -8;
pub const AV_LOG_ERROR: c_int = 16;
