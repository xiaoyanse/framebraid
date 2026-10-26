//! 本地合成选择验收：CPU、自动选择、配置缺失、瞬态失败和共享探测忙。
use am_camera::media::{
    codec::{
        selection::{HardwarePolicy, SelectionFailure},
        CpuDecoder, VideoCodec,
    },
    ffmpeg_lib::FfmpegApi,
    ffmpeg_sys::{AVBufferRef, AVCodecHWConfig},
    hardware::{self, ProbeError},
};
use std::{
    ffi::{c_char, c_void},
    path::Path,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Condvar, Mutex,
    },
    time::Duration,
};
static CREATES: AtomicU32 = AtomicU32::new(0);
static GATE: (Mutex<(bool, bool)>, Condvar) = (Mutex::new((false, false)), Condvar::new());

unsafe extern "C" fn fail_create(
    out: *mut *mut AVBufferRef,
    _: i32,
    _: *const c_char,
    _: *mut c_void,
    _: i32,
) -> i32 {
    CREATES.fetch_add(1, Ordering::Relaxed);
    if !out.is_null() {
        unsafe { out.write(std::ptr::null_mut()) };
    }
    if let Ok(mut gate) = GATE.0.lock() {
        if gate.0 {
            gate.1 = true;
            GATE.1.notify_all();
            drop(GATE.1.wait_while(gate, |g| g.0));
        }
    }
    -12
}
unsafe extern "C" fn no_config(_: *const c_void, _: i32) -> *const AVCodecHWConfig {
    std::ptr::null()
}
unsafe extern "C" fn no_device_types(_: i32) -> i32 {
    0
}
fn pixels(mut decoder: CpuDecoder, packet: &[u8]) -> Result<(u64, bool), String> {
    if decoder.hardware_backend().is_some() {
        return Err("hardware claimed before output".into());
    }
    if !decoder
        .send(packet, 17, 17)
        .map_err(|e| format!("send: {e:?}"))?
    {
        return Err("unexpected first-packet backpressure".into());
    }
    let info = decoder
        .peek()
        .map_err(|e| format!("peek: {e:?}"))?
        .ok_or("missing first output")?;
    if info.pts != 17 || info.width != 1920 || info.height != 1080 {
        return Err("wrong output identity".into());
    }
    let frame = decoder
        .copy_rgba(info.rgba_bytes)
        .map_err(|e| format!("pixels: {e:?}"))?
        .ok_or("missing pixels")?;
    let hash = frame.data.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    Ok((hash, decoder.hardware_backend().is_some()))
}
fn selected(
    api: Arc<FfmpegApi>,
    packet: &[u8],
    policy: HardwarePolicy,
    budget: Duration,
) -> Result<(serde_json::Value, bool, u64), String> {
    let selection =
        CpuDecoder::create_with_policy(api, VideoCodec::H264, &[], 1920 * 1080, policy, budget)
            .map_err(|e| format!("select: {e:?}"))?;
    let report = serde_json::to_value(&selection.report).map_err(|e| e.to_string())?;
    let (hash, hardware) = pixels(selection.decoder, packet)?;
    Ok((report, hardware, hash))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: hardware_select runtime_dir single_au_1080p.h264".into());
    }
    let runtime = Path::new(&args[1]);
    let packet = std::fs::read(&args[2])?;
    if packet.len() > 16 * 1024 * 1024 {
        return Err("packet budget exceeded".into());
    }
    let mut injected = FfmpegApi::load_from(runtime)?;
    injected.av_hwdevice_ctx_create = fail_create;
    let injected = Arc::new(injected);
    let budget = Duration::from_secs(5);
    let (cpu, is_hw, reference) = selected(
        injected.clone(),
        &packet,
        HardwarePolicy::SoftwareOnly,
        Duration::ZERO,
    )?;
    if is_hw || CREATES.load(Ordering::Relaxed) != 0 {
        return Err("SoftwareOnly touched hardware".into());
    }
    let failure = CpuDecoder::create_with_policy(
        injected.clone(),
        VideoCodec::H264,
        &[],
        1920 * 1080,
        HardwarePolicy::PreferHardware,
        budget,
    )
    .map_err(|e| format!("failure selection: {e:?}"))?;
    if failure.report.selected.is_some() || failure.report.fallback.is_none() {
        return Err("initialization failure did not fall back".into());
    }
    if !failure.report.attempts.iter().any(|attempt| {
        matches!(
            attempt.error,
            Some(SelectionFailure::Inventory(ProbeError::DeviceInit(-12)))
        )
    }) {
        return Err("device initialization reason was lost".into());
    }
    let failure_report = serde_json::to_value(&failure.report)?;
    if pixels(failure.decoder, &packet)? != (reference, false) {
        return Err("failure fallback changed CPU output".into());
    }
    if CREATES.load(Ordering::Relaxed) > 16 {
        return Err("attempt budget exceeded".into());
    }
    let tiny = CpuDecoder::create_with_policy(
        injected.clone(),
        VideoCodec::H264,
        &[],
        1920 * 1080,
        HardwarePolicy::PreferHardware,
        Duration::from_nanos(1),
    )
    .map_err(|e| format!("tiny budget: {e:?}"))?;
    if !matches!(
        tiny.report.fallback,
        Some(SelectionFailure::Inventory(ProbeError::BudgetExhausted))
    ) {
        return Err("time budget not enforced".into());
    }
    let tiny_report = serde_json::to_value(&tiny.report)?;
    if pixels(tiny.decoder, &packet)? != (reference, false) {
        return Err("budget fallback changed CPU output".into());
    }
    if CpuDecoder::create_with_policy(
        injected.clone(),
        VideoCodec::H264,
        &[],
        1920 * 1080,
        HardwarePolicy::PreferHardware,
        Duration::ZERO,
    )
    .is_ok()
    {
        return Err("zero hardware budget accepted".into());
    }
    let mut absent = FfmpegApi::load_from(runtime)?;
    absent.avcodec_get_hw_config = no_config;
    absent.av_hwdevice_ctx_create = fail_create;
    let before = CREATES.load(Ordering::Relaxed);
    let (missing, is_hw, hash) = selected(
        Arc::new(absent),
        &packet,
        HardwarePolicy::PreferHardware,
        budget,
    )?;
    if is_hw || hash != reference || CREATES.load(Ordering::Relaxed) != before {
        return Err("missing codec config initialized a device".into());
    }
    let mut uncompiled = FfmpegApi::load_from(runtime)?;
    uncompiled.av_hwdevice_iterate_types = no_device_types;
    uncompiled.av_hwdevice_ctx_create = fail_create;
    let before = CREATES.load(Ordering::Relaxed);
    let (uncompiled, is_hw, hash) = selected(
        Arc::new(uncompiled),
        &packet,
        HardwarePolicy::PreferHardware,
        budget,
    )?;
    if is_hw || hash != reference || CREATES.load(Ordering::Relaxed) != before {
        return Err("uncompiled backend initialized a device".into());
    }
    if !uncompiled["attempts"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["error"]["Inventory"] == "NotCompiled")
    }) {
        return Err("uncompiled backend reason was lost".into());
    }
    {
        let mut gate = GATE.0.lock().map_err(|_| "gate poisoned")?;
        *gate = (true, false);
    }
    let probe = std::thread::spawn(move || hardware::probe(injected, budget));
    let busy_result = (|| -> Result<serde_json::Value, String> {
        let gate = GATE.0.lock().map_err(|_| "gate poisoned")?;
        let (gate, _) = GATE
            .1
            .wait_timeout_while(gate, Duration::from_secs(3), |g| !g.1)
            .map_err(|_| "wait poisoned")?;
        if !gate.1 {
            return Err("probe did not enter injected driver".into());
        }
        drop(gate);
        let selection = CpuDecoder::create_with_policy(
            Arc::new(FfmpegApi::load_from(runtime).map_err(|e| e.to_string())?),
            VideoCodec::H264,
            &[],
            1920 * 1080,
            HardwarePolicy::PreferHardware,
            budget,
        )
        .map_err(|e| format!("busy: {e:?}"))?;
        if !matches!(
            selection.report.fallback,
            Some(SelectionFailure::Inventory(ProbeError::Busy))
        ) {
            return Err("busy probe was not shared with selection".into());
        }
        let report = serde_json::to_value(&selection.report).map_err(|e| e.to_string())?;
        if pixels(selection.decoder, &packet)? != (reference, false) {
            return Err("busy fallback changed CPU output".into());
        }
        Ok(report)
    })();
    if let Ok(mut gate) = GATE.0.lock() {
        gate.0 = false;
        GATE.1.notify_all();
    }
    probe.join().map_err(|_| "probe panicked")??;
    let busy = busy_result?;
    // 先前注入失败不能成为永久负缓存；新请求重新初始化并验证真实输出。
    let (native, native_hw, _) = selected(
        Arc::new(FfmpegApi::load_from(runtime)?),
        &packet,
        HardwarePolicy::PreferHardware,
        budget,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"cpu":cpu,"initialization_failure":failure_report,"codec_config_missing":missing,"backend_not_compiled":uncompiled,"budget_exhausted":tiny_report,"probe_busy":busy,"native_retry":native,"native_stream_verified":native_hw,"cpu_checksum":format!("{reference:016x}")})
        )?
    );
    Ok(())
}
