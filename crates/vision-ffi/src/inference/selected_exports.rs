//! 新入口显式给允许列表；旧模型描述不变，互斥旧backend参数。
use super::{models, selection};
use am_sdk_support::{abi::*, args, status, Result};
use am_vision_core::sdk::ModelSource;
use std::{mem::size_of, path::Path};

unsafe fn outputs(model: *mut *mut amv_model_t, id: *mut u64) -> Result<()> {
    args::pointer(model)?;
    args::pointer(id)?;
    let a = model as usize;
    let b = id as usize;
    if a.abs_diff(b) < size_of::<u64>() {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    unsafe {
        model.write(std::ptr::null_mut());
        id.write(0);
    }
    Ok(())
}
#[no_mangle]
pub unsafe extern "C" fn amv_model_load_selected(
    ctx: *mut amv_context_t,
    p: *const am_model_desc_t,
    policy: *const am_backend_selection_t,
    out: *mut *mut amv_model_t,
    id: *mut u64,
) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        outputs(out, id)?;
        let p = args::read(p, size_of::<am_model_desc_t>())?;
        if !p.backend.is_null() {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let path = args::string(p.path_utf8)?;
        let policy = selection::policy(policy)?;
        let active = crate::domain().active(ctx as usize)?;
        let attempt = selection::Attempt::begin(ctx as usize, &policy)?;
        id.write(attempt.id);
        let result = models::load_selected(
            ctx as usize,
            p.load_mode,
            p.profile_id,
            p.intra_threads,
            ModelSource::Path(Path::new(&path)),
            &policy,
            &attempt,
        );
        finish(&attempt, &result)?;
        out.write(result? as _);
        drop(active);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_model_load_memory_selected(
    ctx: *mut amv_context_t,
    p: *const am_model_memory_desc_t,
    policy: *const am_backend_selection_t,
    threads: u32,
    out: *mut *mut amv_model_t,
    id: *mut u64,
) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        outputs(out, id)?;
        let p = args::read(p, size_of::<am_model_memory_desc_t>())?;
        if !p.backend.is_null() {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        if p.size == 0 || p.size > SDK_MAX_MEMORY_MODEL_BYTES {
            return Err(AM_ERR_VISION_MODEL_TOO_LARGE);
        }
        args::pointer(p.bytes)?;
        let policy = selection::policy(policy)?;
        let active = crate::domain().active(ctx as usize)?;
        let attempt = selection::Attempt::begin(ctx as usize, &policy)?;
        id.write(attempt.id);
        let bytes = std::slice::from_raw_parts(p.bytes, p.size as usize);
        let result = models::load_selected(
            ctx as usize,
            p.load_mode,
            p.profile_id,
            threads,
            ModelSource::Memory(bytes),
            &policy,
            &attempt,
        );
        finish(&attempt, &result)?;
        out.write(result? as _);
        drop(active);
        Ok(())
    })
}

fn finish(attempt: &selection::Attempt, result: &Result<usize>) -> Result<()> {
    if let Err(code) = attempt.finish(result.as_ref().err().copied().unwrap_or(AM_OK as i32)) {
        // 成功注册后诊断发布失败也不能遗留调用方拿不到的活动模型。
        if let Ok(key) = result {
            if crate::domain().close(*key, 1000).is_ok() {
                let _ = crate::domain().destroy(*key);
            }
        }
        return Err(code);
    }
    Ok(())
}
