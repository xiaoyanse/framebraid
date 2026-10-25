// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
pub type am_status_t = i32;
pub type am_lifecycle_state_t = u32;
#[repr(C)]
pub struct amc_context_t {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct amv_context_t {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct amc_source_t {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct amc_encoded_sub_t {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct amc_frame_sub_t {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct amc_frame_t {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct amc_capture_sub_t {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct amc_decoder_t {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct amv_model_t {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct amc_shared_pool_t {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct amv_shared_pool_t {
    _opaque: [u8; 0],
}
pub type am_queue_class_e = i32;
pub const AM_Q_REALTIME_FRAMES: am_queue_class_e = 0;
pub const AM_Q_ENCODED_STREAM: am_queue_class_e = 1;
pub const AM_Q_REALTIME_INFER: am_queue_class_e = 2;
pub const AM_Q_OFFLINE_BATCH: am_queue_class_e = 3;
pub const AM_Q_RESULTS: am_queue_class_e = 4;
pub const AM_Q_CONTROL: am_queue_class_e = 5;
pub type am_shared_pool_state_e = i32;
pub const AM_SFP_ENABLED: am_shared_pool_state_e = 0;
pub const AM_SFP_DISABLED: am_shared_pool_state_e = 1;
pub const AM_SFP_NOT_COMPILED: am_shared_pool_state_e = 2;
pub const AM_SFP_PLATFORM_UNSUPPORTED: am_shared_pool_state_e = 3;
pub type am_source_kind_e = i32;
pub const AM_SRC_RTSP_URL: am_source_kind_e = 0;
pub const AM_SRC_ISAPI_URL: am_source_kind_e = 1;
pub const AM_SRC_DEVICE_REF: am_source_kind_e = 2;
pub type am_source_state_e = i32;
pub const AM_SRC_WAITING_CREDENTIALS: am_source_state_e = 0;
pub const AM_SRC_CONNECTING: am_source_state_e = 1;
pub const AM_SRC_LIVE: am_source_state_e = 2;
pub const AM_SRC_RECONNECT_WAIT: am_source_state_e = 3;
pub const AM_SRC_AUTH_WAITING: am_source_state_e = 4;
pub const AM_SRC_CLOSING: am_source_state_e = 5;
pub const AM_SRC_CLOSED: am_source_state_e = 6;
pub type am_fault_class_e = i32;
pub const AM_FAULT_NONE: am_fault_class_e = 0;
pub const AM_FAULT_NETWORK: am_fault_class_e = 1;
pub const AM_FAULT_AUTH: am_fault_class_e = 2;
pub const AM_FAULT_RESOURCE: am_fault_class_e = 3;
pub const AM_FAULT_KEYFRAME_WAIT: am_fault_class_e = 4;
pub const AM_FAULT_DECODE: am_fault_class_e = 5;
pub type am_probe_status_e = i32;
pub const AM_PROBE_OK: am_probe_status_e = 0;
pub const AM_PROBE_TIMEOUT: am_probe_status_e = 1;
pub const AM_PROBE_AUTH_FAILED: am_probe_status_e = 2;
pub const AM_PROBE_UNREACHABLE: am_probe_status_e = 3;
pub const AM_PROBE_UNSUPPORTED: am_probe_status_e = 4;
pub const AM_PROBE_PENDING: am_probe_status_e = 5;
pub type am_pixel_format_e = i32;
pub const AM_PF_RGBA8: am_pixel_format_e = 0;
pub const AM_PF_RGB8: am_pixel_format_e = 1;
pub const AM_PF_BGR8: am_pixel_format_e = 2;
pub const AM_PF_NV12: am_pixel_format_e = 3;
pub type am_color_matrix_e = i32;
pub const AM_CM_BT601: am_color_matrix_e = 0;
pub const AM_CM_BT709: am_color_matrix_e = 1;
pub const AM_CM_BT2020_NCL: am_color_matrix_e = 2;
pub type am_color_range_e = i32;
pub const AM_CR_LIMITED: am_color_range_e = 0;
pub const AM_CR_FULL: am_color_range_e = 1;
pub type am_clock_quality_e = i32;
pub const AM_CQ_UNKNOWN: am_clock_quality_e = 0;
pub const AM_CQ_LOCAL_RECEIPT: am_clock_quality_e = 1;
pub const AM_CQ_DEVICE_SYNCED: am_clock_quality_e = 2;
pub const AM_CQ_EXTRAPOLATED: am_clock_quality_e = 3;
pub type am_decoder_hw_policy_e = i32;
pub const AM_DHW_PREFER_HW: am_decoder_hw_policy_e = 0;
pub const AM_DHW_SOFTWARE_ONLY: am_decoder_hw_policy_e = 1;
pub type am_decoder_state_e = i32;
pub const AM_DEC_READY: am_decoder_state_e = 0;
pub const AM_DEC_DRAINING: am_decoder_state_e = 1;
pub const AM_DEC_DRAINED: am_decoder_state_e = 2;
pub const AM_DEC_RESET_PENDING: am_decoder_state_e = 3;
pub const AM_DEC_CLOSING: am_decoder_state_e = 4;
pub const AM_DEC_CLOSED: am_decoder_state_e = 5;
pub type am_event_scope_e = i32;
pub const AM_EVENT_SCOPE_SINGLE_SUB: am_event_scope_e = 0;
pub const AM_EVENT_SCOPE_SOURCE: am_event_scope_e = 1;
pub const AM_EVENT_SCOPE_ALL_CONSUMERS: am_event_scope_e = 2;
pub type am_backend_e = i32;
pub const AM_BACKEND_CPU: am_backend_e = 0;
pub const AM_BACKEND_OPENVINO: am_backend_e = 1;
pub const AM_BACKEND_CUDA: am_backend_e = 2;
pub type am_dtype_e = i32;
pub const AM_DTYPE_F32: am_dtype_e = 0;
pub const AM_DTYPE_F16: am_dtype_e = 1;
pub const AM_DTYPE_I64: am_dtype_e = 2;
pub const AM_DTYPE_I32: am_dtype_e = 3;
pub const AM_DTYPE_U8: am_dtype_e = 4;
pub type am_task_e = i32;
pub const AM_TASK_NONE: am_task_e = 0;
pub const AM_TASK_POSE: am_task_e = 1;
pub const AM_TASK_DETECT: am_task_e = 2;
pub const AM_TASK_CLASSIFY: am_task_e = 3;
pub const AM_TASK_SEGMENT: am_task_e = 4;
pub const AM_TASK_OBB: am_task_e = 5;
pub const AM_TASK_GENERIC: am_task_e = 6;
pub type am_source_event_e = i32;
pub const AM_EVENT_STATE_CHANGED: am_source_event_e = 0;
pub const AM_EVENT_GENERATION_CHANGED: am_source_event_e = 1;
pub const AM_EVENT_ENCODED_RESYNC: am_source_event_e = 2;
pub const AM_EVENT_SPATIAL_CHANGED: am_source_event_e = 3;
pub const AM_EVENT_AUTH_WAITING: am_source_event_e = 4;
pub type am_lifecycle_state_e = i32;
pub const AM_LIFECYCLE_RUNNING: am_lifecycle_state_e = 0;
pub const AM_LIFECYCLE_CANCEL_REQUESTED: am_lifecycle_state_e = 1;
pub const AM_LIFECYCLE_CLOSING: am_lifecycle_state_e = 2;
pub const AM_LIFECYCLE_CLOSED: am_lifecycle_state_e = 3;
pub type am_request_state_e = i32;
pub const AM_REQ_PENDING: am_request_state_e = 0;
pub const AM_REQ_RUNNING: am_request_state_e = 1;
pub const AM_REQ_DONE: am_request_state_e = 2;
pub const AM_REQ_FAILED: am_request_state_e = 3;
pub const AM_REQ_CANCELED: am_request_state_e = 4;
pub const AM_REQ_EXPIRED: am_request_state_e = 5;
pub const AM_REQ_REPLACED: am_request_state_e = 6;
pub const AM_REQ_UNKNOWN: am_request_state_e = 7;
pub type am_credential_fetch_fn = Option<
    unsafe extern "C" fn(*mut c_void, u32, *mut am_credential_data_t, *mut u32) -> am_status_t,
>;
pub type am_credential_drop_fn = Option<unsafe extern "C" fn(*mut c_void)>;
pub type am_encoded_cb_fn =
    Option<unsafe extern "C" fn(*mut amc_encoded_sub_t, *mut am_encoded_packet_t, *mut c_void)>;
pub type am_frame_cb_fn =
    Option<unsafe extern "C" fn(*mut amc_frame_sub_t, *mut amc_frame_t, *mut c_void)>;
pub type am_capture_cb_fn =
    Option<unsafe extern "C" fn(*mut amc_capture_sub_t, *mut am_captured_image_t, *mut c_void)>;
pub type am_source_event_fn = Option<
    unsafe extern "C" fn(
        *mut amc_source_t,
        am_source_event_e,
        *const am_event_detail_t,
        *mut c_void,
    ),
>;
pub type am_result_notify_fn =
    Option<unsafe extern "C" fn(*mut amv_model_t, u64, u32, *mut c_void)>;
/// 只复制完整字段，避免输出C布局中的未初始化填充字节。
pub trait AbiLayout {
    const FIELDS: &'static [(usize, usize)];
}
