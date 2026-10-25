// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
impl AbiLayout for am_version_info_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_version_info_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_version_info_t, abi_major),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_version_info_t, abi_minor),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_version_info_t, package_version_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_version_info_t, build_target_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_version_info_t, runtime_notes_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
    ];
}
impl AbiLayout for amc_init_params_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(amc_init_params_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amc_init_params_t, abi_version),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amc_init_params_t, runtime_dirs_utf8),
            core::mem::size_of::<*const *const c_char>(),
        ),
        (
            core::mem::offset_of!(amc_init_params_t, runtime_dir_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amc_init_params_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
        (
            core::mem::offset_of!(amc_init_params_t, feature_flags),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for amv_init_params_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(amv_init_params_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_init_params_t, abi_version),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_init_params_t, runtime_dirs_utf8),
            core::mem::size_of::<*const *const c_char>(),
        ),
        (
            core::mem::offset_of!(amv_init_params_t, runtime_dir_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_init_params_t, infer_threads),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_init_params_t, domain_infer_slots),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(amv_init_params_t, total_thread_budget),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_init_params_t, reserved),
            core::mem::size_of::<[u32; 3]>(),
        ),
        (
            core::mem::offset_of!(amv_init_params_t, feature_flags),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for am_error_info_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_error_info_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_error_info_t, code),
            core::mem::size_of::<am_status_t>(),
        ),
        (
            core::mem::offset_of!(am_error_info_t, message_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_error_info_t, source_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_error_info_t, retryable),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_error_info_t, fault_class),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_error_info_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
    ];
}
impl AbiLayout for am_init_error_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_init_error_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_init_error_t, code),
            core::mem::size_of::<am_status_t>(),
        ),
        (
            core::mem::offset_of!(am_init_error_t, message),
            core::mem::size_of::<[c_char; 256]>(),
        ),
        (
            core::mem::offset_of!(am_init_error_t, source_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_init_error_t, failed_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_init_error_t, attempt_seq),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for am_request_snapshot_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_request_snapshot_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_request_snapshot_t, state),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_request_snapshot_t, terminal),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_request_snapshot_t, error),
            core::mem::size_of::<*const am_error_info_t>(),
        ),
        (
            core::mem::offset_of!(am_request_snapshot_t, created_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_request_snapshot_t, finished_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_request_snapshot_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
    ];
}
impl AbiLayout for am_queue_policy_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_queue_policy_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_queue_policy_t, max_items),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_queue_policy_t, max_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_queue_policy_t, max_waiters),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_queue_policy_t, wait_deadline_ms),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_queue_policy_t, ttl_ms),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_queue_policy_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
    ];
}
impl AbiLayout for am_credential_data_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_credential_data_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_credential_data_t, username_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_credential_data_t, password_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
    ];
}
impl AbiLayout for am_credential_provider_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_credential_provider_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_credential_provider_t, fetch),
            core::mem::size_of::<am_credential_fetch_fn>(),
        ),
        (
            core::mem::offset_of!(am_credential_provider_t, on_drop),
            core::mem::size_of::<am_credential_drop_fn>(),
        ),
        (
            core::mem::offset_of!(am_credential_provider_t, user_data),
            core::mem::size_of::<*mut c_void>(),
        ),
        (
            core::mem::offset_of!(am_credential_provider_t, fetch_timeout_ms),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_shared_frame_desc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_shared_frame_desc_t, protocol_version),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, pixel_format),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, width),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, height),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, plane_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, stride_bytes),
            core::mem::size_of::<[u32; 4]>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, offset_bytes),
            core::mem::size_of::<[u32; 4]>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, plane_valid_bytes),
            core::mem::size_of::<[u32; 4]>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, color_matrix),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, color_range),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, publication_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, frame_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, source_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, source_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, config_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, assoc_valid),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, pool_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, time_flags),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, pts),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, time_base_num),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, time_base_den),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, captured_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, received_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, clock_quality),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_shared_frame_desc_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
    ];
}
