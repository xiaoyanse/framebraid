//! 合成单AU反复解码，计生产转换路径与上下文创建/释放，不接触现场设备。
use am_camera::media::{
    codec::{CpuDecoder, VideoCodec},
    ffmpeg_lib::FfmpegApi,
    ffmpeg_sys::SwsContext,
};
use std::{
    ffi::{c_int, c_void},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
    time::Instant,
};

type Create = unsafe extern "C" fn(
    c_int,
    c_int,
    c_int,
    c_int,
    c_int,
    c_int,
    c_int,
    *mut c_void,
    *mut c_void,
    *const f64,
) -> *mut SwsContext;
type Free = unsafe extern "C" fn(*mut SwsContext);
static ORIGINAL: OnceLock<(Create, Free)> = OnceLock::new();
static CREATED: AtomicU64 = AtomicU64::new(0);
static FREED: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" fn create(
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
    let Some(original) = ORIGINAL.get() else {
        return std::ptr::null_mut();
    };
    let pointer = unsafe { (original.0)(a, b, c, d, e, f, g, h, i, j) };
    if !pointer.is_null() {
        CREATED.fetch_add(1, Ordering::Relaxed);
    }
    pointer
}

unsafe extern "C" fn free(pointer: *mut SwsContext) {
    if let Some(original) = ORIGINAL.get() {
        if !pointer.is_null() {
            FREED.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { (original.1)(pointer) };
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("usage: perf_decode runtime_dir single_au.h264 samples warmup".into());
    }
    let samples: usize = args[3].parse()?;
    let warmup: usize = args[4].parse()?;
    if !(1..=100_000).contains(&samples) || warmup > 10_000 {
        return Err("sample budget exceeded".into());
    }
    let mut api = FfmpegApi::load_from(Path::new(&args[1]))?;
    ORIGINAL
        .set((api.sws_get_context, api.sws_free_context))
        .map_err(|_| "already initialized")?;
    api.sws_get_context = create;
    api.sws_free_context = free;
    let packet = std::fs::read(&args[2])?;
    if packet.len() > 16 * 1024 * 1024 {
        return Err("packet budget exceeded".into());
    }
    let mut decoder = CpuDecoder::create(Arc::new(api), VideoCodec::H264, &[], 8192 * 8192)
        .map_err(|e| format!("create: {e:?}"))?;
    let mut durations = Vec::with_capacity(samples);
    let mut decode_durations = Vec::with_capacity(samples);
    let mut expected = None;
    let mut dimensions = (0, 0);
    for index in 0..(samples + warmup) {
        let start = Instant::now();
        if !decoder
            .send(&packet, index as i64, index as i64)
            .map_err(|e| format!("send: {e:?}"))?
        {
            return Err("unexpected backpressure for single AU".into());
        }
        let info = decoder
            .peek()
            .map_err(|e| format!("peek: {e:?}"))?
            .ok_or("single AU did not produce a frame")?;
        let decode_ns = start.elapsed().as_nanos();
        dimensions = (info.width, info.height);
        let start = Instant::now();
        let frame = decoder
            .copy_rgba(8192 * 8192 * 4)
            .map_err(|e| format!("convert: {e:?}"))?
            .ok_or("missing frame")?;
        let elapsed = start.elapsed().as_nanos();
        let checksum = frame.data.iter().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        if expected.is_some_and(|v| v != checksum) {
            return Err("pixel checksum changed".into());
        }
        expected = Some(checksum);
        if index >= warmup {
            durations.push(elapsed);
            decode_durations.push(decode_ns);
        }
    }
    drop(decoder);
    println!("{{\"samples_ns\":{durations:?},\"decode_samples_ns\":{decode_durations:?},\"width\":{},\"height\":{},\"checksum_fnv1a64\":\"{:016x}\",\"contexts_created\":{},\"contexts_freed\":{},\"total_frames_with_warmup\":{}}}",
        dimensions.0, dimensions.1, expected.ok_or("no output")?, CREATED.load(Ordering::Relaxed), FREED.load(Ordering::Relaxed), samples + warmup);
    Ok(())
}
