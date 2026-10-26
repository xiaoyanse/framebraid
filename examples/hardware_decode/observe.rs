//! 仅验收示例使用：在颜色转换前对拍Y/U/V，避免混淆不同swscale输入格式的插值差异。
use am_camera::media::{
    ffmpeg_lib::FfmpegApi,
    ffmpeg_sys::{AVCodecContext, AVFrame},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};

type Receive = unsafe extern "C" fn(*mut AVCodecContext, *mut AVFrame) -> i32;
type Transfer = unsafe extern "C" fn(*mut AVFrame, *const AVFrame, i32) -> i32;
static FUNCTIONS: OnceLock<(Receive, Transfer)> = OnceLock::new();
static PIXELS: Mutex<Option<Vec<u16>>> = Mutex::new(None);
static FAIL_NEXT: AtomicBool = AtomicBool::new(false);
static FAILED: AtomicBool = AtomicBool::new(false);

pub fn inject_transfer_failure() {
    FAIL_NEXT.store(true, Ordering::Relaxed);
}
pub fn injected_failure() -> bool {
    FAILED.swap(false, Ordering::Relaxed)
}

pub fn install(api: &mut FfmpegApi) -> Result<(), &'static str> {
    FUNCTIONS
        .set((api.avcodec_receive_frame, api.av_hwframe_transfer_data))
        .map_err(|_| "observer already installed")?;
    api.avcodec_receive_frame = receive;
    api.av_hwframe_transfer_data = transfer;
    Ok(())
}
pub fn take() -> Option<Vec<u16>> {
    PIXELS.lock().ok()?.take()
}
unsafe fn capture(frame: *const AVFrame) {
    if frame.is_null() {
        return;
    }
    let frame = unsafe { &*frame };
    // 只读取已回CPU的8/10-bit 4:2:0平面，不误读硬件表面。
    if ![0, 23, 62, 158].contains(&frame.format) || frame.width <= 0 || frame.height <= 0 {
        return;
    }
    let Ok(mut captured) = PIXELS.lock() else {
        return;
    };
    if captured.is_some() {
        return;
    }
    let (w, h) = (frame.width as usize, frame.height as usize);
    if w > 1920 || h > 1080 || w % 2 != 0 || h % 2 != 0 {
        return;
    }
    let mut output = Vec::with_capacity(w * h * 3 / 2);
    for plane in 0..3 {
        let (pw, ph) = if plane == 0 { (w, h) } else { (w / 2, h / 2) };
        let semiplanar = [23, 158].contains(&frame.format);
        let wide = [62, 158].contains(&frame.format);
        let source_plane = if semiplanar && plane > 0 { 1 } else { plane };
        let stride = frame.linesize[source_plane] as isize;
        let data = frame.data[source_plane];
        if data.is_null() {
            return;
        }
        for y in 0..ph {
            for x in 0..pw {
                let column = if semiplanar && plane > 0 {
                    x * 2 + plane - 1
                } else {
                    x
                };
                let column = column * if wide { 2 } else { 1 };
                let address = unsafe { data.offset(y as isize * stride + column as isize) };
                let value = if wide {
                    u16::from_le(unsafe { address.cast::<u16>().read_unaligned() })
                        >> if frame.format == 158 { 6 } else { 0 }
                } else {
                    u16::from(unsafe { *address })
                };
                output.push(value);
            }
        }
    }
    *captured = Some(output);
}
unsafe extern "C" fn receive(ctx: *mut AVCodecContext, frame: *mut AVFrame) -> i32 {
    let Some(functions) = FUNCTIONS.get() else {
        return -12;
    };
    let rc = unsafe { (functions.0)(ctx, frame) };
    if rc >= 0 {
        unsafe { capture(frame) };
    }
    rc
}
unsafe extern "C" fn transfer(dst: *mut AVFrame, src: *const AVFrame, flags: i32) -> i32 {
    if FAIL_NEXT.swap(false, Ordering::Relaxed) {
        FAILED.store(true, Ordering::Relaxed);
        return -5;
    }
    let Some(functions) = FUNCTIONS.get() else {
        return -12;
    };
    let rc = unsafe { (functions.1)(dst, src, flags) };
    if rc >= 0 {
        unsafe { capture(dst) };
    }
    rc
}
