// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_segment_result_t {
    pub count: u32,
    pub instances: *const am_segment_instance_t,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_obb_instance_t {
    pub vertices_xy: [f32; 8],
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    pub angle: f32,
    pub confidence: f32,
    pub class_id: i32,
    pub angle_normalized: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_obb_result_t {
    pub count: u32,
    pub instances: *const am_obb_instance_t,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct amv_result_t {
    pub struct_size: u32,
    pub request_id: u64,
    pub task: am_task_e,
    pub frame: *const am_frame_assoc_t,
    pub model: am_model_identity_t,
    pub execution: am_backend_actual_t,
    pub transform: *const am_preprocess_meta_t,
    pub pose: am_pose_result_t,
    pub detect: am_detect_result_t,
    pub classify: am_classify_result_t,
    pub segment: am_segment_result_t,
    pub obb: am_obb_result_t,
    pub raw_output_count: u32,
    pub raw_outputs: *const am_tensor_out_t,
    pub infer_start_ms: i64,
    pub infer_end_ms: i64,
}
impl Default for amv_result_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            request_id: Default::default(),
            task: Default::default(),
            frame: Default::default(),
            model: Default::default(),
            execution: Default::default(),
            transform: Default::default(),
            pose: Default::default(),
            detect: Default::default(),
            classify: Default::default(),
            segment: Default::default(),
            obb: Default::default(),
            raw_output_count: Default::default(),
            raw_outputs: Default::default(),
            infer_start_ms: Default::default(),
            infer_end_ms: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_realtime_policy_t {
    pub struct_size: u32,
    pub max_inflight: u32,
    pub replace_oldest: u32,
    pub slot_count: u32,
    pub fair_round_robin: u32,
}
impl Default for am_realtime_policy_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            max_inflight: Default::default(),
            replace_oldest: Default::default(),
            slot_count: Default::default(),
            fair_round_robin: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_limits_t {
    pub struct_size: u32,
    pub max_sources: u32,
    pub max_subscribers_per_source: u32,
    pub max_decoders: u32,
    pub max_capture_continuous: u32,
    pub frame_budget_bytes: u64,
    pub external_hold_max_frames: u32,
    pub external_hold_max_bytes: u64,
    pub max_models: u32,
    pub infer_parallelism: u32,
    pub realtime_max_inflight: u32,
    pub result_budget_bytes: u64,
    pub result_max_items: u32,
    pub control_max_items: u32,
    pub preprocess_budget_bytes: u64,
    pub shared_pool_max_slots: u32,
    pub shared_pool_max_frame_bytes: u32,
    pub shared_pool_max_total_bytes: u64,
    pub shared_pool_max_consumers: u32,
    pub shared_pool_max_outstanding_borrows: u32,
    pub shared_pool_max_pools: u32,
    pub shared_pool_metadata_bytes: u64,
    pub reserved: [u32; 8],
}
impl Default for am_limits_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            max_sources: Default::default(),
            max_subscribers_per_source: Default::default(),
            max_decoders: Default::default(),
            max_capture_continuous: Default::default(),
            frame_budget_bytes: Default::default(),
            external_hold_max_frames: Default::default(),
            external_hold_max_bytes: Default::default(),
            max_models: Default::default(),
            infer_parallelism: Default::default(),
            realtime_max_inflight: Default::default(),
            result_budget_bytes: Default::default(),
            result_max_items: Default::default(),
            control_max_items: Default::default(),
            preprocess_budget_bytes: Default::default(),
            shared_pool_max_slots: Default::default(),
            shared_pool_max_frame_bytes: Default::default(),
            shared_pool_max_total_bytes: Default::default(),
            shared_pool_max_consumers: Default::default(),
            shared_pool_max_outstanding_borrows: Default::default(),
            shared_pool_max_pools: Default::default(),
            shared_pool_metadata_bytes: Default::default(),
            reserved: [Default::default(); 8],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_stats_t {
    pub struct_size: u32,
    pub scope_utf8: *const c_char,
    pub queue_items: u64,
    pub queue_bytes: u64,
    pub external_held_items: u64,
    pub external_held_bytes: u64,
    pub accepted: u64,
    pub rejected_backpressure: u64,
    pub replaced: u64,
    pub expired: u64,
    pub dropped: u64,
    pub frames_decoded: u64,
    pub frames_dropped: u64,
    pub newest_frame_age_ms: i64,
    pub reconnect_count: u64,
    pub resync_count: u64,
    pub infer_executed: u64,
    pub infer_failed: u64,
    pub infer_queue_wait_avg_ms: i64,
    pub infer_exec_avg_ms: i64,
    pub actual_capture_fps: f64,
    pub native_rss_estimate_bytes: u64,
    pub threads_active: u32,
    pub threads_peak: u32,
    pub shared_mapped_bytes: u64,
    pub shared_metadata_bytes: u64,
    pub shared_pool_count: u32,
    pub shared_outstanding_borrows: u32,
    pub reserved: [u32; 6],
}
impl Default for am_stats_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            scope_utf8: Default::default(),
            queue_items: Default::default(),
            queue_bytes: Default::default(),
            external_held_items: Default::default(),
            external_held_bytes: Default::default(),
            accepted: Default::default(),
            rejected_backpressure: Default::default(),
            replaced: Default::default(),
            expired: Default::default(),
            dropped: Default::default(),
            frames_decoded: Default::default(),
            frames_dropped: Default::default(),
            newest_frame_age_ms: Default::default(),
            reconnect_count: Default::default(),
            resync_count: Default::default(),
            infer_executed: Default::default(),
            infer_failed: Default::default(),
            infer_queue_wait_avg_ms: Default::default(),
            infer_exec_avg_ms: Default::default(),
            actual_capture_fps: Default::default(),
            native_rss_estimate_bytes: Default::default(),
            threads_active: Default::default(),
            threads_peak: Default::default(),
            shared_mapped_bytes: Default::default(),
            shared_metadata_bytes: Default::default(),
            shared_pool_count: Default::default(),
            shared_outstanding_borrows: Default::default(),
            reserved: [Default::default(); 6],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_pixel_view_t {
    pub struct_size: u32,
    pub magic: u32,
    pub owner_handle: *mut c_void,
    pub retain_owner: Option<unsafe extern "C" fn(*mut c_void)>,
    pub release_owner: Option<unsafe extern "C" fn(*mut c_void)>,
    pub view: *const am_frame_view_t,
    pub assoc: *const am_frame_assoc_t,
}
impl Default for am_pixel_view_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            magic: Default::default(),
            owner_handle: Default::default(),
            retain_owner: Default::default(),
            release_owner: Default::default(),
            view: Default::default(),
            assoc: Default::default(),
        }
    }
}
