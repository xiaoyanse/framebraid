//! FFmpeg LGPL 动态库装载（P08 1.1）：对齐 vendor/snap7 模式。
//! libloading 以绝对路径加载（LOAD_WITH_ALTERED_SEARCH_PATH），依赖 DLL 从同目录解析；
//! 五个 DLL 全部显式装载后忘记句柄，进程生命周期不卸载；失败给可读错误码不 panic。
use std::path::PathBuf;
use std::sync::OnceLock;

use core::ffi::c_int;

use super::ffmpeg_sys::*;

// 函数指针类型（ABI 依据随包头文件）
type AvutilVersionFn = unsafe extern "C" fn() -> u32;
type AvFrameAllocFn = unsafe extern "C" fn() -> *mut AVFrame;
type AvFrameFreeFn = unsafe extern "C" fn(*mut *mut AVFrame);
type AvFrameUnrefFn = unsafe extern "C" fn(*mut AVFrame);
type AvPacketAllocFn = unsafe extern "C" fn() -> *mut AVPacket;
type AvPacketFreeFn = unsafe extern "C" fn(*mut *mut AVPacket);
type AvPacketUnrefFn = unsafe extern "C" fn(*mut AVPacket);
type AvBufferUnrefFn = unsafe extern "C" fn(*mut *mut AVBufferRef);
type AvBufferRefFn = unsafe extern "C" fn(*mut AVBufferRef) -> *mut AVBufferRef;
type AvHwdeviceIterateTypesFn = unsafe extern "C" fn(c_int) -> c_int;
type AvHwdeviceCtxCreateFn = unsafe extern "C" fn(
    *mut *mut AVBufferRef,
    c_int,
    *const core::ffi::c_char,
    *mut core::ffi::c_void,
    c_int,
) -> c_int;
type AvHwframeTransferDataFn = unsafe extern "C" fn(*mut AVFrame, *const AVFrame, c_int) -> c_int;
type AvHwframeTransferFormatsFn =
    unsafe extern "C" fn(*mut AVBufferRef, c_int, *mut *mut c_int, c_int) -> c_int;
type AvHwframeCtxInitFn = unsafe extern "C" fn(*mut AVBufferRef) -> c_int;
type AvcodecGetHwFramesParametersFn = unsafe extern "C" fn(
    *mut AVCodecContext,
    *mut AVBufferRef,
    c_int,
    *mut *mut AVBufferRef,
) -> c_int;
type AvFrameGetBufferFn = unsafe extern "C" fn(*mut AVFrame, c_int) -> c_int;
type AvImageGetBufferSizeFn = unsafe extern "C" fn(c_int, c_int, c_int, c_int) -> c_int;
type AvStrerrorFn = unsafe extern "C" fn(c_int, *mut core::ffi::c_char, usize) -> c_int;
type AvMemdupFn = unsafe extern "C" fn(*const core::ffi::c_void, usize) -> *mut core::ffi::c_void;
type AvcodecFindDecoderFn = unsafe extern "C" fn(c_int) -> *const core::ffi::c_void;
type AvcodecFindDecoderByNameFn =
    unsafe extern "C" fn(*const core::ffi::c_char) -> *const core::ffi::c_void;
type AvcodecAllocContext3Fn = unsafe extern "C" fn(*const core::ffi::c_void) -> *mut AVCodecContext;
type AvcodecFreeContextFn = unsafe extern "C" fn(*mut *mut AVCodecContext);
type AvcodecOpen2Fn = unsafe extern "C" fn(
    *mut AVCodecContext,
    *const core::ffi::c_void,
    *mut core::ffi::c_void,
) -> c_int;
type AvcodecSendPacketFn = unsafe extern "C" fn(*mut AVCodecContext, *const AVPacket) -> c_int;
type AvcodecReceiveFrameFn = unsafe extern "C" fn(*mut AVCodecContext, *mut AVFrame) -> c_int;
type AvcodecParametersToContextFn =
    unsafe extern "C" fn(*mut AVCodecContext, *const AVCodecParameters) -> c_int;
type AvcodecGetHwConfigFn =
    unsafe extern "C" fn(*const core::ffi::c_void, c_int) -> *const AVCodecHWConfig;
type AvformatNetworkInitFn = unsafe extern "C" fn() -> c_int;
type AvformatOpenInputFn = unsafe extern "C" fn(
    *mut *mut AVFormatContext,
    *const core::ffi::c_char,
    *const core::ffi::c_void,
    *mut *mut core::ffi::c_void,
) -> c_int;
type AvformatFindStreamInfoFn =
    unsafe extern "C" fn(*mut AVFormatContext, *mut core::ffi::c_void) -> c_int;
type AvformatCloseInputFn = unsafe extern "C" fn(*mut *mut AVFormatContext);
type AvformatAllocContextFn = unsafe extern "C" fn() -> *mut AVFormatContext;
// dict（libavutil/dict.h）：RTSP 打开选项（超时/TCP 传输）
type AvDictSetFn = unsafe extern "C" fn(
    *mut *mut AVDictionary,
    *const core::ffi::c_char,
    *const core::ffi::c_char,
    c_int,
) -> c_int;
type AvDictFreeFn = unsafe extern "C" fn(*mut *mut AVDictionary);
// dict 读回：open_input 后查剩余项（未被消费 = 选项未生效）
type AvDictGetFn = unsafe extern "C" fn(
    *const AVDictionary,
    *const core::ffi::c_char,
    *const AVDictionaryEntry,
    c_int,
) -> *mut AVDictionaryEntry;
// AVClass 选项系统写入：thread_type 等布局未抄字段经此设置
type AvOptSetIntFn =
    unsafe extern "C" fn(*mut core::ffi::c_void, *const core::ffi::c_char, i64, c_int) -> c_int;
type AvReadFrameFn = unsafe extern "C" fn(*mut AVFormatContext, *mut AVPacket) -> c_int;
type AvFindBestStreamFn = unsafe extern "C" fn(
    *mut AVFormatContext,
    c_int,
    c_int,
    c_int,
    *mut *mut core::ffi::c_void,
    c_int,
) -> c_int;
// 帧率估计（frame 可 NULL）：avg_frame_rate 优先回退 r_frame_rate
type AvGuessFrameRateFn =
    unsafe extern "C" fn(*mut AVFormatContext, *mut AVStream, *mut AVFrame) -> AVRational;
type SwsGetContextFn = unsafe extern "C" fn(
    c_int,
    c_int,
    c_int,
    c_int,
    c_int,
    c_int,
    c_int,
    *mut core::ffi::c_void,
    *mut core::ffi::c_void,
    *const f64,
) -> *mut SwsContext;
type SwsScaleFn = unsafe extern "C" fn(
    *mut SwsContext,
    *const *const u8, // const uint8_t *const srcSlice[]
    *const c_int,
    c_int,
    c_int,
    *const *mut u8, // uint8_t *const dst[]
    *const c_int,
) -> c_int;
type SwsFreeContextFn = unsafe extern "C" fn(*mut SwsContext);
// parser：裸流按 AU 拆包（RTSP demuxer 已保证每包一帧，此路径用于本地样本）
// 签名：(parser_ctx, avctx, poutbuf, poutbuf_size, buf, buf_size, pts, dts, pos)
type AvParserInitFn = unsafe extern "C" fn(c_int) -> *mut core::ffi::c_void;
type AvParserParse2Fn = unsafe extern "C" fn(
    *mut core::ffi::c_void,
    *mut AVCodecContext,
    *mut *mut u8,
    *mut c_int,
    *const u8,
    c_int,
    i64,
    i64,
    i64,
) -> c_int;
type AvParserCloseFn = unsafe extern "C" fn(*mut core::ffi::c_void);

/// 已解析符号表：全为裸函数指针，天然线程安全。
pub struct FfmpegApi {
    // avutil
    // 版本核对仅测试构建使用；装载期以符号存在性校验
    #[cfg_attr(not(test), allow(dead_code))]
    pub avutil_version: AvutilVersionFn,
    pub av_frame_alloc: AvFrameAllocFn,
    pub av_frame_free: AvFrameFreeFn,
    pub av_frame_unref: AvFrameUnrefFn,
    pub av_dict_set: AvDictSetFn,
    pub av_dict_free: AvDictFreeFn,
    pub av_dict_get: AvDictGetFn,
    pub av_opt_set_int: AvOptSetIntFn,
    pub av_packet_alloc: AvPacketAllocFn,
    pub av_packet_free: AvPacketFreeFn,
    pub av_packet_unref: AvPacketUnrefFn,
    pub av_buffer_unref: AvBufferUnrefFn,
    pub av_buffer_ref: AvBufferRefFn,
    pub av_hwdevice_iterate_types: AvHwdeviceIterateTypesFn,
    pub av_hwdevice_ctx_create: AvHwdeviceCtxCreateFn,
    pub av_hwframe_transfer_data: AvHwframeTransferDataFn,
    pub av_hwframe_transfer_get_formats: AvHwframeTransferFormatsFn,
    pub av_hwframe_ctx_init: AvHwframeCtxInitFn,
    pub avcodec_get_hw_frames_parameters: AvcodecGetHwFramesParametersFn,
    pub av_frame_get_buffer: AvFrameGetBufferFn,
    pub av_frame_get_plane_buffer: unsafe extern "C" fn(*const AVFrame, c_int) -> *mut AVBufferRef,
    pub av_image_get_buffer_size: AvImageGetBufferSizeFn,
    pub av_free: unsafe extern "C" fn(*mut core::ffi::c_void),
    pub av_strerror: AvStrerrorFn,
    pub av_log_get_level: unsafe extern "C" fn() -> c_int,
    pub av_log_set_level: unsafe extern "C" fn(c_int),
    pub av_memdup: AvMemdupFn,
    // avcodec
    pub avcodec_find_decoder: AvcodecFindDecoderFn,
    pub avcodec_get_name: unsafe extern "C" fn(c_int) -> *const core::ffi::c_char,
    pub avcodec_find_decoder_by_name: AvcodecFindDecoderByNameFn,
    pub avcodec_alloc_context3: AvcodecAllocContext3Fn,
    pub avcodec_free_context: AvcodecFreeContextFn,
    pub avcodec_open2: AvcodecOpen2Fn,
    pub avcodec_send_packet: AvcodecSendPacketFn,
    pub avcodec_receive_frame: AvcodecReceiveFrameFn,
    pub avcodec_parameters_to_context: AvcodecParametersToContextFn,
    pub avcodec_get_hw_config: AvcodecGetHwConfigFn,
    // avformat
    pub avformat_network_init: AvformatNetworkInitFn,
    pub avformat_alloc_context: AvformatAllocContextFn,
    pub avformat_open_input: AvformatOpenInputFn,
    pub avformat_find_stream_info: AvformatFindStreamInfoFn,
    pub avformat_close_input: AvformatCloseInputFn,
    pub av_read_frame: AvReadFrameFn,
    pub av_find_best_stream: AvFindBestStreamFn,
    pub av_guess_frame_rate: AvGuessFrameRateFn,
    // swscale
    pub sws_get_context: SwsGetContextFn,
    pub sws_scale: SwsScaleFn,
    pub sws_free_context: SwsFreeContextFn,
    // parser（libavcodec）
    pub av_parser_init: AvParserInitFn,
    pub av_parser_parse2: AvParserParse2Fn,
    pub av_parser_close: AvParserCloseFn,
}

/// DLL 装载清单：avutil 无依赖先载，其余按依赖顺序。
const DLLS: [&str; 5] = [
    "avutil-61.dll",
    "swresample-7.dll",
    "swscale-10.dll",
    "avcodec-63.dll",
    "avformat-63.dll",
];

/// 只保留库名、稳定原因和OS码，不把路径、URL或认证信息带入诊断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FfmpegLoadError {
    pub key: &'static str,
    pub component: &'static str,
    pub os_code: Option<i32>,
}
impl FfmpegLoadError {
    fn new(key: &'static str, component: &'static str) -> Self {
        Self {
            key,
            component,
            os_code: None,
        }
    }
    #[cfg(not(windows))]
    fn library(component: &'static str, error: libloading::Error) -> Self {
        let os_code = std::error::Error::source(&error)
            .and_then(|source| source.downcast_ref::<std::io::Error>())
            .and_then(std::io::Error::raw_os_error);
        Self {
            key: "media.ffmpegLibraryLoadFailed",
            component,
            os_code,
        }
    }
}

#[cfg(windows)]
fn load_library(
    path: &std::path::Path,
    component: &'static str,
) -> Result<libloading::Library, FfmpegLoadError> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(path: *const u16, file: isize, flags: u32) -> isize;
        fn GetLastError() -> u32;
        fn SetThreadErrorMode(mode: u32, previous: *mut u32) -> i32;
    }
    let mut filename: Vec<u16> = path.as_os_str().encode_wide().collect();
    if filename.contains(&0) {
        return Err(FfmpegLoadError::new(
            "media.ffmpegLibraryLoadFailed",
            component,
        ));
    }
    filename.push(0);
    let mut previous_mode = 0;
    // libloading 0.9把OS码藏在私有WindowsError中；在原生装载瞬间捕获，
    // 不能在错误模式恢复后重读GetLastError，也不能解析本地化错误文本。
    let (handle, os_code) = unsafe {
        let changed = SetThreadErrorMode(1, &mut previous_mode) != 0;
        let handle = LoadLibraryExW(filename.as_ptr(), 0, 0);
        let code = if handle == 0 {
            Some(GetLastError() as i32)
        } else {
            None
        };
        if changed {
            SetThreadErrorMode(previous_mode, std::ptr::null_mut());
        }
        (handle, code)
    };
    if handle == 0 {
        return Err(FfmpegLoadError {
            key: "media.ffmpegLibraryLoadFailed",
            component,
            os_code,
        });
    }
    // 句柄只有一份所有权，交回libloading沿用原来的失败回收/成功常驻策略。
    Ok(unsafe { libloading::os::windows::Library::from_raw(handle) }.into())
}

#[cfg(not(windows))]
fn load_library(
    path: &std::path::Path,
    component: &'static str,
) -> Result<libloading::Library, FfmpegLoadError> {
    unsafe { libloading::Library::new(path) }
        .map_err(|error| FfmpegLoadError::library(component, error))
}

/// DLL 目录探测候选（纯函数，测试复用）：
/// AM_RT_FFMPEG_DIR → exe 旁 → 向上五级 vendor/ffmpeg/win-x64/bin。
pub fn probe_dirs(env_value: Option<std::ffi::OsString>, exe: Option<PathBuf>) -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Some(from_env) = env_value {
        list.push(PathBuf::from(from_env));
    }
    if let Some(exe) = exe {
        list.push(
            exe.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| exe.clone()),
        );
        for ancestor in exe.ancestors().skip(1).take(5) {
            list.push(
                ancestor
                    .join("vendor")
                    .join("ffmpeg")
                    .join("win-x64")
                    .join("bin"),
            );
        }
    }
    list
}

impl FfmpegApi {
    /// 从指定目录按依赖顺序装载并解析符号。
    pub fn load_from(dir: &std::path::Path) -> Result<Self, &'static str> {
        Self::load_from_detailed(dir).map_err(|error| error.key)
    }
    pub fn load_from_detailed(dir: &std::path::Path) -> Result<Self, FfmpegLoadError> {
        // 依赖链一致性：候选目录必须齐全五个 DLL，缺一即失败（防混用系统半套库）。
        if let Some(missing) = DLLS.iter().find(|dll| !dir.join(dll).is_file()) {
            return Err(FfmpegLoadError::new("media.ffmpegIncomplete", missing));
        }
        let mut loaded = Vec::new();
        for dll in DLLS {
            // SAFETY：仅按登记路径装载动态库；绝对路径使依赖 DLL 同目录解析。
            let library = load_library(&dir.join(dll), dll)?;
            loaded.push(library);
        }
        // SAFETY：符号名 NUL 结尾；库忘记句柄后进程生命周期不卸载，指针长期有效。
        unsafe {
            // 取符号为裸函数指针：T 为函数指针类型，get 以 T 反解符号（对齐 snap7.rs 模式）
            unsafe fn get_fn<T: Copy>(
                lib: &libloading::Library,
                name: &[u8],
            ) -> Result<T, FfmpegLoadError> {
                // SAFETY：T 为函数指针类型；符号存在性由 get 校验。
                let symbol: libloading::Symbol<T> = lib
                    .get(name)
                    .map_err(|_| FfmpegLoadError::new("media.ffmpegSymbolMissing", "ffmpeg"))?;
                Ok(*symbol)
            }
            let [avutil, swresample, swscale, avcodec, avformat] = &loaded[..] else {
                return Err(FfmpegLoadError::new(
                    "media.ffmpegLibraryLoadFailed",
                    "ffmpeg",
                ));
            };
            // 手工FFI前缀只对冻结主版本有效，不能仅凭DLL文件名相信布局兼容。
            for (library, symbol, major) in [
                (avutil, b"avutil_version\0".as_slice(), 61),
                (avcodec, b"avcodec_version\0".as_slice(), 63),
                (avformat, b"avformat_version\0".as_slice(), 63),
                (swscale, b"swscale_version\0".as_slice(), 10),
                (swresample, b"swresample_version\0".as_slice(), 7),
            ] {
                let version: AvutilVersionFn = get_fn(library, symbol)?;
                if version() >> 16 != major {
                    return Err(FfmpegLoadError::new(
                        "media.ffmpegVersionMismatch",
                        "ffmpeg",
                    ));
                }
            }
            let api = FfmpegApi {
                avutil_version: get_fn(avutil, b"avutil_version\0")?,
                av_frame_alloc: get_fn(avutil, b"av_frame_alloc\0")?,
                av_frame_free: get_fn(avutil, b"av_frame_free\0")?,
                av_frame_unref: get_fn(avutil, b"av_frame_unref\0")?,
                av_dict_set: get_fn(avutil, b"av_dict_set\0")?,
                av_dict_free: get_fn(avutil, b"av_dict_free\0")?,
                av_dict_get: get_fn(avutil, b"av_dict_get\0")?,
                av_opt_set_int: get_fn(avutil, b"av_opt_set_int\0")?,
                // packet API 属 libavcodec（packet.h），不在 libavutil
                av_packet_alloc: get_fn(avcodec, b"av_packet_alloc\0")?,
                av_packet_free: get_fn(avcodec, b"av_packet_free\0")?,
                av_packet_unref: get_fn(avcodec, b"av_packet_unref\0")?,
                av_buffer_unref: get_fn(avutil, b"av_buffer_unref\0")?,
                av_buffer_ref: get_fn(avutil, b"av_buffer_ref\0")?,
                av_hwdevice_iterate_types: get_fn(avutil, b"av_hwdevice_iterate_types\0")?,
                av_hwdevice_ctx_create: get_fn(avutil, b"av_hwdevice_ctx_create\0")?,
                av_hwframe_transfer_data: get_fn(avutil, b"av_hwframe_transfer_data\0")?,
                av_hwframe_transfer_get_formats: get_fn(
                    avutil,
                    b"av_hwframe_transfer_get_formats\0",
                )?,
                av_hwframe_ctx_init: get_fn(avutil, b"av_hwframe_ctx_init\0")?,
                avcodec_get_hw_frames_parameters: get_fn(
                    avcodec,
                    b"avcodec_get_hw_frames_parameters\0",
                )?,
                av_frame_get_buffer: get_fn(avutil, b"av_frame_get_buffer\0")?,
                av_frame_get_plane_buffer: get_fn(avutil, b"av_frame_get_plane_buffer\0")?,
                av_image_get_buffer_size: get_fn(avutil, b"av_image_get_buffer_size\0")?,
                av_free: get_fn(avutil, b"av_free\0")?,
                av_strerror: get_fn(avutil, b"av_strerror\0")?,
                av_log_get_level: get_fn(avutil, b"av_log_get_level\0")?,
                av_log_set_level: get_fn(avutil, b"av_log_set_level\0")?,
                av_memdup: get_fn(avutil, b"av_memdup\0")?,
                avcodec_find_decoder: get_fn(avcodec, b"avcodec_find_decoder\0")?,
                avcodec_get_name: get_fn(avcodec, b"avcodec_get_name\0")?,
                avcodec_find_decoder_by_name: get_fn(avcodec, b"avcodec_find_decoder_by_name\0")?,
                avcodec_alloc_context3: get_fn(avcodec, b"avcodec_alloc_context3\0")?,
                avcodec_free_context: get_fn(avcodec, b"avcodec_free_context\0")?,
                avcodec_open2: get_fn(avcodec, b"avcodec_open2\0")?,
                avcodec_send_packet: get_fn(avcodec, b"avcodec_send_packet\0")?,
                avcodec_receive_frame: get_fn(avcodec, b"avcodec_receive_frame\0")?,
                avcodec_parameters_to_context: get_fn(avcodec, b"avcodec_parameters_to_context\0")?,
                avcodec_get_hw_config: get_fn(avcodec, b"avcodec_get_hw_config\0")?,
                avformat_network_init: get_fn(avformat, b"avformat_network_init\0")?,
                avformat_alloc_context: get_fn(avformat, b"avformat_alloc_context\0")?,
                avformat_open_input: get_fn(avformat, b"avformat_open_input\0")?,
                avformat_find_stream_info: get_fn(avformat, b"avformat_find_stream_info\0")?,
                avformat_close_input: get_fn(avformat, b"avformat_close_input\0")?,
                av_read_frame: get_fn(avformat, b"av_read_frame\0")?,
                av_find_best_stream: get_fn(avformat, b"av_find_best_stream\0")?,
                av_guess_frame_rate: get_fn(avformat, b"av_guess_frame_rate\0")?,
                sws_get_context: get_fn(swscale, b"sws_getContext\0")?,
                sws_scale: get_fn(swscale, b"sws_scale\0")?,
                sws_free_context: get_fn(swscale, b"sws_freeContext\0")?,
                av_parser_init: get_fn(avcodec, b"av_parser_init\0")?,
                av_parser_parse2: get_fn(avcodec, b"av_parser_parse2\0")?,
                av_parser_close: get_fn(avcodec, b"av_parser_close\0")?,
            };
            // swresample 由 avcodec 动态依赖自动解析，无直接调用符号
            let _ = swresample;
            // 库句柄全部忘记：符号指针在进程生命周期内有效
            for library in loaded {
                std::mem::forget(library);
            }
            Ok(api)
        }
    }

    /// 错误码转人读文本（av_strerror 兜底十六进制）。
    pub fn errstr(&self, code: i32) -> String {
        let mut buf = [0 as core::ffi::c_char; 128];
        // SAFETY：buf 长度与调用约定一致，仅读取返回时写入的 NUL 结尾内容。
        let ok = unsafe { (self.av_strerror)(code, buf.as_mut_ptr(), buf.len()) };
        if ok == 0 {
            let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
            String::from_utf8_lossy(
                // SAFETY：av_strerror 写入的是合法 UTF-8 错误文本。
                unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), end) },
            )
            .into_owned()
        } else {
            format!("ffmpeg error {code}")
        }
    }
}

/// 全局单例：首个调用方触发装载，之后复用。
pub fn api() -> Result<&'static FfmpegApi, &'static str> {
    static API: OnceLock<Result<FfmpegApi, &'static str>> = OnceLock::new();
    API.get_or_init(|| {
        let candidates = probe_dirs(
            std::env::var_os("AM_RT_FFMPEG_DIR"),
            std::env::current_exe().ok(),
        );
        let Some(dir) = candidates.iter().find(|d| d.join(DLLS[0]).is_file()) else {
            return Err("media.ffmpegDirMissing");
        };
        FfmpegApi::load_from(dir)
    })
    .as_ref()
    .map_err(|code| *code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_dirs_env_first() {
        let list = probe_dirs(Some(std::ffi::OsString::from("Z:/ff/bin")), None);
        assert_eq!(list[0], PathBuf::from("Z:/ff/bin"));
        let exe = PathBuf::from("C:/app/bin/app.exe");
        let list = probe_dirs(None, Some(exe));
        assert_eq!(list[0], PathBuf::from("C:/app/bin"));
        assert!(list
            .iter()
            .any(|p| p.ends_with("vendor/ffmpeg/win-x64/bin")));
    }

    #[test]
    fn load_reports_incomplete_dir() {
        let tmp = std::env::temp_dir().join("p08_ffmpeg_missing_probe");
        std::fs::create_dir_all(&tmp).ok();
        assert_eq!(
            FfmpegApi::load_from(&tmp).err(),
            Some("media.ffmpegIncomplete")
        );
    }
    #[test]
    fn detailed_failure_identifies_missing_component_without_directory_contents() {
        let tmp =
            std::env::temp_dir().join(format!("am-sdk-ffmpeg-diagnostic-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("avutil-61.dll"), b"fixture").unwrap();
        let error = FfmpegApi::load_from_detailed(&tmp).err().unwrap();
        assert_eq!(error.key, "media.ffmpegIncomplete");
        assert_eq!(error.component, "swresample-7.dll");
        assert_eq!(error.os_code, None);
        std::fs::remove_file(tmp.join("avutil-61.dll")).unwrap();
        std::fs::remove_dir(tmp).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn detailed_invalid_image_preserves_native_error_before_mode_restore() {
        let tmp = tempfile::tempdir().unwrap();
        for name in DLLS {
            std::fs::write(tmp.path().join(name), b"invalid PE image").unwrap();
        }
        let error = FfmpegApi::load_from_detailed(tmp.path()).err().unwrap();
        assert_eq!(error.key, "media.ffmpegLibraryLoadFailed");
        assert_eq!(error.component, "avutil-61.dll");
        assert_eq!(error.os_code, Some(193));
    }
}
