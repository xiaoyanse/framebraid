// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
impl AbiLayout for am_decoder_status_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_decoder_status_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_status_t, state),
            core::mem::size_of::<am_decoder_state_e>(),
        ),
        (
            core::mem::offset_of!(am_decoder_status_t, decoder_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_status_t, config_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_status_t, queue_items),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_status_t, queue_bytes),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for am_decoder_info_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_decoder_info_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_info_t, codec),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_info_t, bitstream_format),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_info_t, output_pixel_format),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_info_t, hw_policy),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_info_t, hw_accelerated),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_info_t, backend_name_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_decoder_info_t, config_generation),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_capture_request_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_capture_request_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_request_t, output_kind),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_request_t, deadline_ms),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_request_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
    ];
}
impl AbiLayout for am_captured_image_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_captured_image_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_captured_image_t, request_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_captured_image_t, output_kind),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_captured_image_t, encoded),
            core::mem::size_of::<*const u8>(),
        ),
        (
            core::mem::offset_of!(am_captured_image_t, encoded_size),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_captured_image_t, view),
            core::mem::size_of::<*const am_frame_view_t>(),
        ),
        (
            core::mem::offset_of!(am_captured_image_t, meta),
            core::mem::size_of::<*const am_frame_meta_t>(),
        ),
    ];
}
impl AbiLayout for am_capture_continuous_desc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_capture_continuous_desc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_continuous_desc_t, target_interval_ms),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_continuous_desc_t, max_inflight),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_continuous_desc_t, request_timeout_ms),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_continuous_desc_t, output_kind),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_continuous_desc_t, mode),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_continuous_desc_t, queue_max_items),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_continuous_desc_t, queue_max_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_capture_continuous_desc_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
    ];
}
impl AbiLayout for am_capture_stats_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_capture_stats_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_stats_t, actual_fps),
            core::mem::size_of::<f64>(),
        ),
        (
            core::mem::offset_of!(am_capture_stats_t, last_received_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_capture_stats_t, consecutive_failures),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_capture_stats_t, newest_age_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_capture_stats_t, skipped_beats),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_capture_stats_t, shared_consumers),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for am_event_detail_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_event_detail_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_event_detail_t, generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_event_detail_t, scope),
            core::mem::size_of::<am_event_scope_e>(),
        ),
        (
            core::mem::offset_of!(am_event_detail_t, affected_handle),
            core::mem::size_of::<*mut c_void>(),
        ),
        (
            core::mem::offset_of!(am_event_detail_t, fault_class),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_event_detail_t, reason_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
    ];
}
impl AbiLayout for amv_capability_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(amv_capability_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_capability_t, abi_minor),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_capability_t, feature_bits),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_capability_t, dtype_bits),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_capability_t, max_batch_items),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_capability_t, max_models),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_capability_t, infer_parallelism),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_capability_t, result_retention_default_ms),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(amv_capability_t, shared_pool_state),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_backend_row_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_backend_row_t, backend),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_row_t, hw_detected),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_row_t, deps_present),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_row_t, provider_loadable),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_row_t, verified),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_row_t, note_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
    ];
}
impl AbiLayout for am_backend_matrix_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_backend_matrix_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_matrix_t, row_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_matrix_t, rows),
            core::mem::size_of::<*const am_backend_row_t>(),
        ),
    ];
}
