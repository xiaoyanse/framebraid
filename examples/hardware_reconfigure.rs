//! 合成码流验证参数重建复用设备；注入计数器只观察本进程运行库副本。
use am_camera::media::{
    codec::{selection::HardwarePolicy, CpuDecoder, DecodeError, VideoCodec},
    ffmpeg_lib::FfmpegApi,
    ffmpeg_sys::AVBufferRef,
};
use std::{
    ffi::{c_char, c_void},
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};
static API: OnceLock<Arc<FfmpegApi>> = OnceLock::new();
static CREATES: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn create_device(
    out: *mut *mut AVBufferRef,
    kind: i32,
    name: *const c_char,
    options: *mut c_void,
    flags: i32,
) -> i32 {
    CREATES.fetch_add(1, Ordering::Relaxed);
    let Some(api) = API.get() else { return -12 };
    unsafe { (api.av_hwdevice_ctx_create)(out, kind, name, options, flags) }
}
fn output(
    decoder: &mut CpuDecoder,
    packet: &[u8],
    pts: i64,
    hardware: bool,
) -> Result<Vec<u8>, String> {
    if !decoder
        .send(packet, pts, pts)
        .map_err(|e| format!("send {e:?}"))?
    {
        return Err("unexpected input backpressure".into());
    }
    let info = decoder
        .peek()
        .map_err(|e| format!("peek {e:?}"))?
        .ok_or("missing frame")?;
    if info.pts != pts || info.width != 1920 || info.height != 1080 {
        return Err("wrong frame identity".into());
    }
    if decoder.hardware_backend().is_some() != hardware {
        return Err("wrong actual backend".into());
    }
    Ok(decoder
        .copy_rgba(info.rgba_bytes)
        .map_err(|e| format!("copy {e:?}"))?
        .ok_or("missing pixels")?
        .data)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: hardware_reconfigure runtime_dir single_au_1080p.h264".into());
    }
    let runtime = Path::new(&args[1]);
    let packet = std::fs::read(&args[2])?;
    API.set(Arc::new(FfmpegApi::load_from(runtime)?))
        .map_err(|_| "duplicate API")?;
    let mut observed = FfmpegApi::load_from(runtime)?;
    observed.av_hwdevice_ctx_create = create_device;
    let selection = CpuDecoder::create_with_policy(
        Arc::new(observed),
        VideoCodec::H264,
        &[],
        1920 * 1080,
        HardwarePolicy::PreferHardware,
        Duration::from_secs(5),
    )
    .map_err(|e| format!("select {e:?}"))?;
    let selected = selection
        .report
        .selected
        .as_ref()
        .ok_or("test requires real hardware")?;
    let before = CREATES.load(Ordering::Relaxed);
    let mut old = selection.decoder;
    let first = output(&mut old, &packet, 1, true)?;
    // 失败重建不得破坏原上下文和它尚未交付的帧。
    if !old.send(&packet, 2, 2).map_err(|e| format!("send {e:?}"))? {
        return Err("old context unexpectedly blocked".into());
    }
    let pending = old
        .peek()
        .map_err(|e| format!("pending {e:?}"))?
        .ok_or("pending absent")?;
    if !matches!(
        old.reconfigure(VideoCodec::H264, &[], 0),
        Err(DecodeError::InvalidInput)
    ) {
        return Err("invalid rebuild accepted".into());
    }
    if old
        .peek()
        .map_err(|e| format!("retained {e:?}"))?
        .ok_or("pending lost")?
        .pts
        != 2
    {
        return Err("failed rebuild changed old frame".into());
    }
    let mut next = old
        .reconfigure(VideoCodec::H264, &[], 1920 * 1080)
        .map_err(|e| format!("reconfigure {e:?}"))?;
    if next.hardware_backend().is_some() {
        return Err("inherited old verification state".into());
    }
    let retained = old
        .copy_rgba(pending.rgba_bytes)
        .map_err(|e| format!("retained copy {e:?}"))?
        .ok_or("old frame absent")?
        .data;
    drop(old);
    let second = output(&mut next, &packet, 3, true)?;
    if first != retained || first != second {
        return Err("rebuild changed deterministic same-backend output".into());
    }
    let mut cpu = next
        .software_reconfigure(VideoCodec::H264, &[], 1920 * 1080)
        .map_err(|e| format!("software rebuild {e:?}"))?;
    drop(next);
    let cpu_pixels = output(&mut cpu, &packet, 4, false)?;
    // CPU重建仍应独立可用，且不经过设备发现。
    let mut cpu_next = cpu
        .reconfigure(VideoCodec::H264, &[], 1920 * 1080)
        .map_err(|e| format!("cpu rebuild {e:?}"))?;
    drop(cpu);
    if cpu_pixels != output(&mut cpu_next, &packet, 5, false)? {
        return Err("CPU rebuild changed output".into());
    }
    let after = CREATES.load(Ordering::Relaxed);
    if before == 0 || before != after {
        return Err("rebuild initialized another device".into());
    }
    let api = API.get().ok_or("API absent")?;
    let seed = CpuDecoder::create_on_device(
        api.clone(),
        VideoCodec::H264,
        &[],
        1920 * 1080,
        selected.backend,
        &selected.adapter,
    )
    .map_err(|e| format!("benchmark seed {e:?}"))?;
    let mut fresh_us = Vec::new();
    let mut reused_us = Vec::new();
    // 同进程交替测量，只计构造；不把初始化收益当每帧解码收益。
    for round in 0..12 {
        let start = Instant::now();
        let fresh = CpuDecoder::create_on_device(
            api.clone(),
            VideoCodec::H264,
            &[],
            1920 * 1080,
            selected.backend,
            &selected.adapter,
        )
        .map_err(|e| format!("fresh {e:?}"))?;
        let fresh_time = start.elapsed().as_micros();
        drop(fresh);
        let start = Instant::now();
        let reused = seed
            .reconfigure(VideoCodec::H264, &[], 1920 * 1080)
            .map_err(|e| format!("reused {e:?}"))?;
        let reused_time = start.elapsed().as_micros();
        drop(reused);
        if round >= 2 {
            fresh_us.push(fresh_time);
            reused_us.push(reused_time);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "selected": selected, "device_creates_before": before, "device_creates_after": after,
            "retained_pending_frame": true, "fresh_verification": true,
            "old_context_destroyed_before_new_output": true, "cpu_rebuild": true,
        "same_backend_pixels_equal": true,
        "construction_comparison": {"warmup": 2, "samples": 10,
            "fresh_device_us": fresh_us, "reuse_device_us": reused_us,
            "scope": "codec construction only; excludes send, decode, readback and output"},
        }))?
    );
    Ok(())
}
