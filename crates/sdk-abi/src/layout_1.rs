// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
#[test]
fn am_shared_pool_config_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_shared_pool_config_t>(), 56);
    assert_eq!(core::mem::align_of::<am_shared_pool_config_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_config_t, struct_size),
        0,
        "am_shared_pool_config_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_config_t, slot_count),
        4,
        "am_shared_pool_config_t.slot_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_config_t, max_frame_bytes),
        8,
        "am_shared_pool_config_t.max_frame_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_config_t, total_bytes),
        16,
        "am_shared_pool_config_t.total_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_config_t, max_consumers),
        24,
        "am_shared_pool_config_t.max_consumers"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_config_t, max_outstanding_borrows),
        28,
        "am_shared_pool_config_t.max_outstanding_borrows"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_config_t, drop_policy),
        32,
        "am_shared_pool_config_t.drop_policy"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_config_t, publish_wait_ms),
        36,
        "am_shared_pool_config_t.publish_wait_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_config_t, reserved),
        40,
        "am_shared_pool_config_t.reserved"
    );
}
#[test]
fn am_shared_pool_attach_desc_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_shared_pool_attach_desc_t>(), 72);
    assert_eq!(core::mem::align_of::<am_shared_pool_attach_desc_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_attach_desc_t, struct_size),
        0,
        "am_shared_pool_attach_desc_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_attach_desc_t, protocol_version),
        4,
        "am_shared_pool_attach_desc_t.protocol_version"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_attach_desc_t, pool_id),
        8,
        "am_shared_pool_attach_desc_t.pool_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_attach_desc_t, owner_pid),
        16,
        "am_shared_pool_attach_desc_t.owner_pid"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_attach_desc_t, owner_started_at_100ns),
        24,
        "am_shared_pool_attach_desc_t.owner_started_at_100ns"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_attach_desc_t, pool_generation),
        32,
        "am_shared_pool_attach_desc_t.pool_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_attach_desc_t, identity_tag),
        36,
        "am_shared_pool_attach_desc_t.identity_tag"
    );
}
#[test]
fn am_shared_frame_view_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_shared_frame_view_t>(), 232);
    assert_eq!(core::mem::align_of::<am_shared_frame_view_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_view_t, struct_size),
        0,
        "am_shared_frame_view_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_view_t, desc),
        8,
        "am_shared_frame_view_t.desc"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_view_t, plane_data),
        192,
        "am_shared_frame_view_t.plane_data"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_view_t, borrow_id),
        224,
        "am_shared_frame_view_t.borrow_id"
    );
}
#[test]
fn am_shared_pool_stats_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_shared_pool_stats_t>(), 88);
    assert_eq!(core::mem::align_of::<am_shared_pool_stats_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, struct_size),
        0,
        "am_shared_pool_stats_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, published),
        8,
        "am_shared_pool_stats_t.published"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, dropped_stale),
        16,
        "am_shared_pool_stats_t.dropped_stale"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, dropped_full),
        24,
        "am_shared_pool_stats_t.dropped_full"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, wait_timeouts),
        32,
        "am_shared_pool_stats_t.wait_timeouts"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, slots_in_use),
        40,
        "am_shared_pool_stats_t.slots_in_use"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, slots_borrowed),
        44,
        "am_shared_pool_stats_t.slots_borrowed"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, outstanding_borrows),
        48,
        "am_shared_pool_stats_t.outstanding_borrows"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, consumers_attached),
        52,
        "am_shared_pool_stats_t.consumers_attached"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, bytes_in_use),
        56,
        "am_shared_pool_stats_t.bytes_in_use"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, metadata_bytes),
        64,
        "am_shared_pool_stats_t.metadata_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_pool_stats_t, reserved),
        72,
        "am_shared_pool_stats_t.reserved"
    );
}
#[test]
fn amc_capability_t_matches_c() {
    assert_eq!(core::mem::size_of::<amc_capability_t>(), 48);
    assert_eq!(core::mem::align_of::<amc_capability_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(amc_capability_t, struct_size),
        0,
        "amc_capability_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(amc_capability_t, abi_minor),
        4,
        "amc_capability_t.abi_minor"
    );
    assert_eq!(
        core::mem::offset_of!(amc_capability_t, feature_bits),
        8,
        "amc_capability_t.feature_bits"
    );
    assert_eq!(
        core::mem::offset_of!(amc_capability_t, pixel_formats),
        12,
        "amc_capability_t.pixel_formats"
    );
    assert_eq!(
        core::mem::offset_of!(amc_capability_t, max_sources),
        16,
        "amc_capability_t.max_sources"
    );
    assert_eq!(
        core::mem::offset_of!(amc_capability_t, max_decoders),
        20,
        "amc_capability_t.max_decoders"
    );
    assert_eq!(
        core::mem::offset_of!(amc_capability_t, max_subscribers_per_source),
        24,
        "amc_capability_t.max_subscribers_per_source"
    );
    assert_eq!(
        core::mem::offset_of!(amc_capability_t, frame_budget_bytes),
        32,
        "amc_capability_t.frame_budget_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(amc_capability_t, shared_pool_state),
        40,
        "amc_capability_t.shared_pool_state"
    );
}
#[test]
fn am_source_desc_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_source_desc_t>(), 64);
    assert_eq!(core::mem::align_of::<am_source_desc_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_source_desc_t, struct_size),
        0,
        "am_source_desc_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_desc_t, kind),
        4,
        "am_source_desc_t.kind"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_desc_t, url_utf8),
        8,
        "am_source_desc_t.url_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_desc_t, stream_kind_utf8),
        16,
        "am_source_desc_t.stream_kind_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_desc_t, creds),
        24,
        "am_source_desc_t.creds"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_desc_t, provider),
        32,
        "am_source_desc_t.provider"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_desc_t, reuse),
        40,
        "am_source_desc_t.reuse"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_desc_t, reserved),
        44,
        "am_source_desc_t.reserved"
    );
}
#[test]
fn am_source_status_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_source_status_t>(), 40);
    assert_eq!(core::mem::align_of::<am_source_status_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_source_status_t, struct_size),
        0,
        "am_source_status_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_status_t, state),
        4,
        "am_source_status_t.state"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_status_t, source_id),
        8,
        "am_source_status_t.source_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_status_t, generation),
        16,
        "am_source_status_t.generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_status_t, config_generation),
        20,
        "am_source_status_t.config_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_status_t, fault_class),
        24,
        "am_source_status_t.fault_class"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_status_t, reconnect_attempts),
        28,
        "am_source_status_t.reconnect_attempts"
    );
    assert_eq!(
        core::mem::offset_of!(am_source_status_t, last_frame_at_ms),
        32,
        "am_source_status_t.last_frame_at_ms"
    );
}
#[test]
fn am_reconnect_policy_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_reconnect_policy_t>(), 20);
    assert_eq!(core::mem::align_of::<am_reconnect_policy_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_reconnect_policy_t, struct_size),
        0,
        "am_reconnect_policy_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_reconnect_policy_t, initial_backoff_ms),
        4,
        "am_reconnect_policy_t.initial_backoff_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_reconnect_policy_t, max_backoff_ms),
        8,
        "am_reconnect_policy_t.max_backoff_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_reconnect_policy_t, jitter_ms),
        12,
        "am_reconnect_policy_t.jitter_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_reconnect_policy_t, max_attempts),
        16,
        "am_reconnect_policy_t.max_attempts"
    );
}
#[test]
fn am_probe_candidate_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_probe_candidate_t>(), 64);
    assert_eq!(core::mem::align_of::<am_probe_candidate_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_probe_candidate_t, struct_size),
        0,
        "am_probe_candidate_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_candidate_t, reference_utf8),
        8,
        "am_probe_candidate_t.reference_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_candidate_t, status),
        16,
        "am_probe_candidate_t.status"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_candidate_t, codec_utf8),
        24,
        "am_probe_candidate_t.codec_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_candidate_t, width),
        32,
        "am_probe_candidate_t.width"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_candidate_t, height),
        36,
        "am_probe_candidate_t.height"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_candidate_t, fps),
        40,
        "am_probe_candidate_t.fps"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_candidate_t, observed_at_ms),
        48,
        "am_probe_candidate_t.observed_at_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_candidate_t, valid_until_ms),
        56,
        "am_probe_candidate_t.valid_until_ms"
    );
}
#[test]
fn am_probe_request_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_probe_request_t>(), 32);
    assert_eq!(core::mem::align_of::<am_probe_request_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_probe_request_t, struct_size),
        0,
        "am_probe_request_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_request_t, candidate_count),
        4,
        "am_probe_request_t.candidate_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_request_t, references_utf8),
        8,
        "am_probe_request_t.references_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_request_t, creds),
        16,
        "am_probe_request_t.creds"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_request_t, force_refresh),
        24,
        "am_probe_request_t.force_refresh"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_request_t, timeout_ms),
        28,
        "am_probe_request_t.timeout_ms"
    );
}
