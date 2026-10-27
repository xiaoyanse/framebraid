// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
#[test]
fn am_tensor_out_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_tensor_out_t>(), 56);
    assert_eq!(core::mem::align_of::<am_tensor_out_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_tensor_out_t, name),
        0,
        "am_tensor_out_t.name"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_out_t, dtype),
        8,
        "am_tensor_out_t.dtype"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_out_t, shape),
        16,
        "am_tensor_out_t.shape"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_out_t, shape_len),
        24,
        "am_tensor_out_t.shape_len"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_out_t, data),
        32,
        "am_tensor_out_t.data"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_out_t, data_size),
        40,
        "am_tensor_out_t.data_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_out_t, is_contiguous),
        48,
        "am_tensor_out_t.is_contiguous"
    );
}
#[test]
fn am_keypoint_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_keypoint_t>(), 12);
    assert_eq!(core::mem::align_of::<am_keypoint_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_keypoint_t, x),
        0,
        "am_keypoint_t.x"
    );
    assert_eq!(
        core::mem::offset_of!(am_keypoint_t, y),
        4,
        "am_keypoint_t.y"
    );
    assert_eq!(
        core::mem::offset_of!(am_keypoint_t, confidence),
        8,
        "am_keypoint_t.confidence"
    );
}
#[test]
fn am_pose_detection_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_pose_detection_t>(), 32);
    assert_eq!(core::mem::align_of::<am_pose_detection_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_pose_detection_t, bounds),
        0,
        "am_pose_detection_t.bounds"
    );
    assert_eq!(
        core::mem::offset_of!(am_pose_detection_t, confidence),
        16,
        "am_pose_detection_t.confidence"
    );
    assert_eq!(
        core::mem::offset_of!(am_pose_detection_t, kpt_count),
        20,
        "am_pose_detection_t.kpt_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_pose_detection_t, keypoints),
        24,
        "am_pose_detection_t.keypoints"
    );
}
#[test]
fn am_pose_result_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_pose_result_t>(), 16);
    assert_eq!(core::mem::align_of::<am_pose_result_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_pose_result_t, count),
        0,
        "am_pose_result_t.count"
    );
    assert_eq!(
        core::mem::offset_of!(am_pose_result_t, detections),
        8,
        "am_pose_result_t.detections"
    );
}
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
