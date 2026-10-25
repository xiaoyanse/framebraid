#![cfg_attr(not(feature = "shared-frames"), allow(unused_unsafe))]
use super::*;
use am_sdk_support::shared_call;
#[no_mangle]
pub unsafe extern "C" fn amc_shared_pool_publish(
    pool: *mut amc_shared_pool_t,
    frame: *const amc_frame_t,
    out: *mut u64,
) -> i32 {
    status(domain(), pool as usize, || {
        am_sdk_support::args::pointer(out)?;
        unsafe {
            out.write(0);
        }
        let body = super::frames::frame(frame as usize)?;
        let f = body
            .as_any()
            .downcast_ref::<super::frames::Frame>()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
        let pixel = am_pixel_view_t {
            magic: AM_PIXEL_VIEW_MAGIC,
            view: &*f.view,
            assoc: &*f.assoc,
            ..Default::default()
        };
        let id = unsafe { shared_call!(publish_view(domain(), pool as usize, &pixel)) }?;
        unsafe {
            out.write(id);
        }
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_shared_pool_create(
    ctx: *mut amc_context_t,
    cfg: *const am_shared_pool_config_t,
    attach: *mut am_shared_pool_attach_desc_t,
    out: *mut *mut amc_shared_pool_t,
) -> i32 {
    status(domain(), ctx as usize, || {
        am_sdk_support::args::pointer(out)?;
        unsafe {
            out.write(std::ptr::null_mut());
            let key: usize = shared_call!(create(domain(), ctx as usize, cfg, attach))?;
            out.write(key as _);
        }
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_shared_pool_publish_view(
    pool: *mut amc_shared_pool_t,
    view: *const am_pixel_view_t,
    out: *mut u64,
) -> i32 {
    status(domain(), pool as usize, || {
        am_sdk_support::args::pointer(out)?;
        unsafe {
            out.write(0);
            let id = shared_call!(publish_view(domain(), pool as usize, view))?;
            out.write(id);
        }
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_shared_pool_get_attach(
    pool: *const amc_shared_pool_t,
    out: *mut am_shared_pool_attach_desc_t,
) -> i32 {
    status(domain(), pool as usize, || unsafe {
        shared_call!(get_attach(domain(), pool as usize, out))
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_shared_pool_close(
    pool: *mut amc_shared_pool_t,
    wait: u32,
    out: *mut u32,
) -> i32 {
    status(domain(), pool as usize, || {
        am_sdk_support::args::pointer(out)?;
        let state = shared_call!(close(domain(), pool as usize, wait, true))?;
        unsafe {
            out.write(state);
        }
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_shared_pool_get_stats(
    pool: *const amc_shared_pool_t,
    out: *mut am_shared_pool_stats_t,
) -> i32 {
    status(domain(), pool as usize, || unsafe {
        shared_call!(stats(domain(), pool as usize, out))
    })
}
