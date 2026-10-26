use super::*;
use crate::{
    frames::ControlCharge,
    sources::{self, Source},
};
use am_sdk_support::buffers::Buffer;
use std::mem::size_of;

unsafe fn request(p: *const am_capture_request_t) -> Result<am_capture_request_t> {
    let value = unsafe { args::read(p, size_of::<am_capture_request_t>()) }?;
    kind(value.output_kind)?;
    deadline(value.deadline_ms)?;
    if value.reserved.iter().any(|n| *n != 0) {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    Ok(value)
}
#[no_mangle]
pub unsafe extern "C" fn amc_capture_sync(
    src: *mut amc_source_t,
    req: *const am_capture_request_t,
    out: *mut *mut am_captured_image_t,
) -> i32 {
    status(domain(), src as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        let req = request(req)?;
        let _active = domain().active(src as usize)?;
        let entry = sources::get(src as usize)?;
        let source = &entry.data::<Source>()?.shared;
        let state = state::State::get(&source.state)?;
        let _worker = state.reserve_worker()?;
        let until = Instant::now() + deadline(req.deadline_ms)?;
        let image =
            images::Image::capture(&state, source, req.output_kind, deadline(req.deadline_ms)?)?;
        if Instant::now() >= until {
            return Err(AM_ERR_SDK_DEADLINE_EXCEEDED);
        }
        out.write(images::transfer(image, &source.state, 0)?);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_capture_async(
    src: *mut amc_source_t,
    req: *const am_capture_request_t,
    out: *mut u64,
) -> i32 {
    status(domain(), src as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        out.write(0);
        let req = request(req)?;
        let active = domain().active(src as usize)?;
        let entry = sources::get(src as usize)?;
        let source = entry.data::<Source>()?.shared.clone();
        if !source.isapi {
            return Err(AM_ERR_CAMERA_MODE_CONFLICT);
        }
        let state = state::State::get(&source.state)?;
        let worker = state.reserve_worker()?;
        // 记录本体在表存储计费，原控制项目token仍保留。
        let charge = source.state.control(0, false)?;
        let id = {
            let mut records = lock(&state.records)?;
            state.purge(&mut records);
            let policy = state.result_policy()?;
            let limits = source.state.limits()?;
            if records.len() >= policy.max_items.min(limits.result_max_items) as usize {
                return Err(AM_ERR_SDK_QUEUE_FULL);
            }
            records.reserve(
                &source.state,
                1,
                policy.max_items.min(limits.result_max_items) as usize,
            )?;
            let id = state.id()?;
            records.insert(
                id,
                state::Record {
                    state: AM_REQ_PENDING as u32,
                    code: 0,
                    created: monotonic_ms(),
                    finished: 0,
                    expires: i64::MAX,
                    result: None,
                    _charge: charge,
                },
            );
            id
        };
        let state_copy = state.clone();
        if source
            .state
            .metrics
            .threads
            .clone()
            .spawn("amc-capture", move || {
                let _active = active;
                let _worker = worker;
                if let Ok(mut records) = state_copy.records.lock() {
                    match records.get_mut(&id) {
                        Some(r) if r.state < AM_REQ_DONE as u32 => r.state = AM_REQ_RUNNING as u32,
                        _ => return,
                    }
                }
                let result = images::Image::capture(
                    &state_copy,
                    &source,
                    req.output_kind,
                    deadline(req.deadline_ms).unwrap_or(Duration::from_secs(5)),
                );
                if let Ok(mut records) = state_copy.records.lock() {
                    let current_bytes: u64 = records
                        .values()
                        .filter_map(|r| r.result.as_ref())
                        .map(|r| r.bytes())
                        .sum();
                    if let Some(r) = records.get_mut(&id) {
                        if r.state < AM_REQ_DONE as u32 {
                            let policy = state_copy.result_policy().ok();
                            let budget = source.state.limits().map_or(0, |l| l.result_budget_bytes);
                            match result {
                                Ok(image)
                                    if policy.is_some_and(|p| {
                                        image.bytes()
                                            <= p.max_bytes.min(budget).saturating_sub(current_bytes)
                                    }) =>
                                {
                                    r.state = AM_REQ_DONE as u32;
                                    r.result = Some(image);
                                }
                                Ok(_) => {
                                    r.state = AM_REQ_FAILED as u32;
                                    r.code = AM_ERR_SDK_RESOURCE_EXHAUSTED;
                                }
                                Err(code) => {
                                    r.state = AM_REQ_FAILED as u32;
                                    r.code = code;
                                }
                            }
                            r.finished = monotonic_ms();
                            r.expires = r
                                .finished
                                .saturating_add(policy.map_or(0, |p| p.ttl_ms as i64));
                        }
                    }
                }
                state_copy.changed.notify_all();
            })
            .is_err()
        {
            lock(&state.records)?.remove(&id);
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        out.write(id);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_captured_image_fetch(
    ctx: *mut amc_context_t,
    id: u64,
    out: *mut *mut am_captured_image_t,
) -> i32 {
    status(domain(), ctx as usize, || unsafe {
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        let state = state::State::context(ctx as usize)?;
        let mut records = lock(&state.records)?;
        state.purge(&mut records);
        let r = records.get(&id).ok_or_else(|| state.unknown(id))?;
        if r.state < AM_REQ_DONE as u32 {
            return Err(AM_ERR_SDK_TRY_AGAIN);
        }
        if r.code != 0 {
            return Err(r.code);
        }
        out.write(images::transfer(
            r.result.as_ref().ok_or(AM_ERR_SDK_RESULT_EXPIRED)?.clone(),
            &crate::frames::camera_state(ctx as usize)?,
            id,
        )?);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_captured_image_ack(ctx: *mut amc_context_t, id: u64) -> i32 {
    status(domain(), ctx as usize, || {
        let state = state::State::context(ctx as usize)?;
        let mut records = lock(&state.records)?;
        state.purge(&mut records);
        let record = records.get(&id).ok_or_else(|| state.unknown(id))?;
        if record.state < AM_REQ_DONE as u32 {
            return Err(AM_ERR_SDK_TRY_AGAIN);
        }
        records.remove(&id);
        Ok(())
    })
}
struct Snapshot {
    _value: Box<am_request_snapshot_t>,
    _error: Box<am_error_info_t>,
    _charge: ControlCharge,
}
unsafe impl Send for Snapshot {}
unsafe impl Sync for Snapshot {}
impl Buffer for Snapshot {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
#[no_mangle]
pub unsafe extern "C" fn amc_request_query(
    ctx: *mut amc_context_t,
    id: u64,
    out: *mut *mut am_request_snapshot_t,
) -> i32 {
    status(domain(), ctx as usize, || unsafe {
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        let state = state::State::context(ctx as usize)?;
        let mut records = lock(&state.records)?;
        state.purge(&mut records);
        let r = records.get(&id).ok_or_else(|| state.unknown(id))?;
        let charge = crate::frames::camera_state(ctx as usize)?
            .control(size_of::<Snapshot>() + size_of::<am_error_info_t>(), true)?;
        let mut error = Box::<am_error_info_t>::default();
        args::write(
            &mut *error,
            am_error_info_t {
                code: r.code,
                message_utf8: status_name(r.code).unwrap_or(c"camera.error").as_ptr(),
                source_utf8: c"camera.isapi".as_ptr(),
                ..Default::default()
            },
        )?;
        let mut value = Box::<am_request_snapshot_t>::default();
        args::write(
            &mut *value,
            am_request_snapshot_t {
                state: r.state,
                terminal: u32::from(r.state >= AM_REQ_DONE as u32),
                error: &*error,
                created_at_ms: r.created,
                finished_at_ms: r.finished,
                ..Default::default()
            },
        )?;
        let key = (&mut *value as *mut am_request_snapshot_t) as usize;
        domain().buffers.insert(
            key,
            Arc::new(Snapshot {
                _value: value,
                _error: error,
                _charge: charge,
            }),
        )?;
        out.write(key as *mut _);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_request_snapshot_release(value: *mut am_request_snapshot_t) -> i32 {
    status(domain(), 0, || {
        if !domain()
            .buffers
            .get(value as usize)?
            .as_any()
            .is::<Snapshot>()
        {
            return Err(AM_ERR_SDK_HANDLE_INVALID);
        }
        domain().buffers.release(value as usize)
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_request_cancel(ctx: *mut amc_context_t, id: u64) -> i32 {
    status(domain(), ctx as usize, || {
        let state = state::State::context(ctx as usize)?;
        let mut records = lock(&state.records)?;
        state.purge(&mut records);
        let r = records.get_mut(&id).ok_or_else(|| state.unknown(id))?;
        if r.state < AM_REQ_DONE as u32 {
            r.state = AM_REQ_CANCELED as u32;
            r.finished = monotonic_ms();
            r.expires = r
                .finished
                .saturating_add(state.result_policy()?.ttl_ms as i64);
        }
        state.changed.notify_all();
        Ok(())
    })
}
