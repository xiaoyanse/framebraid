// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
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
#[test]
fn am_vocab_hit_map_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_vocab_hit_map_t>(), 16);
    assert_eq!(core::mem::align_of::<am_vocab_hit_map_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_vocab_hit_map_t, struct_size),
        0,
        "am_vocab_hit_map_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_vocab_hit_map_t, count),
        4,
        "am_vocab_hit_map_t.count"
    );
    assert_eq!(
        core::mem::offset_of!(am_vocab_hit_map_t, hits),
        8,
        "am_vocab_hit_map_t.hits"
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
