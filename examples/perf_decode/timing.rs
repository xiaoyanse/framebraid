//! 固定开销的函数边界观测，区分GPU回读等待与颜色转换，不进入生产路径。
use am_camera::media::{
    ffmpeg_lib::FfmpegApi,
    ffmpeg_sys::{AVFrame, SwsContext},
};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        OnceLock,
    },
    time::Instant,
};
type Transfer = unsafe extern "C" fn(*mut AVFrame, *const AVFrame, i32) -> i32;
type Scale = unsafe extern "C" fn(
    *mut SwsContext,
    *const *const u8,
    *const i32,
    i32,
    i32,
    *const *mut u8,
    *const i32,
) -> i32;
static API: OnceLock<(Transfer, Scale)> = OnceLock::new();
static TRANSFER: AtomicU64 = AtomicU64::new(0);
static SCALE: AtomicU64 = AtomicU64::new(0);
pub fn install(api: &mut FfmpegApi) -> Result<(), &'static str> {
    API.set((api.av_hwframe_transfer_data, api.sws_scale))
        .map_err(|_| "timing already installed")?;
    api.av_hwframe_transfer_data = transfer;
    api.sws_scale = scale;
    Ok(())
}
pub fn reset() {
    TRANSFER.store(0, Ordering::Relaxed);
    SCALE.store(0, Ordering::Relaxed);
}
pub fn samples() -> (u64, u64) {
    (
        TRANSFER.load(Ordering::Relaxed),
        SCALE.load(Ordering::Relaxed),
    )
}
unsafe extern "C" fn transfer(dst: *mut AVFrame, src: *const AVFrame, flags: i32) -> i32 {
    let Some(api) = API.get() else {
        return -12;
    };
    let start = Instant::now();
    let rc = unsafe { (api.0)(dst, src, flags) };
    TRANSFER.store(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    rc
}
unsafe extern "C" fn scale(
    ctx: *mut SwsContext,
    src: *const *const u8,
    strides: *const i32,
    y: i32,
    h: i32,
    dst: *const *mut u8,
    out_stride: *const i32,
) -> i32 {
    let Some(api) = API.get() else {
        return -12;
    };
    let start = Instant::now();
    let rc = unsafe { (api.1)(ctx, src, strides, y, h, dst, out_stride) };
    SCALE.store(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    rc
}
