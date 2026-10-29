//! 每源一条读流链，每订阅独立有界队列；推模式回调不占采集线程。
use crate::{
    domain,
    frames::{CameraState, Charge, Frame},
    *,
};
use am_sdk_support::{args, handles::Entry, Result};
use std::{
    collections::VecDeque,
    ffi::c_void,
    mem::size_of,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
        Arc, Condvar, Mutex, MutexGuard, Weak,
    },
    time::{Duration, Instant},
};
mod hub;
mod pixels;
pub(crate) mod queue;
pub use hub::Hub;
use queue::{Callback, Item, Kind, Subscription};
pub(crate) fn lock<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    m.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)
}
pub fn start(shared: &Arc<crate::sources::SharedSource>) -> Result<()> {
    Hub::start(shared)
}

fn subscription(key: usize, encoded: bool) -> Result<Arc<Entry>> {
    let entry = domain().get(key)?;
    let sub = entry.data::<Subscription>()?;
    if matches!(sub.kind, Kind::Encoded { .. }) != encoded {
        return Err(AM_ERR_SDK_HANDLE_INVALID);
    }
    Ok(entry)
}
fn subscribe(source: usize, kind: Kind, mode: u32, items: u32, bytes: u64) -> Result<usize> {
    am_sdk_support::callbacks::reject_reentry()?;
    let _active = domain().active(source)?;
    let entry = crate::sources::get(source)?;
    let shared = &entry.data::<crate::sources::Source>()?.shared;
    if shared.isapi {
        return Err(AM_ERR_CAMERA_MODE_CONFLICT);
    }
    // MJPEG 源无原始编码流输出语义；像素订阅由 hub 的流循环解码投递。
    if shared.mjpeg && matches!(kind, Kind::Encoded { .. }) {
        return Err(AM_ERR_CAMERA_CODEC_UNSUPPORTED);
    }
    start(shared)?;
    let hub = lock(&shared.stream)?
        .as_ref()
        .cloned()
        .ok_or(AM_ERR_CAMERA_PROBE_PENDING)?;
    if let Kind::Encoded { mask } = kind {
        if mask == 0 || mask & !7 != 0 {
            return Err(AM_ERR_CAMERA_CODEC_UNSUPPORTED);
        }
        let codec = hub.codec.load(Ordering::Acquire);
        if codec == 0 {
            shared.secret()?;
            return Err(AM_ERR_CAMERA_PROBE_PENDING);
        }
        if mask & codec == 0 {
            return Err(AM_ERR_CAMERA_CODEC_UNSUPPORTED);
        }
    }
    let sub = Subscription::new(shared.state.clone(), &hub, kind, mode, items, bytes)?;
    let mut decode = lock(&shared.decode)?;
    let mut subscribers = lock(&hub.subscribers)?;
    subscribers.prune();
    if subscribers.len() >= shared.state.limits()?.max_subscribers_per_source as usize
        || subscribers.is_full()
    {
        return Err(AM_ERR_CAMERA_SUBSCRIBER_LIMIT_REACHED);
    }
    let key = domain().insert(source, sub.clone())?;
    sub.key.store(key, Ordering::Release);
    subscribers.push_reserved(Arc::downgrade(&sub));
    if matches!(sub.kind, Kind::Frames { .. }) {
        decode.locked = true;
    }
    hub.changed.notify_all();
    Ok(key)
}
#[no_mangle]
pub unsafe extern "C" fn amc_subscribe_encoded(
    src: *mut amc_source_t,
    desc: *const am_encoded_sub_desc_t,
    out: *mut *mut amc_encoded_sub_t,
) -> i32 {
    status(domain(), src as usize, || {
        args::pointer(out)?;
        unsafe {
            out.write(std::ptr::null_mut());
        }
        let d = unsafe { args::read(desc, size_of::<am_encoded_sub_desc_t>()) }?;
        let key = subscribe(
            src as usize,
            Kind::Encoded { mask: d.codec_mask },
            d.mode,
            d.queue_max_items,
            d.queue_max_bytes,
        )?;
        unsafe {
            out.write(key as _);
        }
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_subscribe_frames(
    src: *mut amc_source_t,
    desc: *const am_frame_sub_desc_t,
    out: *mut *mut amc_frame_sub_t,
) -> i32 {
    status(domain(), src as usize, || {
        args::pointer(out)?;
        unsafe {
            out.write(std::ptr::null_mut());
        }
        let d = unsafe { args::read(desc, size_of::<am_frame_sub_desc_t>()) }?;
        if d.pixel_format > AM_PF_NV12 as u32 {
            return Err(AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED);
        }
        if d.preset_count as u64 > SDK_MAX_SCALED_FRAMES {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        if d.preset_count > 0 {
            args::pointer(d.presets)?;
        }
        let presets = if d.preset_count == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(d.presets, d.preset_count as usize) }
                .iter()
                .map(|p| {
                    crate::frames::scale::bytes(p.width, p.height, d.pixel_format)?;
                    Ok((p.width, p.height))
                })
                .collect::<Result<Vec<_>>>()?
        };
        let key = subscribe(
            src as usize,
            Kind::Frames {
                format: d.pixel_format,
                fps: d.sampling_max_fps,
                presets,
            },
            d.mode,
            d.queue_max_items,
            d.queue_max_bytes,
        )?;
        unsafe {
            out.write(key as _);
        }
        Ok(())
    })
}
fn unsubscribe(key: usize, encoded: bool, timeout: u32, out: *mut u32) -> Result<()> {
    am_sdk_support::callbacks::reject_reentry()?;
    args::pointer(out)?;
    subscription(key, encoded)?;
    let value = domain().close(key, timeout)?;
    unsafe {
        out.write(value);
    }
    Ok(())
}
#[no_mangle]
pub unsafe extern "C" fn amc_encoded_unsubscribe(
    sub: *mut amc_encoded_sub_t,
    timeout: u32,
    out: *mut u32,
) -> i32 {
    status(domain(), sub as usize, || {
        unsubscribe(sub as usize, true, timeout, out)
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_frame_unsubscribe(
    sub: *mut amc_frame_sub_t,
    timeout: u32,
    out: *mut u32,
) -> i32 {
    status(domain(), sub as usize, || {
        unsubscribe(sub as usize, false, timeout, out)
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_encoded_set_callback(
    sub: *mut amc_encoded_sub_t,
    cb: am_encoded_cb_fn,
    user: *mut c_void,
) -> i32 {
    status(domain(), sub as usize, || {
        let e = subscription(sub as usize, true)?;
        let s = e.data::<Subscription>()?;
        set_callback(s, cb.map(Callback::Encoded), user as usize)
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_frame_set_callback(
    sub: *mut amc_frame_sub_t,
    cb: am_frame_cb_fn,
    user: *mut c_void,
) -> i32 {
    status(domain(), sub as usize, || {
        let e = subscription(sub as usize, false)?;
        let s = e.data::<Subscription>()?;
        set_callback(s, cb.map(Callback::Frame), user as usize)
    })
}
fn set_callback(s: &Subscription, cb: Option<Callback>, user: usize) -> Result<()> {
    let hub = s.hub.upgrade().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
    let sub = lock(&hub.subscribers)?
        .iter()
        .filter_map(Weak::upgrade)
        .find(|a| std::ptr::eq(&**a, s))
        .ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
    sub.set_callback(cb, user)
}
fn pull(key: usize, encoded: bool, timeout: u32) -> Result<usize> {
    let _active = domain().active(key)?;
    let e = subscription(key, encoded)?;
    let sub = e.data::<Subscription>()?;
    sub.pull_export(timeout)
}
#[no_mangle]
pub unsafe extern "C" fn amc_encoded_pull(
    sub: *mut amc_encoded_sub_t,
    timeout: u32,
    out: *mut *mut am_encoded_packet_t,
) -> i32 {
    status(domain(), sub as usize, || {
        args::pointer(out)?;
        unsafe {
            out.write(std::ptr::null_mut());
        }
        let key = pull(sub as usize, true, timeout)?;
        unsafe {
            out.write(key as _);
        }
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_frame_pull(
    sub: *mut amc_frame_sub_t,
    timeout: u32,
    out: *mut *mut amc_frame_t,
) -> i32 {
    status(domain(), sub as usize, || {
        args::pointer(out)?;
        unsafe {
            out.write(std::ptr::null_mut());
        }
        let key = pull(sub as usize, false, timeout)?;
        unsafe {
            out.write(key as _);
        }
        Ok(())
    })
}
fn packet(key: usize) -> Result<()> {
    if domain()
        .buffers
        .get(key)?
        .as_any()
        .downcast_ref::<queue::Packet>()
        .is_none()
    {
        return Err(AM_ERR_SDK_HANDLE_INVALID);
    }
    Ok(())
}
#[no_mangle]
pub unsafe extern "C" fn amc_encoded_packet_retain(p: *mut am_encoded_packet_t) -> i32 {
    status(domain(), 0, || {
        packet(p as usize)?;
        domain().buffers.retain(p as usize)
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_encoded_packet_release(p: *mut am_encoded_packet_t) -> i32 {
    status(domain(), 0, || {
        packet(p as usize)?;
        domain().buffers.release(p as usize)
    })
}
