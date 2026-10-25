// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_decoder_status_t {
    pub struct_size: u32,
    pub state: am_decoder_state_e,
    pub decoder_generation: u32,
    pub config_generation: u32,
    pub queue_items: u32,
    pub queue_bytes: u64,
}
impl Default for am_decoder_status_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            state: Default::default(),
            decoder_generation: Default::default(),
            config_generation: Default::default(),
            queue_items: Default::default(),
            queue_bytes: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_decoder_info_t {
    pub struct_size: u32,
    pub codec: u32,
    pub bitstream_format: u32,
    pub output_pixel_format: u32,
    pub hw_policy: u32,
    pub hw_accelerated: u32,
    pub backend_name_utf8: *const c_char,
    pub config_generation: u32,
}
impl Default for am_decoder_info_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            codec: Default::default(),
            bitstream_format: Default::default(),
            output_pixel_format: Default::default(),
            hw_policy: Default::default(),
            hw_accelerated: Default::default(),
            backend_name_utf8: Default::default(),
            config_generation: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_capture_request_t {
    pub struct_size: u32,
    pub output_kind: u32,
    pub deadline_ms: u32,
    pub reserved: [u32; 4],
}
impl Default for am_capture_request_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            output_kind: Default::default(),
            deadline_ms: Default::default(),
            reserved: [Default::default(); 4],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_captured_image_t {
    pub struct_size: u32,
    pub request_id: u64,
    pub output_kind: u32,
    pub encoded: *const u8,
    pub encoded_size: u64,
    pub view: *const am_frame_view_t,
    pub meta: *const am_frame_meta_t,
}
impl Default for am_captured_image_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            request_id: Default::default(),
            output_kind: Default::default(),
            encoded: Default::default(),
            encoded_size: Default::default(),
            view: Default::default(),
            meta: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_capture_continuous_desc_t {
    pub struct_size: u32,
    pub target_interval_ms: u32,
    pub max_inflight: u32,
    pub request_timeout_ms: u32,
    pub output_kind: u32,
    pub mode: u32,
    pub queue_max_items: u32,
    pub queue_max_bytes: u64,
    pub reserved: [u32; 4],
}
impl Default for am_capture_continuous_desc_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            target_interval_ms: Default::default(),
            max_inflight: Default::default(),
            request_timeout_ms: Default::default(),
            output_kind: Default::default(),
            mode: Default::default(),
            queue_max_items: Default::default(),
            queue_max_bytes: Default::default(),
            reserved: [Default::default(); 4],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_capture_stats_t {
    pub struct_size: u32,
    pub actual_fps: f64,
    pub last_received_at_ms: i64,
    pub consecutive_failures: u32,
    pub newest_age_ms: i64,
    pub skipped_beats: u64,
    pub shared_consumers: u64,
}
impl Default for am_capture_stats_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            actual_fps: Default::default(),
            last_received_at_ms: Default::default(),
            consecutive_failures: Default::default(),
            newest_age_ms: Default::default(),
            skipped_beats: Default::default(),
            shared_consumers: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_event_detail_t {
    pub struct_size: u32,
    pub generation: u32,
    pub scope: am_event_scope_e,
    pub affected_handle: *mut c_void,
    pub fault_class: u32,
    pub reason_utf8: *const c_char,
}
impl Default for am_event_detail_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            generation: Default::default(),
            scope: Default::default(),
            affected_handle: Default::default(),
            fault_class: Default::default(),
            reason_utf8: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct amv_capability_t {
    pub struct_size: u32,
    pub abi_minor: u32,
    pub feature_bits: u32,
    pub dtype_bits: u32,
    pub max_batch_items: u32,
    pub max_models: u32,
    pub infer_parallelism: u32,
    pub result_retention_default_ms: u64,
    pub shared_pool_state: u32,
}
impl Default for amv_capability_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            abi_minor: Default::default(),
            feature_bits: Default::default(),
            dtype_bits: Default::default(),
            max_batch_items: Default::default(),
            max_models: Default::default(),
            infer_parallelism: Default::default(),
            result_retention_default_ms: Default::default(),
            shared_pool_state: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_backend_row_t {
    pub backend: u32,
    pub hw_detected: u32,
    pub deps_present: u32,
    pub provider_loadable: u32,
    pub verified: u32,
    pub note_utf8: *const c_char,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_backend_matrix_t {
    pub struct_size: u32,
    pub row_count: u32,
    pub rows: *const am_backend_row_t,
}
impl Default for am_backend_matrix_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            row_count: Default::default(),
            rows: Default::default(),
        }
    }
}
