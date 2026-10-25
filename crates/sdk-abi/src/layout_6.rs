// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
#[test]
fn am_detect_box_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_detect_box_t>(), 32);
    assert_eq!(core::mem::align_of::<am_detect_box_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_detect_box_t, bounds),
        0,
        "am_detect_box_t.bounds"
    );
    assert_eq!(
        core::mem::offset_of!(am_detect_box_t, confidence),
        16,
        "am_detect_box_t.confidence"
    );
    assert_eq!(
        core::mem::offset_of!(am_detect_box_t, class_id),
        20,
        "am_detect_box_t.class_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_detect_box_t, class_name_utf8),
        24,
        "am_detect_box_t.class_name_utf8"
    );
}
#[test]
fn am_detect_result_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_detect_result_t>(), 16);
    assert_eq!(core::mem::align_of::<am_detect_result_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_detect_result_t, count),
        0,
        "am_detect_result_t.count"
    );
    assert_eq!(
        core::mem::offset_of!(am_detect_result_t, boxes),
        8,
        "am_detect_result_t.boxes"
    );
}
#[test]
fn am_classify_entry_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_classify_entry_t>(), 32);
    assert_eq!(core::mem::align_of::<am_classify_entry_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_classify_entry_t, class_id),
        0,
        "am_classify_entry_t.class_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_classify_entry_t, class_name_utf8),
        8,
        "am_classify_entry_t.class_name_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_classify_entry_t, logit),
        16,
        "am_classify_entry_t.logit"
    );
    assert_eq!(
        core::mem::offset_of!(am_classify_entry_t, probability),
        20,
        "am_classify_entry_t.probability"
    );
    assert_eq!(
        core::mem::offset_of!(am_classify_entry_t, has_probability),
        24,
        "am_classify_entry_t.has_probability"
    );
}
#[test]
fn am_classify_result_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_classify_result_t>(), 16);
    assert_eq!(core::mem::align_of::<am_classify_result_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_classify_result_t, count),
        0,
        "am_classify_result_t.count"
    );
    assert_eq!(
        core::mem::offset_of!(am_classify_result_t, entries),
        8,
        "am_classify_result_t.entries"
    );
}
#[test]
fn am_roi_map_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_roi_map_t>(), 20);
    assert_eq!(core::mem::align_of::<am_roi_map_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_roi_map_t, scale_x),
        0,
        "am_roi_map_t.scale_x"
    );
    assert_eq!(
        core::mem::offset_of!(am_roi_map_t, scale_y),
        4,
        "am_roi_map_t.scale_y"
    );
    assert_eq!(
        core::mem::offset_of!(am_roi_map_t, offset_x),
        8,
        "am_roi_map_t.offset_x"
    );
    assert_eq!(
        core::mem::offset_of!(am_roi_map_t, offset_y),
        12,
        "am_roi_map_t.offset_y"
    );
    assert_eq!(
        core::mem::offset_of!(am_roi_map_t, valid),
        16,
        "am_roi_map_t.valid"
    );
}
#[test]
fn am_segment_instance_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_segment_instance_t>(), 72);
    assert_eq!(core::mem::align_of::<am_segment_instance_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_segment_instance_t, bounds),
        0,
        "am_segment_instance_t.bounds"
    );
    assert_eq!(
        core::mem::offset_of!(am_segment_instance_t, confidence),
        16,
        "am_segment_instance_t.confidence"
    );
    assert_eq!(
        core::mem::offset_of!(am_segment_instance_t, class_id),
        20,
        "am_segment_instance_t.class_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_segment_instance_t, mask_width),
        24,
        "am_segment_instance_t.mask_width"
    );
    assert_eq!(
        core::mem::offset_of!(am_segment_instance_t, mask_height),
        28,
        "am_segment_instance_t.mask_height"
    );
    assert_eq!(
        core::mem::offset_of!(am_segment_instance_t, mask_stride_bytes),
        32,
        "am_segment_instance_t.mask_stride_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_segment_instance_t, mask_bits),
        40,
        "am_segment_instance_t.mask_bits"
    );
    assert_eq!(
        core::mem::offset_of!(am_segment_instance_t, roi_map),
        48,
        "am_segment_instance_t.roi_map"
    );
}
#[test]
fn am_segment_result_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_segment_result_t>(), 16);
    assert_eq!(core::mem::align_of::<am_segment_result_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_segment_result_t, count),
        0,
        "am_segment_result_t.count"
    );
    assert_eq!(
        core::mem::offset_of!(am_segment_result_t, instances),
        8,
        "am_segment_result_t.instances"
    );
}
#[test]
fn am_obb_instance_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_obb_instance_t>(), 64);
    assert_eq!(core::mem::align_of::<am_obb_instance_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_obb_instance_t, vertices_xy),
        0,
        "am_obb_instance_t.vertices_xy"
    );
    assert_eq!(
        core::mem::offset_of!(am_obb_instance_t, center_x),
        32,
        "am_obb_instance_t.center_x"
    );
    assert_eq!(
        core::mem::offset_of!(am_obb_instance_t, center_y),
        36,
        "am_obb_instance_t.center_y"
    );
    assert_eq!(
        core::mem::offset_of!(am_obb_instance_t, width),
        40,
        "am_obb_instance_t.width"
    );
    assert_eq!(
        core::mem::offset_of!(am_obb_instance_t, height),
        44,
        "am_obb_instance_t.height"
    );
    assert_eq!(
        core::mem::offset_of!(am_obb_instance_t, angle),
        48,
        "am_obb_instance_t.angle"
    );
    assert_eq!(
        core::mem::offset_of!(am_obb_instance_t, confidence),
        52,
        "am_obb_instance_t.confidence"
    );
    assert_eq!(
        core::mem::offset_of!(am_obb_instance_t, class_id),
        56,
        "am_obb_instance_t.class_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_obb_instance_t, angle_normalized),
        60,
        "am_obb_instance_t.angle_normalized"
    );
}
#[test]
fn am_obb_result_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_obb_result_t>(), 16);
    assert_eq!(core::mem::align_of::<am_obb_result_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_obb_result_t, count),
        0,
        "am_obb_result_t.count"
    );
    assert_eq!(
        core::mem::offset_of!(am_obb_result_t, instances),
        8,
        "am_obb_result_t.instances"
    );
}
#[test]
fn amv_result_t_matches_c() {
    assert_eq!(core::mem::size_of::<amv_result_t>(), 208);
    assert_eq!(core::mem::align_of::<amv_result_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(amv_result_t, struct_size),
        0,
        "amv_result_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, request_id),
        8,
        "amv_result_t.request_id"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, task),
        16,
        "amv_result_t.task"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, frame),
        24,
        "amv_result_t.frame"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, model),
        32,
        "amv_result_t.model"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, execution),
        56,
        "amv_result_t.execution"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, transform),
        88,
        "amv_result_t.transform"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, pose),
        96,
        "amv_result_t.pose"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, detect),
        112,
        "amv_result_t.detect"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, classify),
        128,
        "amv_result_t.classify"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, segment),
        144,
        "amv_result_t.segment"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, obb),
        160,
        "amv_result_t.obb"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, raw_output_count),
        176,
        "amv_result_t.raw_output_count"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, raw_outputs),
        184,
        "amv_result_t.raw_outputs"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, infer_start_ms),
        192,
        "amv_result_t.infer_start_ms"
    );
    assert_eq!(
        core::mem::offset_of!(amv_result_t, infer_end_ms),
        200,
        "amv_result_t.infer_end_ms"
    );
}
