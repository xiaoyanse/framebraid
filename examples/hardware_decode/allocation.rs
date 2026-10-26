//! 只跟踪经SDK显式分配的CPU目标帧，不把解码器内部/驱动缓冲计作托管缓冲。
use am_camera::media::{
    ffmpeg_lib::FfmpegApi,
    ffmpeg_sys::{AVBufferRef, AVFrame},
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex, OnceLock,
    },
};
type Get = unsafe extern "C" fn(*mut AVFrame, i32) -> i32;
type Unref = unsafe extern "C" fn(*mut AVFrame);
type Plane = unsafe extern "C" fn(*const AVFrame, i32) -> *mut AVBufferRef;
static API: OnceLock<(Get, Unref, Plane)> = OnceLock::new();
static LIVE: Mutex<Option<HashMap<usize, usize>>> = Mutex::new(None);
static LIMIT: AtomicUsize = AtomicUsize::new(0);
static CREATED: AtomicUsize = AtomicUsize::new(0);
static RELEASED: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static BAD: AtomicBool = AtomicBool::new(false);
pub fn install(api: &mut FfmpegApi) -> Result<(), &'static str> {
    API.set((
        api.av_frame_get_buffer,
        api.av_frame_unref,
        api.av_frame_get_plane_buffer,
    ))
    .map_err(|_| "allocation observer already installed")?;
    api.av_frame_get_buffer = get;
    api.av_frame_unref = unref;
    Ok(())
}
pub fn limit(bytes: usize) {
    LIMIT.store(bytes, Ordering::Relaxed);
}
pub fn created() -> usize {
    CREATED.load(Ordering::Relaxed)
}
pub fn summary() -> Result<serde_json::Value, String> {
    let live = LIVE
        .lock()
        .map_err(|_| "observer lock poisoned")?
        .as_ref()
        .map_or(0, HashMap::len);
    let (created, released) = (created(), RELEASED.load(Ordering::Relaxed));
    if live != 0 || created != released || BAD.load(Ordering::Relaxed) {
        return Err("readback buffers leaked or exceeded reservation".into());
    }
    Ok(
        serde_json::json!({"created":created,"released":released,"live":live,"largest_actual_buffer_bytes":PEAK.load(Ordering::Relaxed)}),
    )
}
unsafe extern "C" fn get(frame: *mut AVFrame, align: i32) -> i32 {
    let Some(api) = API.get() else {
        return -12;
    };
    let rc = unsafe { (api.0)(frame, align) };
    if rc >= 0 {
        if let Some(buffer) = unsafe { (api.2)(frame, 0).as_ref() } {
            CREATED.fetch_add(1, Ordering::Relaxed);
            PEAK.fetch_max(buffer.size, Ordering::Relaxed);
            if buffer.size > LIMIT.load(Ordering::Relaxed) {
                BAD.store(true, Ordering::Relaxed);
            }
            if let Ok(mut live) = LIVE.lock() {
                if live
                    .get_or_insert_with(HashMap::new)
                    .insert(frame as usize, buffer.size)
                    .is_some()
                {
                    BAD.store(true, Ordering::Relaxed);
                }
            } else {
                BAD.store(true, Ordering::Relaxed);
            }
        } else {
            BAD.store(true, Ordering::Relaxed);
        }
    }
    rc
}
unsafe extern "C" fn unref(frame: *mut AVFrame) {
    let Some(api) = API.get() else {
        return;
    };
    let tracked = LIVE.lock().ok().is_some_and(|mut live| {
        live.as_mut()
            .is_some_and(|m| m.remove(&(frame as usize)).is_some())
    });
    unsafe { (api.1)(frame) };
    if tracked {
        RELEASED.fetch_add(1, Ordering::Relaxed);
        if !unsafe { (api.2)(frame, 0) }.is_null() {
            BAD.store(true, Ordering::Relaxed);
        }
    }
}
