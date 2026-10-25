// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
#[test]
fn am_realtime_policy_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_realtime_policy_t>(), 20);
    assert_eq!(core::mem::align_of::<am_realtime_policy_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_realtime_policy_t, struct_size),
        0,
        "am_realtime_policy_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_realtime_policy_t, max_inflight),
        4,
        "am_realtime_policy_t.max_inflight"
    );
    assert_eq!(
        core::mem::offset_of!(am_realtime_policy_t, replace_oldest),
        8,
        "am_realtime_policy_t.replace_oldest"
    );
    assert_eq!(
        core::mem::offset_of!(am_realtime_policy_t, slot_count),
        12,
        "am_realtime_policy_t.slot_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_realtime_policy_t, fair_round_robin),
        16,
        "am_realtime_policy_t.fair_round_robin"
    );
}
#[test]
fn am_limits_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_limits_t>(), 160);
    assert_eq!(core::mem::align_of::<am_limits_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_limits_t, struct_size),
        0,
        "am_limits_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, max_sources),
        4,
        "am_limits_t.max_sources"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, max_subscribers_per_source),
        8,
        "am_limits_t.max_subscribers_per_source"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, max_decoders),
        12,
        "am_limits_t.max_decoders"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, max_capture_continuous),
        16,
        "am_limits_t.max_capture_continuous"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, frame_budget_bytes),
        24,
        "am_limits_t.frame_budget_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, external_hold_max_frames),
        32,
        "am_limits_t.external_hold_max_frames"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, external_hold_max_bytes),
        40,
        "am_limits_t.external_hold_max_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, max_models),
        48,
        "am_limits_t.max_models"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, infer_parallelism),
        52,
        "am_limits_t.infer_parallelism"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, realtime_max_inflight),
        56,
        "am_limits_t.realtime_max_inflight"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, result_budget_bytes),
        64,
        "am_limits_t.result_budget_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, result_max_items),
        72,
        "am_limits_t.result_max_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, control_max_items),
        76,
        "am_limits_t.control_max_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, preprocess_budget_bytes),
        80,
        "am_limits_t.preprocess_budget_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, shared_pool_max_slots),
        88,
        "am_limits_t.shared_pool_max_slots"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, shared_pool_max_frame_bytes),
        92,
        "am_limits_t.shared_pool_max_frame_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, shared_pool_max_total_bytes),
        96,
        "am_limits_t.shared_pool_max_total_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, shared_pool_max_consumers),
        104,
        "am_limits_t.shared_pool_max_consumers"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, shared_pool_max_outstanding_borrows),
        108,
        "am_limits_t.shared_pool_max_outstanding_borrows"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, shared_pool_max_pools),
        112,
        "am_limits_t.shared_pool_max_pools"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, shared_pool_metadata_bytes),
        120,
        "am_limits_t.shared_pool_metadata_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_limits_t, reserved),
        128,
        "am_limits_t.reserved"
    );
}
#[test]
fn am_stats_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_stats_t>(), 232);
    assert_eq!(core::mem::align_of::<am_stats_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_stats_t, struct_size),
        0,
        "am_stats_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, scope_utf8),
        8,
        "am_stats_t.scope_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, queue_items),
        16,
        "am_stats_t.queue_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, queue_bytes),
        24,
        "am_stats_t.queue_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, external_held_items),
        32,
        "am_stats_t.external_held_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, external_held_bytes),
        40,
        "am_stats_t.external_held_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, accepted),
        48,
        "am_stats_t.accepted"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, rejected_backpressure),
        56,
        "am_stats_t.rejected_backpressure"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, replaced),
        64,
        "am_stats_t.replaced"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, expired),
        72,
        "am_stats_t.expired"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, dropped),
        80,
        "am_stats_t.dropped"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, frames_decoded),
        88,
        "am_stats_t.frames_decoded"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, frames_dropped),
        96,
        "am_stats_t.frames_dropped"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, newest_frame_age_ms),
        104,
        "am_stats_t.newest_frame_age_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, reconnect_count),
        112,
        "am_stats_t.reconnect_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, resync_count),
        120,
        "am_stats_t.resync_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, infer_executed),
        128,
        "am_stats_t.infer_executed"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, infer_failed),
        136,
        "am_stats_t.infer_failed"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, infer_queue_wait_avg_ms),
        144,
        "am_stats_t.infer_queue_wait_avg_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, infer_exec_avg_ms),
        152,
        "am_stats_t.infer_exec_avg_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, actual_capture_fps),
        160,
        "am_stats_t.actual_capture_fps"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, native_rss_estimate_bytes),
        168,
        "am_stats_t.native_rss_estimate_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, threads_active),
        176,
        "am_stats_t.threads_active"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, threads_peak),
        180,
        "am_stats_t.threads_peak"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, shared_mapped_bytes),
        184,
        "am_stats_t.shared_mapped_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, shared_metadata_bytes),
        192,
        "am_stats_t.shared_metadata_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, shared_pool_count),
        200,
        "am_stats_t.shared_pool_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, shared_outstanding_borrows),
        204,
        "am_stats_t.shared_outstanding_borrows"
    );
    assert_eq!(
        core::mem::offset_of!(am_stats_t, reserved),
        208,
        "am_stats_t.reserved"
    );
}
#[test]
fn am_pixel_view_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_pixel_view_t>(), 48);
    assert_eq!(core::mem::align_of::<am_pixel_view_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, struct_size),
        0,
        "am_pixel_view_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, magic),
        4,
        "am_pixel_view_t.magic"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, owner_handle),
        8,
        "am_pixel_view_t.owner_handle"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, retain_owner),
        16,
        "am_pixel_view_t.retain_owner"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, release_owner),
        24,
        "am_pixel_view_t.release_owner"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, view),
        32,
        "am_pixel_view_t.view"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, assoc),
        40,
        "am_pixel_view_t.assoc"
    );
}
