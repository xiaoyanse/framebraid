// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
#[test]
fn am_decoder_status_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_decoder_status_t>(), 32);
    assert_eq!(core::mem::align_of::<am_decoder_status_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_decoder_status_t, struct_size),
        0,
        "am_decoder_status_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_status_t, state),
        4,
        "am_decoder_status_t.state"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_status_t, decoder_generation),
        8,
        "am_decoder_status_t.decoder_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_status_t, config_generation),
        12,
        "am_decoder_status_t.config_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_status_t, queue_items),
        16,
        "am_decoder_status_t.queue_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_status_t, queue_bytes),
        24,
        "am_decoder_status_t.queue_bytes"
    );
}
#[test]
fn am_decoder_info_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_decoder_info_t>(), 40);
    assert_eq!(core::mem::align_of::<am_decoder_info_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_decoder_info_t, struct_size),
        0,
        "am_decoder_info_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_info_t, codec),
        4,
        "am_decoder_info_t.codec"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_info_t, bitstream_format),
        8,
        "am_decoder_info_t.bitstream_format"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_info_t, output_pixel_format),
        12,
        "am_decoder_info_t.output_pixel_format"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_info_t, hw_policy),
        16,
        "am_decoder_info_t.hw_policy"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_info_t, hw_accelerated),
        20,
        "am_decoder_info_t.hw_accelerated"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_info_t, backend_name_utf8),
        24,
        "am_decoder_info_t.backend_name_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_info_t, config_generation),
        32,
        "am_decoder_info_t.config_generation"
    );
}
#[test]
fn am_capture_request_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_capture_request_t>(), 28);
    assert_eq!(core::mem::align_of::<am_capture_request_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_capture_request_t, struct_size),
        0,
        "am_capture_request_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_request_t, output_kind),
        4,
        "am_capture_request_t.output_kind"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_request_t, deadline_ms),
        8,
        "am_capture_request_t.deadline_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_request_t, reserved),
        12,
        "am_capture_request_t.reserved"
    );
}
#[test]
fn am_captured_image_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_captured_image_t>(), 56);
    assert_eq!(core::mem::align_of::<am_captured_image_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_captured_image_t, struct_size),
        0,
        "am_captured_image_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_captured_image_t, request_id),
        8,
        "am_captured_image_t.request_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_captured_image_t, output_kind),
        16,
        "am_captured_image_t.output_kind"
    );
    assert_eq!(
        core::mem::offset_of!(am_captured_image_t, encoded),
        24,
        "am_captured_image_t.encoded"
    );
    assert_eq!(
        core::mem::offset_of!(am_captured_image_t, encoded_size),
        32,
        "am_captured_image_t.encoded_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_captured_image_t, view),
        40,
        "am_captured_image_t.view"
    );
    assert_eq!(
        core::mem::offset_of!(am_captured_image_t, meta),
        48,
        "am_captured_image_t.meta"
    );
}
#[test]
fn am_capture_continuous_desc_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_capture_continuous_desc_t>(), 56);
    assert_eq!(core::mem::align_of::<am_capture_continuous_desc_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_capture_continuous_desc_t, struct_size),
        0,
        "am_capture_continuous_desc_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_continuous_desc_t, target_interval_ms),
        4,
        "am_capture_continuous_desc_t.target_interval_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_continuous_desc_t, max_inflight),
        8,
        "am_capture_continuous_desc_t.max_inflight"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_continuous_desc_t, request_timeout_ms),
        12,
        "am_capture_continuous_desc_t.request_timeout_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_continuous_desc_t, output_kind),
        16,
        "am_capture_continuous_desc_t.output_kind"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_continuous_desc_t, mode),
        20,
        "am_capture_continuous_desc_t.mode"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_continuous_desc_t, queue_max_items),
        24,
        "am_capture_continuous_desc_t.queue_max_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_continuous_desc_t, queue_max_bytes),
        32,
        "am_capture_continuous_desc_t.queue_max_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_continuous_desc_t, reserved),
        40,
        "am_capture_continuous_desc_t.reserved"
    );
}
#[test]
fn am_capture_stats_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_capture_stats_t>(), 56);
    assert_eq!(core::mem::align_of::<am_capture_stats_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_capture_stats_t, struct_size),
        0,
        "am_capture_stats_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_stats_t, actual_fps),
        8,
        "am_capture_stats_t.actual_fps"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_stats_t, last_received_at_ms),
        16,
        "am_capture_stats_t.last_received_at_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_stats_t, consecutive_failures),
        24,
        "am_capture_stats_t.consecutive_failures"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_stats_t, newest_age_ms),
        32,
        "am_capture_stats_t.newest_age_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_stats_t, skipped_beats),
        40,
        "am_capture_stats_t.skipped_beats"
    );
    assert_eq!(
        core::mem::offset_of!(am_capture_stats_t, shared_consumers),
        48,
        "am_capture_stats_t.shared_consumers"
    );
}
#[test]
fn am_event_detail_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_event_detail_t>(), 40);
    assert_eq!(core::mem::align_of::<am_event_detail_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_event_detail_t, struct_size),
        0,
        "am_event_detail_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_event_detail_t, generation),
        4,
        "am_event_detail_t.generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_event_detail_t, scope),
        8,
        "am_event_detail_t.scope"
    );
    assert_eq!(
        core::mem::offset_of!(am_event_detail_t, affected_handle),
        16,
        "am_event_detail_t.affected_handle"
    );
    assert_eq!(
        core::mem::offset_of!(am_event_detail_t, fault_class),
        24,
        "am_event_detail_t.fault_class"
    );
    assert_eq!(
        core::mem::offset_of!(am_event_detail_t, reason_utf8),
        32,
        "am_event_detail_t.reason_utf8"
    );
}
#[test]
fn amv_capability_t_matches_c() {
    assert_eq!(core::mem::size_of::<amv_capability_t>(), 48);
    assert_eq!(core::mem::align_of::<amv_capability_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(amv_capability_t, struct_size),
        0,
        "amv_capability_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(amv_capability_t, abi_minor),
        4,
        "amv_capability_t.abi_minor"
    );
    assert_eq!(
        core::mem::offset_of!(amv_capability_t, feature_bits),
        8,
        "amv_capability_t.feature_bits"
    );
    assert_eq!(
        core::mem::offset_of!(amv_capability_t, dtype_bits),
        12,
        "amv_capability_t.dtype_bits"
    );
    assert_eq!(
        core::mem::offset_of!(amv_capability_t, max_batch_items),
        16,
        "amv_capability_t.max_batch_items"
    );
    assert_eq!(
        core::mem::offset_of!(amv_capability_t, max_models),
        20,
        "amv_capability_t.max_models"
    );
    assert_eq!(
        core::mem::offset_of!(amv_capability_t, infer_parallelism),
        24,
        "amv_capability_t.infer_parallelism"
    );
    assert_eq!(
        core::mem::offset_of!(amv_capability_t, result_retention_default_ms),
        32,
        "amv_capability_t.result_retention_default_ms"
    );
    assert_eq!(
        core::mem::offset_of!(amv_capability_t, shared_pool_state),
        40,
        "amv_capability_t.shared_pool_state"
    );
}
#[test]
fn am_backend_row_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_backend_row_t>(), 32);
    assert_eq!(core::mem::align_of::<am_backend_row_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_backend_row_t, backend),
        0,
        "am_backend_row_t.backend"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_row_t, hw_detected),
        4,
        "am_backend_row_t.hw_detected"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_row_t, deps_present),
        8,
        "am_backend_row_t.deps_present"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_row_t, provider_loadable),
        12,
        "am_backend_row_t.provider_loadable"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_row_t, verified),
        16,
        "am_backend_row_t.verified"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_row_t, note_utf8),
        24,
        "am_backend_row_t.note_utf8"
    );
}
#[test]
fn am_backend_matrix_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_backend_matrix_t>(), 16);
    assert_eq!(core::mem::align_of::<am_backend_matrix_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_backend_matrix_t, struct_size),
        0,
        "am_backend_matrix_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_matrix_t, row_count),
        4,
        "am_backend_matrix_t.row_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_matrix_t, rows),
        8,
        "am_backend_matrix_t.rows"
    );
}
