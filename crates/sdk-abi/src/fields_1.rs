// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
impl AbiLayout for am_shared_pool_config_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_shared_pool_config_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_config_t, slot_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_config_t, max_frame_bytes),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_config_t, total_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_config_t, max_consumers),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_config_t, max_outstanding_borrows),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_config_t, drop_policy),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_config_t, publish_wait_ms),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_config_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
    ];
}
impl AbiLayout for am_shared_pool_attach_desc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_shared_pool_attach_desc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_attach_desc_t, protocol_version),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_attach_desc_t, pool_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_attach_desc_t, owner_pid),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_attach_desc_t, owner_started_at_100ns),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_attach_desc_t, pool_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_attach_desc_t, identity_tag),
            core::mem::size_of::<[u8; 32]>(),
        ),
    ];
}
impl AbiLayout for am_shared_frame_view_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_shared_frame_view_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, protocol_version),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, pixel_format),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, width),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, height),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, plane_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, stride_bytes),
            core::mem::size_of::<[u32; 4]>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, offset_bytes),
            core::mem::size_of::<[u32; 4]>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, plane_valid_bytes),
            core::mem::size_of::<[u32; 4]>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, color_matrix),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, color_range),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, publication_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, frame_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, source_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, source_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, config_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, assoc_valid),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, pool_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, time_flags),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, pts),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, time_base_num),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, time_base_den),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, captured_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, received_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, clock_quality),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, desc)
                + core::mem::offset_of!(am_shared_frame_desc_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, plane_data),
            core::mem::size_of::<[*const c_void; 4]>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_view_t, borrow_id),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for am_shared_pool_stats_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_shared_pool_stats_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_stats_t, published),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_stats_t, dropped_stale),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_stats_t, dropped_full),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_stats_t, wait_timeouts),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_stats_t, slots_in_use),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_stats_t, slots_borrowed),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_stats_t, outstanding_borrows),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_stats_t, consumers_attached),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_stats_t, bytes_in_use),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_stats_t, metadata_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_pool_stats_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
    ];
}
impl AbiLayout for amc_capability_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(amc_capability_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amc_capability_t, abi_minor),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amc_capability_t, feature_bits),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amc_capability_t, pixel_formats),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amc_capability_t, max_sources),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amc_capability_t, max_decoders),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amc_capability_t, max_subscribers_per_source),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amc_capability_t, frame_budget_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(amc_capability_t, shared_pool_state),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_source_desc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_source_desc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_source_desc_t, kind),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_source_desc_t, url_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_source_desc_t, stream_kind_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_source_desc_t, creds),
            core::mem::size_of::<*const am_credential_data_t>(),
        ),
        (
            core::mem::offset_of!(am_source_desc_t, provider),
            core::mem::size_of::<*const am_credential_provider_t>(),
        ),
        (
            core::mem::offset_of!(am_source_desc_t, reuse),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_source_desc_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
    ];
}
impl AbiLayout for am_source_status_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_source_status_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_source_status_t, state),
            core::mem::size_of::<am_source_state_e>(),
        ),
        (
            core::mem::offset_of!(am_source_status_t, source_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_source_status_t, generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_source_status_t, config_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_source_status_t, fault_class),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_source_status_t, reconnect_attempts),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_source_status_t, last_frame_at_ms),
            core::mem::size_of::<i64>(),
        ),
    ];
}
impl AbiLayout for am_reconnect_policy_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_reconnect_policy_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_reconnect_policy_t, initial_backoff_ms),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_reconnect_policy_t, max_backoff_ms),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_reconnect_policy_t, jitter_ms),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_reconnect_policy_t, max_attempts),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_probe_candidate_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_probe_candidate_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_probe_candidate_t, reference_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_probe_candidate_t, status),
            core::mem::size_of::<am_probe_status_e>(),
        ),
        (
            core::mem::offset_of!(am_probe_candidate_t, codec_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_probe_candidate_t, width),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_probe_candidate_t, height),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_probe_candidate_t, fps),
            core::mem::size_of::<f64>(),
        ),
        (
            core::mem::offset_of!(am_probe_candidate_t, observed_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_probe_candidate_t, valid_until_ms),
            core::mem::size_of::<i64>(),
        ),
    ];
}
impl AbiLayout for am_probe_request_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_probe_request_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_probe_request_t, candidate_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_probe_request_t, references_utf8),
            core::mem::size_of::<*const *const c_char>(),
        ),
        (
            core::mem::offset_of!(am_probe_request_t, creds),
            core::mem::size_of::<*const am_credential_data_t>(),
        ),
        (
            core::mem::offset_of!(am_probe_request_t, force_refresh),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_probe_request_t, timeout_ms),
            core::mem::size_of::<u32>(),
        ),
    ];
}
