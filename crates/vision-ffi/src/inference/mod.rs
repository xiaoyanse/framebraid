pub(crate) mod backends;
mod batch_exports;
mod batching_exports;
mod cascade;
#[cfg(test)]
mod cascade_e2e_tests;
mod exports;
#[cfg(test)]
mod exports_tests;
mod hardware;
mod input;
mod models;
mod notify;
mod receipt;
#[cfg(test)]
mod receipt_tests;
mod records;
mod results;
mod scheduler;
mod selected_exports;
mod selection;
#[cfg(test)]
mod selection_tests;
mod state;
mod submission;
mod swap_exports;
#[cfg(test)]
mod swap_tests;
mod telemetry;
#[cfg(test)]
mod telemetry_tests;
mod tensor_exports;
mod tensors;
mod tracker;
#[cfg(test)]
mod tracker_tests;
#[cfg(test)]
mod vocab_e2e_tests;

use am_sdk_support::{abi::*, Result};
pub fn error(e: am_vision_core::Error) -> i32 {
    let key = e.0.split('|').next().unwrap_or("sdk.internalError");
    (0..=AM_ERR_VISION_INSUFFICIENT_BATCH_REJECTED)
        .find(|n| status_name(*n).is_some_and(|name| name.to_bytes() == key.as_bytes()))
        .unwrap_or(AM_ERR_SDK_INTERNAL_ERROR)
}
pub fn lock<T>(mutex: &std::sync::Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    mutex.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)
}
/// 诊断详情要求 'static 文本；按内容驻留（失败详情种类有限，进程生命周期保活，
/// 旧详情指针不随后续失败失效——与 sdk-support 诊断寿命约定一致）。
/// v0.3.3 起委托 sdk-support 单一实现。
pub fn intern_detail(text: &str) -> &'static std::ffi::CStr {
    am_sdk_support::diagnostics::intern(text)
}
