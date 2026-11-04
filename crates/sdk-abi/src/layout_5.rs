// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
#[test]
fn am_backend_actual_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_backend_actual_t>(), 32);
    assert_eq!(core::mem::align_of::<am_backend_actual_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_backend_actual_t, actual),
        0,
        "am_backend_actual_t.actual"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_actual_t, device_utf8),
        8,
        "am_backend_actual_t.device_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_actual_t, subgraph_cpu_nodes),
        16,
        "am_backend_actual_t.subgraph_cpu_nodes"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_actual_t, fallback_reason_utf8),
        24,
        "am_backend_actual_t.fallback_reason_utf8"
    );
}
#[test]
fn am_model_identity_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_model_identity_t>(), 24);
    assert_eq!(core::mem::align_of::<am_model_identity_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_model_identity_t, model_uid),
        0,
        "am_model_identity_t.model_uid"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_identity_t, model_version),
        8,
        "am_model_identity_t.model_version"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_identity_t, profile_id),
        16,
        "am_model_identity_t.profile_id"
    );
}
#[test]
fn am_model_info_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_model_info_t>(), 104);
    assert_eq!(core::mem::align_of::<am_model_info_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, struct_size),
        0,
        "am_model_info_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, identity),
        8,
        "am_model_info_t.identity"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, load_mode),
        32,
        "am_model_info_t.load_mode"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, input_count),
        36,
        "am_model_info_t.input_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, output_count),
        40,
        "am_model_info_t.output_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, inputs),
        48,
        "am_model_info_t.inputs"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, outputs),
        56,
        "am_model_info_t.outputs"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, native_batch),
        64,
        "am_model_info_t.native_batch"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, execution),
        72,
        "am_model_info_t.execution"
    );
}
#[test]
fn am_frame_assoc_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_frame_assoc_t>(), 80);
    assert_eq!(core::mem::align_of::<am_frame_assoc_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, struct_size),
        0,
        "am_frame_assoc_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, source_id),
        8,
        "am_frame_assoc_t.source_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, frame_id),
        16,
        "am_frame_assoc_t.frame_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, source_generation),
        24,
        "am_frame_assoc_t.source_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, config_generation),
        28,
        "am_frame_assoc_t.config_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, pts),
        32,
        "am_frame_assoc_t.pts"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, time_base_num),
        40,
        "am_frame_assoc_t.time_base_num"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, time_base_den),
        44,
        "am_frame_assoc_t.time_base_den"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, captured_at_ms),
        48,
        "am_frame_assoc_t.captured_at_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, received_at_ms),
        56,
        "am_frame_assoc_t.received_at_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, clock_mask),
        64,
        "am_frame_assoc_t.clock_mask"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, clock_quality),
        68,
        "am_frame_assoc_t.clock_quality"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_assoc_t, valid),
        72,
        "am_frame_assoc_t.valid"
    );
}
#[test]
fn am_gpu_frame_desc_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_gpu_frame_desc_t>(), 120);
    assert_eq!(core::mem::align_of::<am_gpu_frame_desc_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_desc_t, struct_size),
        0,
        "am_gpu_frame_desc_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_desc_t, width),
        4,
        "am_gpu_frame_desc_t.width"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_desc_t, height),
        8,
        "am_gpu_frame_desc_t.height"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_desc_t, d3d_format),
        12,
        "am_gpu_frame_desc_t.d3d_format"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_desc_t, subresource),
        16,
        "am_gpu_frame_desc_t.subresource"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_desc_t, shared_nt_handle),
        24,
        "am_gpu_frame_desc_t.shared_nt_handle"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_desc_t, flags),
        32,
        "am_gpu_frame_desc_t.flags"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_desc_t, assoc),
        40,
        "am_gpu_frame_desc_t.assoc"
    );
}
#[test]
fn am_gpu_frame_stats_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_gpu_frame_stats_t>(), 32);
    assert_eq!(core::mem::align_of::<am_gpu_frame_stats_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_stats_t, struct_size),
        0,
        "am_gpu_frame_stats_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_stats_t, gpu_frames_delivered),
        8,
        "am_gpu_frame_stats_t.gpu_frames_delivered"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_stats_t, cpu_fallbacks),
        16,
        "am_gpu_frame_stats_t.cpu_fallbacks"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_stats_t, bytes_transferred_est),
        24,
        "am_gpu_frame_stats_t.bytes_transferred_est"
    );
}
#[test]
fn am_gpu_frame_ref_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_gpu_frame_ref_t>(), 56);
    assert_eq!(core::mem::align_of::<am_gpu_frame_ref_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_ref_t, struct_size),
        0,
        "am_gpu_frame_ref_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_ref_t, width),
        4,
        "am_gpu_frame_ref_t.width"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_ref_t, height),
        8,
        "am_gpu_frame_ref_t.height"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_ref_t, d3d_format),
        12,
        "am_gpu_frame_ref_t.d3d_format"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_ref_t, subresource),
        16,
        "am_gpu_frame_ref_t.subresource"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_ref_t, shared_nt_handle),
        24,
        "am_gpu_frame_ref_t.shared_nt_handle"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_ref_t, texture),
        32,
        "am_gpu_frame_ref_t.texture"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_ref_t, flags),
        40,
        "am_gpu_frame_ref_t.flags"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_ref_t, reserved),
        44,
        "am_gpu_frame_ref_t.reserved"
    );
    assert_eq!(
        core::mem::offset_of!(am_gpu_frame_ref_t, assoc),
        48,
        "am_gpu_frame_ref_t.assoc"
    );
}
#[test]
fn am_image_input_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_image_input_t>(), 56);
    assert_eq!(core::mem::align_of::<am_image_input_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_image_input_t, struct_size),
        0,
        "am_image_input_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_image_input_t, kind),
        4,
        "am_image_input_t.kind"
    );
    assert_eq!(
        core::mem::offset_of!(am_image_input_t, encoded),
        8,
        "am_image_input_t.encoded"
    );
    assert_eq!(
        core::mem::offset_of!(am_image_input_t, encoded_size),
        16,
        "am_image_input_t.encoded_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_image_input_t, view),
        24,
        "am_image_input_t.view"
    );
    assert_eq!(
        core::mem::offset_of!(am_image_input_t, assoc),
        32,
        "am_image_input_t.assoc"
    );
    assert_eq!(
        core::mem::offset_of!(am_image_input_t, flags),
        40,
        "am_image_input_t.flags"
    );
    assert_eq!(
        core::mem::offset_of!(am_image_input_t, gpu),
        48,
        "am_image_input_t.gpu"
    );
}
#[test]
fn am_cascade_rule_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_cascade_rule_t>(), 16);
    assert_eq!(core::mem::align_of::<am_cascade_rule_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_cascade_rule_t, struct_size),
        0,
        "am_cascade_rule_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_cascade_rule_t, source_class_id),
        4,
        "am_cascade_rule_t.source_class_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_cascade_rule_t, expand),
        8,
        "am_cascade_rule_t.expand"
    );
    assert_eq!(
        core::mem::offset_of!(am_cascade_rule_t, min_roi_size),
        12,
        "am_cascade_rule_t.min_roi_size"
    );
}
#[test]
fn am_cascade_request_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_cascade_request_t>(), 40);
    assert_eq!(core::mem::align_of::<am_cascade_request_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_cascade_request_t, struct_size),
        0,
        "am_cascade_request_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_cascade_request_t, static_rois),
        8,
        "am_cascade_request_t.static_rois"
    );
    assert_eq!(
        core::mem::offset_of!(am_cascade_request_t, static_roi_count),
        16,
        "am_cascade_request_t.static_roi_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_cascade_request_t, rules),
        24,
        "am_cascade_request_t.rules"
    );
    assert_eq!(
        core::mem::offset_of!(am_cascade_request_t, rule_count),
        32,
        "am_cascade_request_t.rule_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_cascade_request_t, secondary_confidence),
        36,
        "am_cascade_request_t.secondary_confidence"
    );
}
