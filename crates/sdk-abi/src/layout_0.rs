// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
#[test]
fn am_version_info_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_version_info_t>(), 40);
    assert_eq!(core::mem::align_of::<am_version_info_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_version_info_t, struct_size),
        0,
        "am_version_info_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_version_info_t, abi_major),
        4,
        "am_version_info_t.abi_major"
    );
    assert_eq!(
        core::mem::offset_of!(am_version_info_t, abi_minor),
        8,
        "am_version_info_t.abi_minor"
    );
    assert_eq!(
        core::mem::offset_of!(am_version_info_t, package_version_utf8),
        16,
        "am_version_info_t.package_version_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_version_info_t, build_target_utf8),
        24,
        "am_version_info_t.build_target_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_version_info_t, runtime_notes_utf8),
        32,
        "am_version_info_t.runtime_notes_utf8"
    );
}
#[test]
fn amc_init_params_t_matches_c() {
    assert_eq!(core::mem::size_of::<amc_init_params_t>(), 48);
    assert_eq!(core::mem::align_of::<amc_init_params_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(amc_init_params_t, struct_size),
        0,
        "amc_init_params_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(amc_init_params_t, abi_version),
        4,
        "amc_init_params_t.abi_version"
    );
    assert_eq!(
        core::mem::offset_of!(amc_init_params_t, runtime_dirs_utf8),
        8,
        "amc_init_params_t.runtime_dirs_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(amc_init_params_t, runtime_dir_count),
        16,
        "amc_init_params_t.runtime_dir_count"
    );
    assert_eq!(
        core::mem::offset_of!(amc_init_params_t, reserved),
        20,
        "amc_init_params_t.reserved"
    );
    assert_eq!(
        core::mem::offset_of!(amc_init_params_t, feature_flags),
        40,
        "amc_init_params_t.feature_flags"
    );
}
#[test]
fn amv_init_params_t_matches_c() {
    assert_eq!(core::mem::size_of::<amv_init_params_t>(), 56);
    assert_eq!(core::mem::align_of::<amv_init_params_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(amv_init_params_t, struct_size),
        0,
        "amv_init_params_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(amv_init_params_t, abi_version),
        4,
        "amv_init_params_t.abi_version"
    );
    assert_eq!(
        core::mem::offset_of!(amv_init_params_t, runtime_dirs_utf8),
        8,
        "amv_init_params_t.runtime_dirs_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(amv_init_params_t, runtime_dir_count),
        16,
        "amv_init_params_t.runtime_dir_count"
    );
    assert_eq!(
        core::mem::offset_of!(amv_init_params_t, infer_threads),
        20,
        "amv_init_params_t.infer_threads"
    );
    assert_eq!(
        core::mem::offset_of!(amv_init_params_t, domain_infer_slots),
        24,
        "amv_init_params_t.domain_infer_slots"
    );
    assert_eq!(
        core::mem::offset_of!(amv_init_params_t, total_thread_budget),
        32,
        "amv_init_params_t.total_thread_budget"
    );
    assert_eq!(
        core::mem::offset_of!(amv_init_params_t, reserved),
        36,
        "amv_init_params_t.reserved"
    );
    assert_eq!(
        core::mem::offset_of!(amv_init_params_t, feature_flags),
        48,
        "amv_init_params_t.feature_flags"
    );
}
#[test]
fn am_error_info_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_error_info_t>(), 48);
    assert_eq!(core::mem::align_of::<am_error_info_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_error_info_t, struct_size),
        0,
        "am_error_info_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_error_info_t, code),
        4,
        "am_error_info_t.code"
    );
    assert_eq!(
        core::mem::offset_of!(am_error_info_t, message_utf8),
        8,
        "am_error_info_t.message_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_error_info_t, source_utf8),
        16,
        "am_error_info_t.source_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_error_info_t, retryable),
        24,
        "am_error_info_t.retryable"
    );
    assert_eq!(
        core::mem::offset_of!(am_error_info_t, fault_class),
        28,
        "am_error_info_t.fault_class"
    );
    assert_eq!(
        core::mem::offset_of!(am_error_info_t, reserved),
        32,
        "am_error_info_t.reserved"
    );
}
#[test]
fn am_init_error_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_init_error_t>(), 288);
    assert_eq!(core::mem::align_of::<am_init_error_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_init_error_t, struct_size),
        0,
        "am_init_error_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_init_error_t, code),
        4,
        "am_init_error_t.code"
    );
    assert_eq!(
        core::mem::offset_of!(am_init_error_t, message),
        8,
        "am_init_error_t.message"
    );
    assert_eq!(
        core::mem::offset_of!(am_init_error_t, source_utf8),
        264,
        "am_init_error_t.source_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_init_error_t, failed_at_ms),
        272,
        "am_init_error_t.failed_at_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_init_error_t, attempt_seq),
        280,
        "am_init_error_t.attempt_seq"
    );
}
#[test]
fn am_request_snapshot_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_request_snapshot_t>(), 56);
    assert_eq!(core::mem::align_of::<am_request_snapshot_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_request_snapshot_t, struct_size),
        0,
        "am_request_snapshot_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_request_snapshot_t, state),
        4,
        "am_request_snapshot_t.state"
    );
    assert_eq!(
        core::mem::offset_of!(am_request_snapshot_t, terminal),
        8,
        "am_request_snapshot_t.terminal"
    );
    assert_eq!(
        core::mem::offset_of!(am_request_snapshot_t, error),
        16,
        "am_request_snapshot_t.error"
    );
    assert_eq!(
        core::mem::offset_of!(am_request_snapshot_t, created_at_ms),
        24,
        "am_request_snapshot_t.created_at_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_request_snapshot_t, finished_at_ms),
        32,
        "am_request_snapshot_t.finished_at_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_request_snapshot_t, reserved),
        40,
        "am_request_snapshot_t.reserved"
    );
}
#[test]
fn am_queue_policy_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_queue_policy_t>(), 48);
    assert_eq!(core::mem::align_of::<am_queue_policy_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_queue_policy_t, struct_size),
        0,
        "am_queue_policy_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_queue_policy_t, max_items),
        4,
        "am_queue_policy_t.max_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_queue_policy_t, max_bytes),
        8,
        "am_queue_policy_t.max_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_queue_policy_t, max_waiters),
        16,
        "am_queue_policy_t.max_waiters"
    );
    assert_eq!(
        core::mem::offset_of!(am_queue_policy_t, wait_deadline_ms),
        20,
        "am_queue_policy_t.wait_deadline_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_queue_policy_t, ttl_ms),
        24,
        "am_queue_policy_t.ttl_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_queue_policy_t, reserved),
        28,
        "am_queue_policy_t.reserved"
    );
}
#[test]
fn am_credential_data_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_credential_data_t>(), 24);
    assert_eq!(core::mem::align_of::<am_credential_data_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_credential_data_t, struct_size),
        0,
        "am_credential_data_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_credential_data_t, username_utf8),
        8,
        "am_credential_data_t.username_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_credential_data_t, password_utf8),
        16,
        "am_credential_data_t.password_utf8"
    );
}
#[test]
fn am_credential_provider_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_credential_provider_t>(), 40);
    assert_eq!(core::mem::align_of::<am_credential_provider_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_credential_provider_t, struct_size),
        0,
        "am_credential_provider_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_credential_provider_t, fetch),
        8,
        "am_credential_provider_t.fetch"
    );
    assert_eq!(
        core::mem::offset_of!(am_credential_provider_t, on_drop),
        16,
        "am_credential_provider_t.on_drop"
    );
    assert_eq!(
        core::mem::offset_of!(am_credential_provider_t, user_data),
        24,
        "am_credential_provider_t.user_data"
    );
    assert_eq!(
        core::mem::offset_of!(am_credential_provider_t, fetch_timeout_ms),
        32,
        "am_credential_provider_t.fetch_timeout_ms"
    );
}
#[test]
fn am_shared_frame_desc_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_shared_frame_desc_t>(), 184);
    assert_eq!(core::mem::align_of::<am_shared_frame_desc_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, protocol_version),
        0,
        "am_shared_frame_desc_t.protocol_version"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, pixel_format),
        4,
        "am_shared_frame_desc_t.pixel_format"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, width),
        8,
        "am_shared_frame_desc_t.width"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, height),
        12,
        "am_shared_frame_desc_t.height"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, plane_count),
        16,
        "am_shared_frame_desc_t.plane_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, stride_bytes),
        20,
        "am_shared_frame_desc_t.stride_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, offset_bytes),
        36,
        "am_shared_frame_desc_t.offset_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, plane_valid_bytes),
        52,
        "am_shared_frame_desc_t.plane_valid_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, color_matrix),
        68,
        "am_shared_frame_desc_t.color_matrix"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, color_range),
        72,
        "am_shared_frame_desc_t.color_range"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, publication_id),
        80,
        "am_shared_frame_desc_t.publication_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, frame_id),
        88,
        "am_shared_frame_desc_t.frame_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, source_id),
        96,
        "am_shared_frame_desc_t.source_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, source_generation),
        104,
        "am_shared_frame_desc_t.source_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, config_generation),
        108,
        "am_shared_frame_desc_t.config_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, assoc_valid),
        112,
        "am_shared_frame_desc_t.assoc_valid"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, pool_generation),
        116,
        "am_shared_frame_desc_t.pool_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, time_flags),
        120,
        "am_shared_frame_desc_t.time_flags"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, pts),
        128,
        "am_shared_frame_desc_t.pts"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, time_base_num),
        136,
        "am_shared_frame_desc_t.time_base_num"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, time_base_den),
        140,
        "am_shared_frame_desc_t.time_base_den"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, captured_at_ms),
        144,
        "am_shared_frame_desc_t.captured_at_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, received_at_ms),
        152,
        "am_shared_frame_desc_t.received_at_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, clock_quality),
        160,
        "am_shared_frame_desc_t.clock_quality"
    );
    assert_eq!(
        core::mem::offset_of!(am_shared_frame_desc_t, reserved),
        164,
        "am_shared_frame_desc_t.reserved"
    );
}
