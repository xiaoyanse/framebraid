//! 显式设备探测示例；不解码、不连接相机，不将初始化成功标成真实码流验证。
use am_camera::media::{ffmpeg_lib::FfmpegApi, ffmpeg_sys::AVBufferRef, hardware};
use std::{
    ffi::{c_char, c_void},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};

type Create =
    unsafe extern "C" fn(*mut *mut AVBufferRef, i32, *const c_char, *mut c_void, i32) -> i32;
type Unref = unsafe extern "C" fn(*mut *mut AVBufferRef);
static FUNCTIONS: OnceLock<(Create, Unref)> = OnceLock::new();
static INJECT: AtomicBool = AtomicBool::new(false);
static CREATED: AtomicU32 = AtomicU32::new(0);
static RELEASED: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn create(
    out: *mut *mut AVBufferRef,
    kind: i32,
    name: *const c_char,
    opts: *mut c_void,
    flags: i32,
) -> i32 {
    let Some(functions) = FUNCTIONS.get() else {
        return -12;
    };
    let code = unsafe { (functions.0)(out, kind, name, opts, flags) };
    if !out.is_null() && !unsafe { *out }.is_null() {
        CREATED.fetch_add(1, Ordering::Relaxed);
        if INJECT.load(Ordering::Relaxed) {
            return -12;
        }
    }
    code
}
unsafe extern "C" fn unref(out: *mut *mut AVBufferRef) {
    if let Some(functions) = FUNCTIONS.get() {
        if !out.is_null() && !unsafe { *out }.is_null() {
            RELEASED.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { (functions.1)(out) };
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let path = args
        .get(1)
        .ok_or("usage: hardware_probe ffmpeg_runtime_dir [budget_ms]")?;
    let budget: u64 = args.get(2).map_or(Ok(1500), |s| s.parse())?;
    if args.len() > 4
        || args
            .get(3)
            .is_some_and(|flag| flag != "--inject-partial-failure")
    {
        return Err("unsupported probe arguments".into());
    }
    INJECT.store(args.get(3).is_some(), Ordering::Relaxed);
    let mut api = FfmpegApi::load_from(Path::new(path))?;
    FUNCTIONS
        .set((api.av_hwdevice_ctx_create, api.av_buffer_unref))
        .map_err(|_| "already initialized")?;
    api.av_hwdevice_ctx_create = create;
    api.av_buffer_unref = unref;
    let api = Arc::new(api);
    let report = hardware::probe(api, Duration::from_millis(budget))?;
    let (created, released) = (
        CREATED.load(Ordering::Relaxed),
        RELEASED.load(Ordering::Relaxed),
    );
    if created != released {
        return Err("hardware probe leaked an owned context reference".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
        "mode":if INJECT.load(Ordering::Relaxed) {"injected-partial-failure"}else{"native"},
        "created_refs":created,"released_refs":released,"report":report}))?
    );
    Ok(())
}
