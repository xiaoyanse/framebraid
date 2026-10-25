#![allow(clippy::missing_safety_doc)]
use super::{
    input, lock,
    models::{self, ModelHandle},
    results::{self, ResultBuffer, Snapshot},
    scheduler,
    state::State,
    submission::Plan,
};
use am_sdk_support::{abi::*, args, status, Result};
use am_vision_core::sdk::ModelSource;
use std::{
    mem::size_of,
    path::Path,
    time::{Duration, Instant},
};

#[no_mangle]
pub unsafe extern "C" fn amv_model_load(
    ctx: *mut amv_context_t,
    p: *const am_model_desc_t,
    out: *mut *mut amv_model_t,
) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        let p = args::read(p, size_of::<am_model_desc_t>())?;
        let path = args::string(p.path_utf8)?;
        let key = models::load(
            ctx as usize,
            p.load_mode,
            p.profile_id,
            p.backend,
            p.intra_threads,
            ModelSource::Path(Path::new(&path)),
        )?;
        out.write(key as _);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_model_load_memory(
    ctx: *mut amv_context_t,
    p: *const am_model_memory_desc_t,
    out: *mut *mut amv_model_t,
) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        let p = args::read(p, size_of::<am_model_memory_desc_t>())?;
        if p.size == 0 || p.size > SDK_MAX_MEMORY_MODEL_BYTES {
            return Err(AM_ERR_VISION_MODEL_TOO_LARGE);
        }
        args::pointer(p.bytes)?;
        let bytes = std::slice::from_raw_parts(p.bytes, p.size as usize);
        let key = models::load(
            ctx as usize,
            p.load_mode,
            p.profile_id,
            p.backend,
            0,
            ModelSource::Memory(bytes),
        )?;
        out.write(key as _);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_model_get_info(
    model: *const amv_model_t,
    out: *mut am_model_info_t,
) -> i32 {
    status(crate::domain(), model as usize, || unsafe {
        let entry = crate::domain().get(model as usize)?;
        args::write(out, entry.data::<ModelHandle>()?.info.value())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_model_close(
    model: *mut amv_model_t,
    timeout: u32,
    out: *mut am_lifecycle_state_t,
) -> i32 {
    status(crate::domain(), model as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        let entry = crate::domain().get(model as usize)?;
        entry.data::<ModelHandle>()?;
        out.write(crate::domain().close(model as usize, timeout)?);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_infer_image_async(
    model: *mut amv_model_t,
    p: *const am_image_input_t,
    params: *const am_infer_params_t,
    out: *mut u64,
) -> i32 {
    status(crate::domain(), model as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        out.write(0);
        let entry = crate::domain().active(model as usize)?;
        let plan = input::plan(entry.data::<ModelHandle>()?, p, params, true)?;
        out.write(scheduler::submit(
            entry.0.clone(),
            Plan::Image(Box::new(plan)),
        )?);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_infer_image_sync(
    model: *mut amv_model_t,
    p: *const am_image_input_t,
    params: *const am_infer_params_t,
    out: *mut *mut amv_result_t,
    out_id: *mut u64,
) -> i32 {
    status(crate::domain(), model as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        if !out_id.is_null() {
            args::pointer(out_id)?;
            out_id.write(0);
        }
        let entry = crate::domain().active(model as usize)?;
        let model = entry.data::<ModelHandle>()?;
        let plan = input::plan(model, p, params, false)?;
        submit_sync(entry.0.clone(), Plan::Image(Box::new(plan)), out, out_id)
    })
}
pub(super) unsafe fn submit_sync(
    entry: std::sync::Arc<am_sdk_support::handles::Entry>,
    plan: Plan,
    out: *mut *mut amv_result_t,
    out_id: *mut u64,
) -> Result<()> {
    let params = plan.params();
    let deadline = Instant::now()
        + Duration::from_millis(if params.sync_deadline_ms == 0 {
            SDK_INFER_TIMEOUT_MS
        } else {
            params.sync_deadline_ms as u64
        });
    let model = entry.data::<ModelHandle>()?;
    let id = unsafe { scheduler::submit(entry.clone(), plan) }?;
    if !out_id.is_null() {
        unsafe { out_id.write(id) };
    }
    let state = &model.state;
    let mut records = lock(&state.records)?;
    loop {
        let record = records.get(&id).ok_or_else(|| state.unknown(id))?;
        if record.state >= AM_REQ_DONE as u32 {
            if record.state == AM_REQ_DONE as u32 {
                let result = record
                    .result
                    .as_ref()
                    .ok_or(AM_ERR_SDK_RESULT_EXPIRED)?
                    .transfer()?;
                unsafe { out.write(result) };
                return Ok(());
            }
            return Err(if record.code != 0 {
                record.code
            } else {
                AM_ERR_VISION_REQUEST_STATE_INVALID
            });
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AM_ERR_SDK_DEADLINE_EXCEEDED);
        }
        records = state
            .changed
            .wait_timeout(records, remaining)
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
            .0;
    }
}
#[no_mangle]
pub unsafe extern "C" fn amv_result_fetch(
    ctx: *mut amv_context_t,
    id: u64,
    out: *mut *mut amv_result_t,
) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        let state = State::get(ctx as usize)?;
        let mut records = lock(&state.records)?;
        state.purge(&mut records);
        let r = records.get(&id).ok_or_else(|| state.unknown(id))?;
        if r.state < AM_REQ_DONE as u32 {
            return Err(AM_ERR_SDK_TRY_AGAIN);
        }
        if r.state != AM_REQ_DONE as u32 {
            return Err(if r.code != 0 {
                r.code
            } else {
                AM_ERR_VISION_REQUEST_STATE_INVALID
            });
        }
        out.write(
            r.result
                .as_ref()
                .ok_or(AM_ERR_SDK_RESULT_EXPIRED)?
                .transfer()?,
        );
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_result_ack(ctx: *mut amv_context_t, id: u64) -> i32 {
    status(crate::domain(), ctx as usize, || {
        let state = State::get(ctx as usize)?;
        let mut records = lock(&state.records)?;
        state.purge(&mut records);
        let r = records.get(&id).ok_or_else(|| state.unknown(id))?;
        if r.state < AM_REQ_DONE as u32 {
            return Err(AM_ERR_VISION_REQUEST_STATE_INVALID);
        }
        records.remove(&id);
        Ok(())
    })
}
fn buffer<T: am_sdk_support::buffers::Buffer>(key: usize, retain: bool) -> Result<()> {
    let value = crate::domain().buffers.get(key)?;
    if !value.as_any().is::<T>() {
        return Err(AM_ERR_SDK_HANDLE_INVALID);
    }
    if retain {
        crate::domain().buffers.retain(key)
    } else {
        crate::domain().buffers.release(key)
    }
}
#[no_mangle]
pub unsafe extern "C" fn amv_result_retain(result: *mut amv_result_t) -> i32 {
    status(crate::domain(), 0, || {
        let body = crate::domain().buffers.get(result as usize)?;
        body.as_any()
            .downcast_ref::<ResultBuffer>()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)?
            .change_reference(true)
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_result_release(result: *mut amv_result_t) -> i32 {
    status(crate::domain(), 0, || {
        let body = crate::domain().buffers.get(result as usize)?;
        body.as_any()
            .downcast_ref::<ResultBuffer>()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)?
            .change_reference(false)
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_request_query(
    ctx: *mut amv_context_t,
    id: u64,
    out: *mut *mut am_request_snapshot_t,
) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        let state = State::get(ctx as usize)?;
        out.write(results::snapshot(&state, id)?);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_request_snapshot_release(snapshot: *mut am_request_snapshot_t) -> i32 {
    status(crate::domain(), 0, || {
        buffer::<Snapshot>(snapshot as usize, false)
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_request_cancel(ctx: *mut amv_context_t, id: u64) -> i32 {
    status(crate::domain(), ctx as usize, || {
        let state = State::get(ctx as usize)?;
        let mut records = lock(&state.records)?;
        state.purge(&mut records);
        let r = records.get_mut(&id).ok_or_else(|| state.unknown(id))?;
        if r.state < AM_REQ_DONE as u32 {
            r.canceled = true;
        }
        drop(records);
        scheduler::wake();
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_set_realtime_policy(
    model: *mut amv_model_t,
    p: *const am_realtime_policy_t,
) -> i32 {
    status(crate::domain(), model as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        let p = args::read(p, size_of::<am_realtime_policy_t>())?;
        if p.max_inflight > 16
            || p.slot_count > 16
            || p.replace_oldest > 1
            || p.fair_round_robin > 1
        {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        let entry = crate::domain().active(model as usize)?;
        *lock(&entry.data::<ModelHandle>()?.policy)? = p;
        Ok(())
    })
}
