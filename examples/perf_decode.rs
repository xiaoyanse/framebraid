//! 合成单AU反复解码，计生产转换路径与上下文创建/释放，不接触现场设备。
use am_camera::media::{
    codec::{CpuDecoder, VideoCodec},
    ffmpeg_lib::FfmpegApi,
    ffmpeg_sys::SwsContext,
    hardware::{self, Backend},
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

#[path = "perf_decode/timing.rs"]
mod timing;
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
    if args.len() != 5 && args.len() != 7 {
        return Err("usage: perf_decode runtime_dir single_au.h264 samples warmup [d3d11va|dxva2 adapter_index]".into());
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
    timing::install(&mut api)?;
    let packet = std::fs::read(&args[2])?;
    if packet.len() > 16 * 1024 * 1024 {
        return Err("packet budget exceeded".into());
    }
    let selected = if args.len() == 7 {
        let backend = match args[5].as_str() {
            "d3d11va" => Backend::D3d11va,
            "dxva2" => Backend::Dxva2,
            _ => return Err("unsupported backend".into()),
        };
        let index: u32 = args[6].parse()?;
        let adapter = hardware::adapters()?
            .into_iter()
            .find(|a| a.index == index && !a.software)
            .ok_or("hardware adapter not found")?;
        Some((backend, adapter))
    } else {
        None
    };
    let api = Arc::new(api);
    let mut decoder = match &selected {
        Some((backend, adapter)) => {
            CpuDecoder::create_on_device(api, VideoCodec::H264, &[], 8192 * 8192, *backend, adapter)
        }
        None => CpuDecoder::create(api, VideoCodec::H264, &[], 8192 * 8192),
    }
    .map_err(|e| format!("create: {e:?}"))?;
    let clock_average_ns = (0..1000)
        .map(|_| {
            let start = Instant::now();
            start.elapsed().as_nanos()
        })
        .sum::<u128>()
        / 1000;
    let mut durations = Vec::with_capacity(samples);
    let mut decode_durations = Vec::with_capacity(samples);
    let mut transfer_durations = Vec::with_capacity(samples);
    let mut scale_durations = Vec::with_capacity(samples);
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
        timing::reset();
        let start = Instant::now();
        let frame = decoder
            .copy_rgba(8192 * 8192 * 4)
            .map_err(|e| format!("convert: {e:?}"))?
            .ok_or("missing frame")?;
        let elapsed = start.elapsed().as_nanos();
        let (transfer_ns, scale_ns) = timing::samples();
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
            transfer_durations.push(transfer_ns);
            scale_durations.push(scale_ns);
        }
    }
    if selected
        .as_ref()
        .is_some_and(|(backend, _)| decoder.hardware_backend() != Some(*backend))
    {
        return Err("requested hardware did not produce hardware frames".into());
    }
    let actual_backend = match decoder.hardware_backend() {
        Some(Backend::D3d11va) => "ffmpeg-d3d11va",
        Some(Backend::Dxva2) => "ffmpeg-dxva2",
        None => "ffmpeg-cpu",
    };
    let luid = selected.as_ref().map(|(_, adapter)| adapter.luid);
    drop(decoder);
    println!(
        "{}",
        serde_json::json!({"samples_ns":durations,"decode_samples_ns":decode_durations,"transfer_samples_ns":transfer_durations,"scale_samples_ns":scale_durations,"clock_average_ns":clock_average_ns,"width":dimensions.0,"height":dimensions.1,"checksum_fnv1a64":format!("{:016x}",expected.ok_or("no output")?),"contexts_created":CREATED.load(Ordering::Relaxed),"contexts_freed":FREED.load(Ordering::Relaxed),"total_frames_with_warmup":samples+warmup,"actual_backend":actual_backend,"adapter_luid":luid})
    );
    Ok(())
}
