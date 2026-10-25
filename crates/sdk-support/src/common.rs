use crate::{
    abi::*,
    args, clock,
    context::{self, Context},
    handles::Domain,
    Result,
};
use std::{
    ffi::c_char,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

#[derive(Default)]
struct InitFailure {
    code: i32,
    seq: u64,
    time: i64,
}
static INIT_ERROR: Mutex<InitFailure> = Mutex::new(InitFailure {
    code: 0,
    seq: 0,
    time: i64::MIN,
});
static INIT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub fn init_error(code: i32) {
    if let Ok(mut error) = INIT_ERROR.lock() {
        let seq = INIT_SEQUENCE.fetch_add(1, Ordering::AcqRel) + 1;
        *error = InitFailure {
            code,
            seq,
            time: if code == 0 {
                i64::MIN
            } else {
                clock::monotonic_ms()
            },
        };
    }
}

/// # Safety
/// out须指向已初始化size的公开版本结构。
pub unsafe fn version(out: *mut am_version_info_t) -> Result<()> {
    let value = am_version_info_t {
        abi_major: 1,
        abi_minor: 0,
        package_version_utf8: c"0.1.0-development".as_ptr(),
        build_target_utf8: if cfg!(all(windows, target_arch = "x86_64")) {
            c"windows-x64".as_ptr()
        } else {
            c"unverified-platform".as_ptr()
        },
        runtime_notes_utf8:
            c"SDK implementation in progress; media/inference runtimes load on demand".as_ptr(),
        ..Default::default()
    };
    unsafe { args::write(out, value) }
}
pub fn name(code: i32) -> *const c_char {
    status_name(code).unwrap_or(c"sdk.unknownStatus").as_ptr()
}

/// # Safety
/// out须指向size已设置的可写结构。
pub unsafe fn get_init_error(out: *mut am_init_error_t) -> Result<()> {
    let error = INIT_ERROR.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let mut value = am_init_error_t {
        code: error.code,
        source_utf8: c"sdk.init".as_ptr(),
        failed_at_ms: error.time,
        attempt_seq: error.seq,
        ..Default::default()
    };
    let text = status_name(error.code)
        .unwrap_or(c"sdk.internalError")
        .to_bytes();
    for (to, from) in value.message.iter_mut().take(255).zip(text) {
        *to = *from as c_char;
    }
    unsafe { args::write(out, value) }
}
/// # Safety
/// key为本库句柄，out结构可写。
pub unsafe fn last_error(domain: &Domain, key: usize, out: *mut am_error_info_t) -> Result<()> {
    let (code, detail) = crate::diagnostics::snapshot(domain, key)?;
    let value = am_error_info_t {
        code,
        message_utf8: detail.map_or_else(|| name(code), |d| d.message.as_ptr()),
        source_utf8: detail.map_or(c"sdk.object".as_ptr(), |d| d.source.as_ptr()),
        fault_class: detail.map_or(0, |d| d.fault_class),
        retryable: matches!(
            code,
            AM_ERR_SDK_TRY_AGAIN
                | AM_ERR_SDK_QUEUE_FULL
                | AM_ERR_SDK_RESOURCE_EXHAUSTED
                | AM_ERR_SDK_DEADLINE_EXCEEDED
        ) as u32,
        ..Default::default()
    };
    unsafe { args::write(out, value) }
}
/// # Safety
/// out结构可写，ctx为本库Context。
pub unsafe fn limits(domain: &Domain, key: usize, out: *mut am_limits_t) -> Result<()> {
    let entry = domain.get(key)?;
    let ctx = entry.data::<Context>()?;
    let value = *ctx.limits.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    unsafe { args::write(out, value) }
}
/// # Safety
/// pointer须指向完整limits结构。
pub unsafe fn set_limits(domain: &Domain, key: usize, pointer: *const am_limits_t) -> Result<()> {
    crate::callbacks::reject_reentry()?;
    let entry = domain.active(key)?;
    let ctx = entry.data::<Context>()?;
    let candidate = unsafe { args::read(pointer, std::mem::size_of::<am_limits_t>()) }?;
    let mut limits = ctx.limits.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    if !limits_within(&candidate, &limits) {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    *limits = candidate;
    Ok(())
}
/// # Safety
/// 所有输出遵循对应C结构，ctx为本库句柄。
pub unsafe fn camera_capability(
    domain: &Domain,
    key: usize,
    out: *mut amc_capability_t,
) -> Result<()> {
    let entry = domain.get(key)?;
    let ctx = entry.data::<Context>()?;
    let l = *ctx.limits.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let value = amc_capability_t {
        feature_bits: if context::shared_state(domain, 0) == AM_SFP_DISABLED as u32 {
            AMC_CAP_SHARED_FRAME_POOL as u32
        } else {
            0
        },
        max_sources: l.max_sources,
        max_decoders: l.max_decoders,
        max_subscribers_per_source: l.max_subscribers_per_source,
        frame_budget_bytes: l.frame_budget_bytes,
        shared_pool_state: context::shared_state(domain, ctx.flags),
        ..Default::default()
    };
    unsafe { args::write(out, value) }
}
/// # Safety
/// out为完整或兼容前缀能力结构。
pub unsafe fn vision_capability(
    domain: &Domain,
    key: usize,
    out: *mut amv_capability_t,
    features: u32,
    dtypes: u32,
) -> Result<()> {
    let entry = domain.get(key)?;
    let ctx = entry.data::<Context>()?;
    let value = amv_capability_t {
        feature_bits: features
            | if context::shared_state(domain, 0) == AM_SFP_DISABLED as u32 {
                AMV_CAP_SHARED_FRAME_POOL as u32
            } else {
                0
            },
        shared_pool_state: context::shared_state(domain, ctx.flags),
        dtype_bits: dtypes,
        max_batch_items: if features & AMV_CAP_BATCH as u32 != 0 {
            64
        } else {
            0
        },
        max_models: ctx
            .limits
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
            .max_models,
        infer_parallelism: ctx.thread_budget.map_or(0, |b| b.parallel),
        result_retention_default_ms: SDK_RESULT_TTL_MS,
        ..Default::default()
    };
    unsafe { args::write(out, value) }
}

/// 两库使用同一实现模板，状态在各自导出模块的Domain内，不跨库误收句柄。
#[macro_export]
macro_rules! common_exports {
    ($vision:expr,$context:ty,$params:ty,$initialize:path,$abi:ident,$version:ident,$init:ident,$shutdown:ident,$query:ident,$initerr:ident,$getlimits:ident,$setlimits:ident,$geterror:ident,$name:ident,$state:ident,$requestclose:ident,$destroy:ident) => {
        static SDK_DOMAIN: $crate::unload::UnloadCell<$crate::handles::Domain> =
            $crate::unload::UnloadCell::new();
        fn domain() -> &'static $crate::handles::Domain {
            SDK_DOMAIN.get_or_init(|| {
                $crate::handles::Domain::with_shared($vision, cfg!(feature = "shared-frames"))
            })
        }
        #[cfg(all(windows, not(test)))]
        fn sdk_unload_ready() -> bool {
            SDK_DOMAIN
                .get()
                .is_none_or(|domain| domain.idle_for_unload())
        }
        #[cfg(all(windows, not(test)))]
        unsafe fn sdk_unload_domain() {
            // 隔离区只在DLL终止时释放；SDK仍加载时不改变地址隔离承诺。
            let _ = unsafe { SDK_DOMAIN.clear_if(|domain| domain.idle_for_unload()) };
        }
        #[no_mangle]
        pub extern "C" fn $abi() -> u32 {
            $crate::abi::AM_ABI_VERSION
        }
        #[no_mangle]
        pub unsafe extern "C" fn $version(out: *mut $crate::abi::am_version_info_t) -> i32 {
            $crate::status(domain(), 0, || unsafe { $crate::common::version(out) })
        }
        #[no_mangle]
        pub unsafe extern "C" fn $init(params: *const $params, out: *mut *mut $context) -> i32 {
            let code = $crate::status(domain(), 0, || {
                $crate::callbacks::reject_reentry()?;
                $crate::args::pointer(out)?;
                unsafe {
                    out.write(std::ptr::null_mut());
                    let key = $initialize(domain(), params)?;
                    out.write(key as *mut $context);
                }
                Ok(())
            });
            if code != 0 {
                $crate::common::init_error(code);
            }
            code
        }
        #[no_mangle]
        pub unsafe extern "C" fn $shutdown(ctx: *mut $context, timeout: u32, out: *mut u32) -> i32 {
            $crate::status(domain(), ctx as usize, || {
                $crate::callbacks::reject_reentry()?;
                $crate::args::pointer(out)?;
                domain()
                    .get(ctx as usize)?
                    .data::<$crate::context::Context>()?;
                let state = domain().close(ctx as usize, timeout)?;
                unsafe {
                    out.write(state);
                }
                Ok(())
            })
        }
        #[no_mangle]
        pub unsafe extern "C" fn $query(ctx: *mut $context, out: *mut u32) -> i32 {
            $crate::status(domain(), ctx as usize, || {
                domain()
                    .get(ctx as usize)?
                    .data::<$crate::context::Context>()?;
                $crate::args::pointer(out)?;
                unsafe {
                    out.write(domain().state(ctx as usize)?);
                }
                Ok(())
            })
        }
        #[no_mangle]
        pub unsafe extern "C" fn $initerr(out: *mut $crate::abi::am_init_error_t) -> i32 {
            $crate::status(domain(), 0, || unsafe {
                $crate::common::get_init_error(out)
            })
        }
        #[no_mangle]
        pub unsafe extern "C" fn $getlimits(
            ctx: *mut $context,
            out: *mut $crate::abi::am_limits_t,
        ) -> i32 {
            $crate::status(domain(), ctx as usize, || unsafe {
                $crate::common::limits(domain(), ctx as usize, out)
            })
        }
        #[no_mangle]
        pub unsafe extern "C" fn $setlimits(
            ctx: *mut $context,
            limits: *const $crate::abi::am_limits_t,
        ) -> i32 {
            $crate::status(domain(), ctx as usize, || unsafe {
                $crate::common::set_limits(domain(), ctx as usize, limits)
            })
        }
        #[no_mangle]
        pub unsafe extern "C" fn $geterror(
            key: *const std::ffi::c_void,
            out: *mut $crate::abi::am_error_info_t,
        ) -> i32 {
            $crate::status(domain(), key as usize, || unsafe {
                $crate::common::last_error(domain(), key as usize, out)
            })
        }
        #[no_mangle]
        pub extern "C" fn $name(code: i32) -> *const std::ffi::c_char {
            $crate::common::name(code)
        }
        #[no_mangle]
        pub unsafe extern "C" fn $state(key: *const std::ffi::c_void, out: *mut u32) -> i32 {
            $crate::status(domain(), key as usize, || {
                $crate::args::pointer(out)?;
                let state = domain().state(key as usize)?;
                unsafe {
                    out.write(state);
                }
                Ok(())
            })
        }
        #[no_mangle]
        pub extern "C" fn $requestclose(key: *const std::ffi::c_void) -> i32 {
            $crate::status(domain(), key as usize, || {
                domain().request_close(key as usize)
            })
        }
        #[no_mangle]
        pub extern "C" fn $destroy(key: *mut std::ffi::c_void) -> i32 {
            $crate::status(domain(), key as usize, || {
                $crate::callbacks::reject_reentry()?;
                domain().destroy(key as usize)
            })
        }
    };
}
