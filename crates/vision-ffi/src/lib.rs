#![allow(clippy::missing_safety_doc)]
//! 对外C入口遵循公开头文件的指针寿命约定；模型及队列使用同源核心。
use am_sdk_support::{abi::*, common, status};
mod inference;
mod shared;
am_sdk_support::queue_exports!(amv_context_t, amv_get_queue_policy, amv_set_queue_policy);
am_sdk_support::common_exports!(
    true,
    amv_context_t,
    amv_init_params_t,
    inference::backends::initialize,
    amv_get_abi_version,
    amv_get_version_info,
    amv_init,
    amv_shutdown,
    amv_shutdown_query,
    amv_get_init_error,
    amv_get_limits,
    amv_set_limits,
    amv_get_last_error,
    amv_status_name,
    amv_object_query_state,
    amv_object_request_close,
    amv_object_destroy
);
#[cfg(all(windows, not(test)))]
unsafe extern "C" fn release_sdk_statics() {
    if sdk_unload_ready() && am_vision_core::sdk::runtime::sdk_unload_ready() {
        unsafe {
            am_vision_core::profiles::release_for_sdk_unload();
            am_vision_core::sdk::runtime::sdk_unload_static();
            sdk_unload_domain();
        }
    }
}
#[cfg(all(windows, not(test)))]
#[used]
#[unsafe(link_section = ".CRT$XPU")]
static SDK_UNLOAD: unsafe extern "C" fn() = release_sdk_statics;
#[no_mangle]
pub unsafe extern "C" fn amv_query_capability(
    ctx: *mut amv_context_t,
    out: *mut amv_capability_t,
) -> i32 {
    status(domain(), ctx as usize, || unsafe {
        common::vision_capability(
            domain(),
            ctx as usize,
            out,
            (AMV_CAP_IMAGE_INPUT
                | AMV_CAP_TENSOR_INPUT
                | AMV_CAP_BATCH
                | AMV_CAP_RAW_MODEL
                | AMV_CAP_MEMORY_MODEL
                | AMV_CAP_IMAGE_PREPROCESS_RAW
                | AMV_CAP_RESULT_RAW_TENSOR
                | AMV_CAP_PUSH_NOTIFY
                | AMV_CAP_ORDERED_SELECTION
                // 设备定向选择器（§3.5b，ABI 1.6）：参数解析能力恒在；
                // 定向零命中按 sdk.invalidArgument 拒绝而非能力缺失。
                | AMV_CAP_DEVICE_SELECTOR
                // GPU 帧直传按实际可载置位（审计 A4；ABI 1.8 起 bit10 为任一互操作
                // 入口可用——CUDA 运行库或 OV remote 探测，查询与实际支持一致）；
                // 级联位仍仅绑 CUDA 互操作（级联合同 CUDA:0 约束不变）。
                | if am_vision_core::gpu::pipeline::GpuPipeline::available() {
                    AMV_CAP_GPU_FRAME_INPUT | AMV_CAP_GPU_CASCADE
                } else if am_vision_core::openvino_remote_available() {
                    AMV_CAP_GPU_FRAME_INPUT
                } else {
                    0
                }) as u32,
            31,
        )
    })
}
