//! 两导出库共用的ABI支撑源码；各库独立登记句柄，不假装跨模块静态变量共享。
pub mod args;
pub mod buffers;
pub mod callbacks;
pub mod clock;
pub mod common;
pub mod context;
pub mod diagnostics;
pub mod handles;
pub mod queues;
#[cfg(all(windows, target_arch = "x86_64", feature = "shared-frames"))]
pub mod shared;
pub mod unload;

pub use am_sdk_abi as abi;
pub type Result<T> = std::result::Result<T, abi::am_status_t>;
// 真正帧池模块接通之前，显式启用也不能报可用。
pub const SHARED_IMPLEMENTED: bool = cfg!(all(
    windows,
    target_arch = "x86_64",
    feature = "shared-frames"
));

#[macro_export]
macro_rules! shared_call {
    ($method:ident($($arg:expr),*)) => {{
        #[cfg(all(windows,target_arch="x86_64",feature="shared-frames"))]
        { $crate::shared::api::$method($($arg),*) }
        #[cfg(not(all(windows,target_arch="x86_64",feature="shared-frames")))]
        { $(let _ = &$arg;)* Err($crate::abi::AM_ERR_SDK_NOT_SUPPORTED) }
    }};
}

pub fn status(domain: &handles::Domain, key: usize, call: impl FnOnce() -> Result<()>) -> i32 {
    let scope = diagnostics::Scope::begin(domain, key);
    let code = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)) {
        Ok(Ok(())) => abi::AM_OK as i32,
        Ok(Err(code)) => code,
        Err(_) => abi::AM_ERR_SDK_INTERNAL_ERROR,
    };
    if code != 0 {
        domain.record_error_detail(key, code, scope.detail(code));
    }
    code
}
