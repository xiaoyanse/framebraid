#![cfg_attr(not(feature = "shared-frames"), allow(unused_unsafe))]
use super::*;
use am_sdk_support::shared_call;
use std::ffi::c_void;
#[no_mangle]
pub unsafe extern "C" fn amv_shared_pool_attach(
    ctx: *mut amv_context_t,
    attach: *const am_shared_pool_attach_desc_t,
    out: *mut *mut amv_shared_pool_t,
) -> i32 {
    status(domain(), ctx as usize, || {
        am_sdk_support::args::pointer(out)?;
        unsafe {
            out.write(std::ptr::null_mut());
            let key: usize = shared_call!(attach(domain(), ctx as usize, attach))?;
            out.write(key as _);
        }
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_shared_pool_borrow(
    pool: *mut amv_shared_pool_t,
    id: u64,
    wait: u32,
    out: *mut am_shared_frame_view_t,
) -> i32 {
    status(domain(), pool as usize, || unsafe {
        shared_call!(borrow(domain(), pool as usize, id, wait, out))
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_shared_frame_as_pixel_view(
    pool: *mut amv_shared_pool_t,
    view: *const am_shared_frame_view_t,
    out: *mut am_pixel_view_t,
) -> i32 {
    status(domain(), pool as usize, || unsafe {
        shared_call!(pixel(
            domain(),
            pool as usize,
            view,
            out,
            amv_shared_frame_retain_bridge,
            amv_shared_frame_release_bridge
        ))
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_shared_frame_retain_bridge(owner: *mut c_void) {
    let _ = status(domain(), 0, || domain().buffers.retain(owner as usize));
}
#[no_mangle]
pub unsafe extern "C" fn amv_shared_frame_release_bridge(owner: *mut c_void) {
    let _ = status(domain(), 0, || domain().buffers.release(owner as usize));
}
#[no_mangle]
pub unsafe extern "C" fn amv_shared_pool_return(
    pool: *mut amv_shared_pool_t,
    view: *const am_shared_frame_view_t,
) -> i32 {
    status(domain(), pool as usize, || unsafe {
        shared_call!(return_view(domain(), pool as usize, view))
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_shared_pool_detach(
    pool: *mut amv_shared_pool_t,
    wait: u32,
    out: *mut u32,
) -> i32 {
    status(domain(), pool as usize, || {
        am_sdk_support::args::pointer(out)?;
        let state = shared_call!(close(domain(), pool as usize, wait, false))?;
        unsafe {
            out.write(state);
        }
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_shared_pool_get_stats(
    pool: *const amv_shared_pool_t,
    out: *mut am_shared_pool_stats_t,
) -> i32 {
    status(domain(), pool as usize, || unsafe {
        shared_call!(stats(domain(), pool as usize, out))
    })
}
