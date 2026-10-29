pub(crate) mod backends;
mod batch_exports;
mod batching_exports;
mod exports;
mod hardware;
mod input;
mod models;
mod notify;
mod receipt;
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
