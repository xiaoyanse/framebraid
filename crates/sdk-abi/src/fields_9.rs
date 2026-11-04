// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
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
