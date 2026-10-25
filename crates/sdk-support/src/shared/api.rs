#![allow(clippy::missing_safety_doc)]
use super::{
    layout::*,
    pool::{Lease, Pool},
    wire::Pair,
};
use crate::{
    abi::*,
    args,
    context::{shared_state, Context},
    handles::Domain,
    Result,
};
use std::{mem::size_of, sync::Arc};

fn enabled(domain: &Domain, ctx: &Context) -> Result<()> {
    if shared_state(domain, ctx.flags) != AM_SFP_ENABLED as u32 {
        return Err(AM_ERR_SDK_NOT_SUPPORTED);
    }
    Ok(())
}
pub unsafe fn create(
    domain: &Domain,
    key: usize,
    cfg: *const am_shared_pool_config_t,
    out: *mut am_shared_pool_attach_desc_t,
) -> Result<usize> {
    let entry = domain.active(key)?;
    let ctx = entry.data::<Context>()?;
    enabled(domain, ctx)?;
    // 先验证输出，失败不得遗留已登记但调用方不知道的池。
    unsafe { args::validate_output(out, size_of::<am_shared_pool_attach_desc_t>()) }?;
    let limits = *ctx.limits.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let config = if cfg.is_null() {
        configuration(&limits)
    } else {
        unsafe { args::read(cfg, size_of::<am_shared_pool_config_t>()) }?
    };
    validate_config(&config, &limits)?;
    let mut ledger = ctx.shared.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    ledger.check(config.total_bytes, &limits)?;
    let pair = ledger.admit(Arc::new(Pair::create(config)?), &limits)?;
    let desc = pair.attach;
    let pool = Arc::new(Pool::new(pair, true, ctx.shared_waiters.clone())?);
    let object = domain.insert(key, pool)?;
    unsafe { args::write(out, desc) }?;
    Ok(object)
}
pub unsafe fn attach(
    domain: &Domain,
    key: usize,
    desc: *const am_shared_pool_attach_desc_t,
) -> Result<usize> {
    let entry = domain.active(key)?;
    let ctx = entry.data::<Context>()?;
    enabled(domain, ctx)?;
    let desc = unsafe { args::read(desc, size_of::<am_shared_pool_attach_desc_t>()) }?;
    let limits = *ctx.limits.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let mut ledger = ctx.shared.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    // 即使本地已有映射也重新核验描述，不能靠tag绕过PID/代次检查。
    let pair = if let Some(pair) = ledger.existing(&desc.identity_tag) {
        let a = pair.attach;
        if desc.protocol_version != a.protocol_version {
            return Err(AM_ERR_SDK_ABI_UNSUPPORTED);
        }
        if desc.pool_id != a.pool_id
            || desc.pool_generation != a.pool_generation
            || desc.owner_pid != a.owner_pid
            || desc.owner_started_at_100ns != a.owner_started_at_100ns
        {
            return Err(AM_ERR_SDK_GENERATION_MISMATCH);
        }
        validate_config(&pair.config, &limits)?;
        pair
    } else {
        let opened = Pair::open(desc, &limits, |bytes| ledger.check(bytes, &limits))?;
        ledger.admit(Arc::new(opened), &limits)?
    };
    let pool = Arc::new(Pool::new(pair, false, ctx.shared_waiters.clone())?);
    match domain.insert(key, pool.clone()) {
        Ok(k) => Ok(k),
        Err(e) => {
            use crate::handles::Managed;
            pool.request_close();
            pool.drained();
            Err(e)
        }
    }
}
pub unsafe fn get_attach(
    domain: &Domain,
    key: usize,
    out: *mut am_shared_pool_attach_desc_t,
) -> Result<()> {
    let entry = domain.get(key)?;
    let pool = entry.data::<Pool>()?;
    if !pool.publisher {
        return Err(AM_ERR_SDK_HANDLE_INVALID);
    }
    unsafe { args::write(out, pool.pair()?.attach) }
}
pub unsafe fn stats(domain: &Domain, key: usize, out: *mut am_shared_pool_stats_t) -> Result<()> {
    let entry = domain.get(key)?;
    let pair = entry.data::<Pool>()?.pair()?;
    let s = pair.lock(0)?;
    s.reap()?;
    unsafe { args::write(out, s.stats()) }
}
pub fn close(domain: &Domain, key: usize, timeout: u32, publisher: bool) -> Result<u32> {
    {
        let entry = domain.get(key)?;
        if entry.data::<Pool>()?.publisher != publisher {
            return Err(AM_ERR_SDK_HANDLE_INVALID);
        }
    }
    domain.close(key, timeout)
}
pub unsafe fn publish_view(domain: &Domain, key: usize, p: *const am_pixel_view_t) -> Result<u64> {
    let entry = domain.active(key)?;
    let pool = entry.data::<Pool>()?;
    let pixel = unsafe { args::read(p, size_of::<am_pixel_view_t>()) }?;
    if pixel.magic != AM_PIXEL_VIEW_MAGIC {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let frame = unsafe { args::read(pixel.view, size_of::<am_frame_view_t>()) }?;
    let a = unsafe { args::read(pixel.assoc, size_of::<am_frame_assoc_t>()) }?;
    let mut desc = am_shared_frame_desc_t {
        protocol_version: 1,
        pixel_format: frame.format,
        width: frame.width,
        height: frame.height,
        plane_count: frame.plane_count,
        stride_bytes: frame.strides,
        color_matrix: frame.color_matrix,
        color_range: frame.color_range,
        frame_id: a.frame_id,
        source_id: a.source_id,
        source_generation: a.source_generation,
        config_generation: a.config_generation,
        assoc_valid: a.valid,
        pts: a.pts,
        time_base_num: a.time_base_num,
        time_base_den: a.time_base_den,
        captured_at_ms: a.captured_at_ms,
        received_at_ms: a.received_at_ms,
        time_flags: a.clock_mask,
        clock_quality: a.clock_quality,
        ..Default::default()
    };
    if frame.plane_count > 4 {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let mut offset = 0u64;
    for i in 0..frame.plane_count as usize {
        desc.offset_bytes[i] = u32::try_from(offset).map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        desc.plane_valid_bytes[i] =
            u32::try_from(frame.plane_size_bytes[i]).map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        offset = offset
            .checked_add(frame.plane_size_bytes[i])
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
    }
    for i in frame.plane_count as usize..4 {
        if !frame.planes[i].is_null() || frame.strides[i] != 0 || frame.plane_size_bytes[i] != 0 {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
    }
    pool.publish(desc, frame.planes)
}
pub unsafe fn borrow(
    domain: &Domain,
    key: usize,
    id: u64,
    wait: u32,
    out: *mut am_shared_frame_view_t,
) -> Result<()> {
    unsafe { args::validate_output(out, size_of::<am_shared_frame_view_t>()) }?;
    let entry = domain.active(key)?;
    let pool = entry.data::<Pool>()?;
    let lease = pool.borrow(id, wait)?;
    let pointer = Arc::as_ptr(&lease) as usize;
    let view = lease.view;
    let mut initial = pool.initial.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    domain.buffers.insert(pointer, lease)?;
    initial.insert(view.borrow_id, pointer);
    unsafe { args::write(out, view) }
}
fn find_lease(
    domain: &Domain,
    key: usize,
    view: &am_shared_frame_view_t,
) -> Result<(usize, Arc<dyn crate::buffers::Buffer>)> {
    let entry = domain.get(key)?;
    let pool = entry.data::<Pool>()?;
    if pool.publisher {
        return Err(AM_ERR_SDK_HANDLE_INVALID);
    }
    let pair = pool.pair()?;
    if view.desc.pool_generation != pair.attach.pool_generation {
        return Err(AM_ERR_SDK_GENERATION_MISMATCH);
    }
    let initial = pool.initial.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let pointer = *initial
        .get(&view.borrow_id)
        .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
    let body = domain.buffers.get(pointer)?;
    let lease = body
        .as_any()
        .downcast_ref::<Lease>()
        .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
    if encode(&lease.view.desc) != encode(&view.desc) || lease.view.plane_data != view.plane_data {
        return Err(AM_ERR_SDK_HANDLE_INVALID);
    }
    Ok((pointer, body))
}
pub unsafe fn return_view(
    domain: &Domain,
    key: usize,
    p: *const am_shared_frame_view_t,
) -> Result<()> {
    let view = unsafe { args::read(p, size_of::<am_shared_frame_view_t>()) }?;
    let (pointer, body) = find_lease(domain, key, &view)?;
    let entry = domain.get(key)?;
    let pool = entry.data::<Pool>()?;
    let removed = pool
        .initial
        .lock()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
        .remove(&view.borrow_id);
    if removed != Some(pointer) {
        return Err(AM_ERR_SDK_HANDLE_INVALID);
    }
    domain.buffers.release(pointer)?;
    drop(body);
    Ok(())
}
pub unsafe fn pixel(
    domain: &Domain,
    key: usize,
    p: *const am_shared_frame_view_t,
    out: *mut am_pixel_view_t,
    retain: unsafe extern "C" fn(*mut std::ffi::c_void),
    release: unsafe extern "C" fn(*mut std::ffi::c_void),
) -> Result<()> {
    let view = unsafe { args::read(p, size_of::<am_shared_frame_view_t>()) }?;
    let (pointer, body) = find_lease(domain, key, &view)?;
    let lease = body
        .as_any()
        .downcast_ref::<Lease>()
        .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
    let value = am_pixel_view_t {
        magic: AM_PIXEL_VIEW_MAGIC,
        owner_handle: pointer as _,
        retain_owner: Some(retain),
        release_owner: Some(release),
        view: &lease.frame,
        assoc: &lease.assoc,
        ..Default::default()
    };
    unsafe { args::write(out, value) }
}
