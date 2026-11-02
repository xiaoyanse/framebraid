#![allow(clippy::missing_safety_doc)]
use super::{error, lock, state::State};
use am_sdk_support::{
    abi::*,
    args, common,
    context::{self, Context},
    handles::Domain,
    status, Result,
};
use am_vision_core::sdk::runtime;
use std::{path::Path, sync::Mutex};

static INITIALIZATION: Mutex<Option<i32>> = Mutex::new(None);
pub struct Rows(pub Box<[am_backend_row_t; 4]>);
impl Default for Rows {
    fn default() -> Self {
        Self(Box::new([am_backend_row_t::default(); 4]))
    }
}
// note仅指向静态字符串；同ctx下一次查询是公布的有效期终点。
unsafe impl Send for Rows {}

unsafe fn initialize_inner(
    domain: &Domain,
    params: *const amv_init_params_t,
    retry: bool,
    failed: &mut Option<i32>,
) -> Result<usize> {
    let previous = *lock(&domain.thread_config)?;
    let key = unsafe { context::initialize_vision(domain, params) }?;
    let result = (|| {
        let entry = domain.get(key)?;
        let c = entry.data::<Context>()?;
        // E1：OpenVINO 原生引擎运行库按合同随 runtime_dirs 提供；定位不到不报错
        //（可选候选，装载期回退）。环境变量 AM_SDK_OPENVINO_DIR 仍可显式覆盖。
        runtime::note_openvino_dirs(&c.runtime_dirs);
        if !retry && c.runtime_dirs.is_empty() && c.flags & AM_INIT_SHARED_FRAMES as u64 != 0 {
            return Ok(());
        }
        if !retry {
            if let Some(code) = failed {
                return Err(*code);
            }
        }
        let path = c
            .runtime_dirs
            .iter()
            .map(|d| Path::new(d).join(runtime::library_name()))
            .find(|p| p.is_file())
            .ok_or(AM_ERR_VISION_RUNTIME_MISSING)?;
        runtime::initialize(&path).map_err(error)?;
        *failed = None;
        Ok(())
    })();
    if let Err(code) = result {
        if !runtime::initialized() {
            *failed = Some(code);
        }
        // 失败路径完整走收尾：close 与 destroy 都执行，`?` 短路会让注册槽滞留
        // （审计 A10）；destroy 对未关闭对象按合同拒绝，不构成强拆。
        let close_result = domain.close(key, 0);
        let destroy_result = domain.destroy(key);
        close_result?;
        destroy_result?;
        *lock(&domain.thread_config)? = previous;
        return Err(code);
    }
    Ok(key)
}
pub unsafe fn initialize(domain: &Domain, params: *const amv_init_params_t) -> Result<usize> {
    let mut failed = lock(&INITIALIZATION)?;
    unsafe { initialize_inner(domain, params, false, &mut failed) }
}
#[no_mangle]
pub unsafe extern "C" fn amv_backend_retry(params: *const amv_init_params_t) -> i32 {
    let code = status(crate::domain(), 0, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let mut failed = lock(&INITIALIZATION)?;
        if crate::domain().context_count()? != 0 {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let key = unsafe { initialize_inner(crate::domain(), params, true, &mut failed) }?;
        crate::domain().close(key, 0)?;
        crate::domain().destroy(key)?;
        Ok(())
    });
    common::init_error(code);
    code
}
#[no_mangle]
pub unsafe extern "C" fn amv_query_backends(
    ctx: *mut amv_context_t,
    out: *mut am_backend_matrix_t,
) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        args::validate_output(out, std::mem::size_of::<am_backend_matrix_t>())?;
        args::write(out, am_backend_matrix_t::default())?;
        let state = State::get(ctx as usize)?;
        let context = state.context()?;
        let context = context.data::<Context>()?;
        let cuda = super::hardware::cached_cuda(context)?;
        let observed = runtime::query_snapshot(&context.runtime_dirs, cuda).map_err(error)?;
        let mut rows = lock(&state.backends)?;
        for (to, from) in rows.0.iter_mut().zip(observed) {
            *to = am_backend_row_t {
                backend: from.backend,
                hw_detected: from.hardware as u32,
                deps_present: from.dependencies as u32,
                provider_loadable: from.loaded as u32,
                verified: from.executed as u32,
                note_utf8: from.note.as_ptr(),
            };
        }
        args::write(
            out,
            am_backend_matrix_t {
                row_count: 4,
                rows: rows.0.as_ptr(),
                ..Default::default()
            },
        )
    })
}
