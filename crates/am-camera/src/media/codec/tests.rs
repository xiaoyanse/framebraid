use super::*;
use std::{
    ffi::{c_int, c_void},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        OnceLock,
    },
};
static CONVERSION_API: OnceLock<Arc<FfmpegApi>> = OnceLock::new();
static CREATED: AtomicUsize = AtomicUsize::new(0);
static FREED: AtomicUsize = AtomicUsize::new(0);
static FAIL_CREATE: AtomicBool = AtomicBool::new(false);
static FAIL_SCALE: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn conversion_context(
    a: c_int,
    b: c_int,
    c: c_int,
    d: c_int,
    e: c_int,
    f: c_int,
    g: c_int,
    h: *mut c_void,
    i: *mut c_void,
    j: *const f64,
) -> *mut SwsContext {
    if FAIL_CREATE.swap(false, Ordering::Relaxed) {
        return ptr::null_mut();
    }
    let Some(api) = CONVERSION_API.get() else {
        return ptr::null_mut();
    };
    let context = unsafe { (api.sws_get_context)(a, b, c, d, e, f, g, h, i, j) };
    if !context.is_null() {
        CREATED.fetch_add(1, Ordering::Relaxed);
    }
    context
}
unsafe extern "C" fn conversion_free(context: *mut SwsContext) {
    if let Some(api) = CONVERSION_API.get() {
        if !context.is_null() {
            FREED.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { (api.sws_free_context)(context) };
    }
}
unsafe extern "C" fn conversion_scale(
    ctx: *mut SwsContext,
    src: *const *const u8,
    stride: *const c_int,
    y: c_int,
    h: c_int,
    dst: *const *mut u8,
    out_stride: *const c_int,
) -> c_int {
    let Some(api) = CONVERSION_API.get() else {
        return -12;
    };
    let rows = unsafe { (api.sws_scale)(ctx, src, stride, y, h, dst, out_stride) };
    if FAIL_SCALE.swap(false, Ordering::Relaxed) {
        -5
    } else {
        rows
    }
}
fn color(frame: &DecodedRgba, format: u32, mut expected: [u8; 3]) {
    if format == 2 {
        expected.swap(0, 2);
    }
    for pixel in frame.data.chunks_exact(if format == 0 { 4 } else { 3 }) {
        for (actual, want) in pixel.iter().zip(expected) {
            assert!(actual.abs_diff(want) <= 5);
        }
        if format == 0 {
            assert_eq!(pixel[3], 255);
        }
    }
}

#[test]
#[ignore = "显式设置AM_SDK_FFMPEG_DIR和AM_SDK_CODEC_SAMPLES，需one.h264红色及alternate.h264蓝色"]
fn real_conversion_cache_reconfigures_and_recovers_without_overwriting_outputs() {
    let runtime = std::env::var_os("AM_SDK_FFMPEG_DIR").unwrap();
    let directory = std::path::PathBuf::from(std::env::var_os("AM_SDK_CODEC_SAMPLES").unwrap());
    let reference = Arc::new(FfmpegApi::load_from(std::path::Path::new(&runtime)).unwrap());
    assert!(CONVERSION_API.set(reference).is_ok());
    let mut api = FfmpegApi::load_from(std::path::Path::new(&runtime)).unwrap();
    api.sws_get_context = conversion_context;
    api.sws_free_context = conversion_free;
    api.sws_scale = conversion_scale;
    let mut decoder = CpuDecoder::create(Arc::new(api), VideoCodec::H264, &[], 256 * 256).unwrap();
    let red = std::fs::read(directory.join("one.h264")).unwrap();
    let blue = std::fs::read(directory.join("alternate.h264")).unwrap();
    let mut first = None;
    for pts in 0..3 {
        assert!(decoder.send(&red, pts, pts).unwrap());
        let frame = decoder.copy_rgba(256 * 144 * 4).unwrap().unwrap();
        color(&frame, 0, [255, 0, 0]);
        first.get_or_insert(frame);
    }
    assert_eq!(CREATED.load(Ordering::Relaxed), 1);
    let same_size_blue = std::fs::read(directory.join("same-size-blue.h264")).unwrap();
    assert!(decoder.send(&same_size_blue, 3, 3).unwrap());
    color(
        &decoder.copy_rgba(256 * 144 * 4).unwrap().unwrap(),
        0,
        [0, 0, 255],
    );
    color(first.as_ref().unwrap(), 0, [255, 0, 0]);
    assert_eq!(CREATED.load(Ordering::Relaxed), 1);
    for format in [1, 2] {
        assert!(decoder
            .send(&red, 10 + format as i64, 10 + format as i64)
            .unwrap());
        color(
            &decoder.copy_pixels(256 * 144 * 3, format).unwrap().unwrap(),
            format,
            [255, 0, 0],
        );
    }
    assert!(decoder.send(&red, 20, 20).unwrap());
    FAIL_CREATE.store(true, Ordering::Relaxed);
    assert!(matches!(
        decoder.copy_rgba(256 * 144 * 4),
        Err(DecodeError::AllocationFailed)
    ));
    assert_eq!(decoder.peek().unwrap().unwrap().pts, 20);
    color(
        &decoder.copy_rgba(256 * 144 * 4).unwrap().unwrap(),
        0,
        [255, 0, 0],
    );
    assert!(decoder.send(&red, 21, 21).unwrap());
    FAIL_SCALE.store(true, Ordering::Relaxed);
    assert!(matches!(
        decoder.copy_rgba(256 * 144 * 4),
        Err(DecodeError::Runtime(-5))
    ));
    assert_eq!(decoder.peek().unwrap().unwrap().pts, 21);
    color(
        &decoder.copy_rgba(256 * 144 * 4).unwrap().unwrap(),
        0,
        [255, 0, 0],
    );
    assert!(decoder.send(&blue, 22, 22).unwrap());
    let frame = decoder.copy_rgba(128 * 96 * 4).unwrap().unwrap();
    assert_eq!(
        (frame.info.width, frame.info.height, frame.info.pts),
        (128, 96, 22)
    );
    color(&frame, 0, [0, 0, 255]);
    let ten_bit = std::fs::read(directory.join("alternate10.h264")).unwrap();
    assert!(decoder.send(&ten_bit, 23, 23).unwrap());
    let frame = decoder.copy_rgba(128 * 96 * 4).unwrap().unwrap();
    assert_eq!(
        (frame.info.width, frame.info.height, frame.info.pts),
        (128, 96, 23)
    );
    color(&frame, 0, [0, 0, 255]);
    color(&first.unwrap(), 0, [255, 0, 0]);
    drop(decoder);
    assert_eq!(CREATED.load(Ordering::Relaxed), 7);
    assert_eq!(
        CREATED.load(Ordering::Relaxed),
        FREED.load(Ordering::Relaxed)
    );
}
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
