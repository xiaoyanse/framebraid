#![allow(clippy::missing_safety_doc)]
//! 对外C入口遵循公开头文件的指针寿命约定；不复制业务服务层。
use am_sdk_support::{abi::*, common, context, status};
mod captures;
mod decoder;
mod frames;
mod hardware;
mod probe;
mod reader;
mod shared;
mod sources;
mod subscriptions;
mod telemetry;
mod threads;
am_sdk_support::queue_exports!(amc_context_t, amc_get_queue_policy, amc_set_queue_policy);
am_sdk_support::common_exports!(
    false,
    amc_context_t,
    amc_init_params_t,
    context::initialize_camera,
    amc_get_abi_version,
    amc_get_version_info,
    amc_init,
    amc_shutdown,
    amc_shutdown_query,
    amc_get_init_error,
    amc_get_limits,
    amc_set_limits,
    amc_get_last_error,
    amc_status_name,
    amc_object_query_state,
    amc_object_request_close,
    amc_object_destroy
);
#[cfg(all(windows, not(test)))]
unsafe extern "C" fn release_sdk_statics() {
    // 这里只回收空域的Rust静态堆，绝不在loader锁中补关线程、库或用户回调。
    if sdk_unload_ready()
        && decoder::config::unload_ready()
        && frames::unload_ready()
        && sources::unload_ready()
    {
        unsafe {
            decoder::config::unload_static();
            frames::unload_static();
            sources::unload_static();
            sdk_unload_domain();
        }
    }
}
#[cfg(all(windows, not(test)))]
#[used]
#[unsafe(link_section = ".CRT$XPU")]
static SDK_UNLOAD: unsafe extern "C" fn() = release_sdk_statics;
#[no_mangle]
pub unsafe extern "C" fn amc_query_capability(
    ctx: *mut amc_context_t,
    out: *mut amc_capability_t,
) -> i32 {
    status(domain(), ctx as usize, || unsafe {
        common::camera_capability(domain(), ctx as usize, out)?;
        let mut budget_cap: amc_capability_t = am_sdk_support::args::read(out, 4)?;
        budget_cap.feature_bits |= AMC_CAP_WORKER_BUDGET as u32;
        am_sdk_support::args::write(out, budget_cap)?;
        if cfg!(all(windows, target_arch = "x86_64")) {
            let mut value: amc_capability_t = am_sdk_support::args::read(out, 4)?;
            value.feature_bits |= (AMC_CAP_DECODE_H264
                | AMC_CAP_DECODE_H265
                | AMC_CAP_DECODER_STANDALONE
                | AMC_CAP_BSF_ANNEXB
                | AMC_CAP_ENCODED_H264
                | AMC_CAP_ENCODED_H265
                | AMC_CAP_DECODE_MJPEG
                | AMC_CAP_CAPTURE_ISAPI
                | AMC_CAP_CAPTURE_CONTINUOUS
                | AMC_CAP_PUSH
                | AMC_CAP_PULL
                | AMC_CAP_REUSE) as u32;
            value.pixel_formats |= 15;
            value.feature_bits |= AMC_CAP_SCALE as u32;
            am_sdk_support::args::write(out, value)?;
        }
        Ok(())
    })
}
