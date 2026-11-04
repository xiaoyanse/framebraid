#![allow(clippy::missing_safety_doc)]
use super::{
    cascade, input, lock,
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
/// 开放词汇词表绑定（A1/A2，§4.1）：tokenization 归调用方；文本塔前向在新嵌入
/// 计算成功后原子换代（失败保持原绑定）。锁序固定 encoder→detector（detector
/// 短锁仅取档案维度，无反向持锁路径）。
pub unsafe extern "C" fn amv_vocab_bind(
    detector: *mut amv_model_t,
    text_encoder: *mut amv_model_t,
    entries: *const am_vocab_entry_t,
    entry_count: u32,
) -> i32 {
    use am_vision_core::profiles::Task;
    status(crate::domain(), detector as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        let detector_entry = crate::domain().get(detector as usize)?;
        let detector_handle = detector_entry.data::<ModelHandle>()?;
        if entries.is_null() || entry_count == 0 {
            if entries.is_null() && entry_count != 0 {
                return Err(AM_ERR_SDK_NULL_ARGUMENT);
            }
            // 解绑（entries=NULL 或 count=0）：清快照，此后双输入模型推理 vocabNotBound
            let mut slot = detector_handle
                .vocab
                .write()
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
            *slot = std::sync::Arc::new(None);
            return Ok(());
        }
        if entry_count as usize > am_vision_core::sdk::vocab::MAX_ENTRIES {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let encoder_entry = crate::domain().get(text_encoder as usize)?;
        let encoder_handle = encoder_entry.data::<ModelHandle>()?;
        // 条目读取与基础校验（struct_size/UTF-8 label/rows·cols 范围/cols 全表一致）
        let mut specs = Vec::with_capacity(entry_count as usize);
        let mut tokens: Vec<i64> = Vec::new();
        let mut cols: Option<usize> = None;
        for index in 0..entry_count as usize {
            let raw = args::read(entries.add(index), size_of::<am_vocab_entry_t>())?;
            if raw.token_rows == 0
                || raw.token_rows as usize > am_vision_core::sdk::vocab::MAX_PROMPTS_PER_ENTRY
                || raw.token_cols == 0
            {
                return Err(AM_ERR_SDK_INVALID_ARGUMENT);
            }
            args::pointer(raw.token_ids)?;
            let label = args::string(raw.label_utf8)?;
            let current = raw.token_cols as usize;
            if cols.is_some_and(|c| c != current) {
                return Err(AM_ERR_SDK_INVALID_ARGUMENT);
            }
            cols = Some(current);
            let count = raw.token_rows as usize * current;
            let slice = std::slice::from_raw_parts(raw.token_ids, count);
            tokens.extend_from_slice(slice);
            specs.push(am_vision_core::sdk::vocab::EntrySpec {
                label,
                token_rows: raw.token_rows,
            });
        }
        am_vision_core::sdk::vocab::validate(&specs).map_err(super::error)?;
        // detector 前置校验（短锁）：PROFILE 双输入 detect 档案；取第二输入嵌入维度
        let detector_dim = {
            let session = lock(&detector_handle.session)?;
            let model = session.as_ref().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
            if !model
                .profile
                .is_some_and(|p| p.task == Task::Detect && p.inputs.len() > 1)
            {
                return Err(AM_ERR_SDK_INVALID_ARGUMENT);
            }
            let dim = model.inputs[1].shape[1];
            if dim <= 0 {
                return Err(AM_ERR_VISION_PROFILE_UNSUPPORTED);
            }
            dim as usize
        };
        // 文本塔编码（长锁，失败原绑定保持）
        let snapshot = {
            let mut session = lock(&encoder_handle.session)?;
            let encoder = session.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
            am_vision_core::sdk::vocab::encode(
                encoder,
                &specs,
                &tokens,
                cols.unwrap_or(0),
                Some(detector_dim),
            )
            .map_err(super::error)?
        };
        let mut slot = detector_handle
            .vocab
            .write()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        *slot = std::sync::Arc::new(Some(snapshot));
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
#[no_mangle]
/// ROI 级联推理（C1，§4.1）异步入口：两级独立句柄均须 PROFILE detect 档案；
/// 请求解析校验见 cascade::parse；执行锁序与失败语义见 run_cascade。
pub unsafe extern "C" fn amv_infer_cascade_async(
    primary: *mut amv_model_t,
    secondary: *mut amv_model_t,
    p: *const am_image_input_t,
    cascade_request: *const am_cascade_request_t,
    params: *const am_infer_params_t,
    out: *mut u64,
) -> i32 {
    status(crate::domain(), primary as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        out.write(0);
        let entry = crate::domain().active(primary as usize)?;
        let model = entry.data::<ModelHandle>()?;
        let secondary_entry = crate::domain().active(secondary as usize)?;
        let secondary_model = secondary_entry.data::<ModelHandle>()?;
        let plan = cascade::plan(
            model,
            &secondary_entry.0,
            secondary_model,
            p,
            cascade_request,
            params,
            true,
        )?;
        out.write(scheduler::submit(entry.0.clone(), plan)?);
        Ok(())
    })
}
#[no_mangle]
/// ROI 级联推理（C1，§4.1）同步入口：submit_sync 同款截止语义。
pub unsafe extern "C" fn amv_infer_cascade_sync(
    primary: *mut amv_model_t,
    secondary: *mut amv_model_t,
    p: *const am_image_input_t,
    cascade_request: *const am_cascade_request_t,
    params: *const am_infer_params_t,
    out: *mut *mut amv_result_t,
    out_id: *mut u64,
) -> i32 {
    status(crate::domain(), primary as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        if !out_id.is_null() {
            args::pointer(out_id)?;
            out_id.write(0);
        }
        let entry = crate::domain().active(primary as usize)?;
        let model = entry.data::<ModelHandle>()?;
        let secondary_entry = crate::domain().active(secondary as usize)?;
        let secondary_model = secondary_entry.data::<ModelHandle>()?;
        let plan = cascade::plan(
            model,
            &secondary_entry.0,
            secondary_model,
            p,
            cascade_request,
            params,
            false,
        )?;
        submit_sync(entry.0.clone(), plan, out, out_id)
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
