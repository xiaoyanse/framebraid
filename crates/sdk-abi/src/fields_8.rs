// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
impl AbiLayout for am_limits_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_limits_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, max_sources),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, max_subscribers_per_source),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, max_decoders),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, max_capture_continuous),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, frame_budget_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, external_hold_max_frames),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, external_hold_max_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, max_models),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, infer_parallelism),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, realtime_max_inflight),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, result_budget_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, result_max_items),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, control_max_items),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, preprocess_budget_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, shared_pool_max_slots),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, shared_pool_max_frame_bytes),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, shared_pool_max_total_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, shared_pool_max_consumers),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, shared_pool_max_outstanding_borrows),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, shared_pool_max_pools),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, shared_pool_metadata_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_limits_t, reserved),
            core::mem::size_of::<[u32; 8]>(),
        ),
    ];
}
impl AbiLayout for am_stats_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_stats_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, scope_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, queue_items),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, queue_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, external_held_items),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, external_held_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, accepted),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, rejected_backpressure),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, replaced),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, expired),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, dropped),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, frames_decoded),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, frames_dropped),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, newest_frame_age_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, reconnect_count),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, resync_count),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, infer_executed),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, infer_failed),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, infer_queue_wait_avg_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, infer_exec_avg_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, actual_capture_fps),
            core::mem::size_of::<f64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, native_rss_estimate_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, threads_active),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, threads_peak),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, shared_mapped_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, shared_metadata_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, shared_pool_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, shared_outstanding_borrows),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, reserved),
            core::mem::size_of::<[u32; 6]>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, gpu_inputs),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_stats_t, gpu_copies_per_frame),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for am_pixel_view_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_pixel_view_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, magic),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, owner_handle),
            core::mem::size_of::<*mut c_void>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, retain_owner),
            core::mem::size_of::<Option<unsafe extern "C" fn(*mut c_void)>>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, release_owner),
            core::mem::size_of::<Option<unsafe extern "C" fn(*mut c_void)>>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, view),
            core::mem::size_of::<*const am_frame_view_t>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, assoc),
            core::mem::size_of::<*const am_frame_assoc_t>(),
        ),
    ];
}
