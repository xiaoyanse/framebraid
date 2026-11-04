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
fn am_track_params_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_track_params_t>(), 20);
    assert_eq!(core::mem::align_of::<am_track_params_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_track_params_t, struct_size),
        0,
        "am_track_params_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_track_params_t, iou_threshold),
        4,
        "am_track_params_t.iou_threshold"
    );
    assert_eq!(
        core::mem::offset_of!(am_track_params_t, ttl_frames),
        8,
        "am_track_params_t.ttl_frames"
    );
    assert_eq!(
        core::mem::offset_of!(am_track_params_t, confirm_hits),
        12,
        "am_track_params_t.confirm_hits"
    );
    assert_eq!(
        core::mem::offset_of!(am_track_params_t, max_tracks),
        16,
        "am_track_params_t.max_tracks"
    );
}
#[test]
fn am_tracked_box_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_tracked_box_t>(), 40);
    assert_eq!(core::mem::align_of::<am_tracked_box_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_tracked_box_t, bounds),
        0,
        "am_tracked_box_t.bounds"
    );
    assert_eq!(
        core::mem::offset_of!(am_tracked_box_t, confidence),
        16,
        "am_tracked_box_t.confidence"
    );
    assert_eq!(
        core::mem::offset_of!(am_tracked_box_t, class_id),
        20,
        "am_tracked_box_t.class_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_tracked_box_t, track_id),
        24,
        "am_tracked_box_t.track_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_tracked_box_t, age_frames),
        32,
        "am_tracked_box_t.age_frames"
    );
    assert_eq!(
        core::mem::offset_of!(am_tracked_box_t, hits),
        36,
        "am_tracked_box_t.hits"
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
fn am_vocab_entry_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_vocab_entry_t>(), 32);
    assert_eq!(core::mem::align_of::<am_vocab_entry_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_vocab_entry_t, struct_size),
        0,
        "am_vocab_entry_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_vocab_entry_t, label_utf8),
        8,
        "am_vocab_entry_t.label_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_vocab_entry_t, token_ids),
        16,
        "am_vocab_entry_t.token_ids"
    );
    assert_eq!(
        core::mem::offset_of!(am_vocab_entry_t, token_rows),
        24,
        "am_vocab_entry_t.token_rows"
    );
    assert_eq!(
        core::mem::offset_of!(am_vocab_entry_t, token_cols),
        28,
        "am_vocab_entry_t.token_cols"
    );
}
#[test]
fn am_vocab_hit_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_vocab_hit_t>(), 8);
    assert_eq!(core::mem::align_of::<am_vocab_hit_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_vocab_hit_t, entry_index),
        0,
        "am_vocab_hit_t.entry_index"
    );
    assert_eq!(
        core::mem::offset_of!(am_vocab_hit_t, prompt_index),
        4,
        "am_vocab_hit_t.prompt_index"
    );
}
