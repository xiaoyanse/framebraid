// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
#[test]
fn am_infer_params_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_infer_params_t>(), 48);
    assert_eq!(core::mem::align_of::<am_infer_params_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_infer_params_t, struct_size),
        0,
        "am_infer_params_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_infer_params_t, confidence_threshold),
        4,
        "am_infer_params_t.confidence_threshold"
    );
    assert_eq!(
        core::mem::offset_of!(am_infer_params_t, nms_iou_threshold),
        8,
        "am_infer_params_t.nms_iou_threshold"
    );
    assert_eq!(
        core::mem::offset_of!(am_infer_params_t, max_det),
        12,
        "am_infer_params_t.max_det"
    );
    assert_eq!(
        core::mem::offset_of!(am_infer_params_t, sync_deadline_ms),
        16,
        "am_infer_params_t.sync_deadline_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_infer_params_t, slot_key),
        24,
        "am_infer_params_t.slot_key"
    );
    assert_eq!(
        core::mem::offset_of!(am_infer_params_t, raw_preprocess),
        32,
        "am_infer_params_t.raw_preprocess"
    );
    assert_eq!(
        core::mem::offset_of!(am_infer_params_t, reserved),
        40,
        "am_infer_params_t.reserved"
    );
}
#[test]
fn am_tensor_in_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_tensor_in_t>(), 64);
    assert_eq!(core::mem::align_of::<am_tensor_in_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_tensor_in_t, name),
        0,
        "am_tensor_in_t.name"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_in_t, dtype),
        8,
        "am_tensor_in_t.dtype"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_in_t, shape),
        16,
        "am_tensor_in_t.shape"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_in_t, shape_len),
        24,
        "am_tensor_in_t.shape_len"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_in_t, data),
        32,
        "am_tensor_in_t.data"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_in_t, data_size),
        40,
        "am_tensor_in_t.data_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_in_t, flags),
        48,
        "am_tensor_in_t.flags"
    );
    assert_eq!(
        core::mem::offset_of!(am_tensor_in_t, assoc),
        56,
        "am_tensor_in_t.assoc"
    );
}
#[test]
fn am_batch_item_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_batch_item_t>(), 48);
    assert_eq!(core::mem::align_of::<am_batch_item_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_batch_item_t, struct_size),
        0,
        "am_batch_item_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_batch_item_t, image),
        8,
        "am_batch_item_t.image"
    );
    assert_eq!(
        core::mem::offset_of!(am_batch_item_t, tensor_count),
        16,
        "am_batch_item_t.tensor_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_batch_item_t, tensors),
        24,
        "am_batch_item_t.tensors"
    );
    assert_eq!(
        core::mem::offset_of!(am_batch_item_t, slot_key),
        32,
        "am_batch_item_t.slot_key"
    );
    assert_eq!(
        core::mem::offset_of!(am_batch_item_t, raw_preprocess),
        40,
        "am_batch_item_t.raw_preprocess"
    );
}
#[test]
fn am_batch_options_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_batch_options_t>(), 12);
    assert_eq!(core::mem::align_of::<am_batch_options_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_batch_options_t, struct_size),
        0,
        "am_batch_options_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_batch_options_t, batch_mode),
        4,
        "am_batch_options_t.batch_mode"
    );
    assert_eq!(
        core::mem::offset_of!(am_batch_options_t, wait_deadline_ms),
        8,
        "am_batch_options_t.wait_deadline_ms"
    );
}
#[test]
fn am_batch_receipt_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_batch_receipt_t>(), 24);
    assert_eq!(core::mem::align_of::<am_batch_receipt_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_batch_receipt_t, struct_size),
        0,
        "am_batch_receipt_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_batch_receipt_t, accepted_count),
        4,
        "am_batch_receipt_t.accepted_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_batch_receipt_t, request_ids),
        8,
        "am_batch_receipt_t.request_ids"
    );
    assert_eq!(
        core::mem::offset_of!(am_batch_receipt_t, rejects),
        16,
        "am_batch_receipt_t.rejects"
    );
}
#[test]
fn am_preprocess_meta_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_preprocess_meta_t>(), 48);
    assert_eq!(core::mem::align_of::<am_preprocess_meta_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, struct_size),
        0,
        "am_preprocess_meta_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, src_w),
        4,
        "am_preprocess_meta_t.src_w"
    );
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, src_h),
        8,
        "am_preprocess_meta_t.src_h"
    );
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, dst_w),
        12,
        "am_preprocess_meta_t.dst_w"
    );
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, dst_h),
        16,
        "am_preprocess_meta_t.dst_h"
    );
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, scale_x),
        20,
        "am_preprocess_meta_t.scale_x"
    );
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, scale_y),
        24,
        "am_preprocess_meta_t.scale_y"
    );
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, crop_x),
        28,
        "am_preprocess_meta_t.crop_x"
    );
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, crop_y),
        32,
        "am_preprocess_meta_t.crop_y"
    );
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, pad_left),
        36,
        "am_preprocess_meta_t.pad_left"
    );
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, pad_top),
        40,
        "am_preprocess_meta_t.pad_top"
    );
    assert_eq!(
        core::mem::offset_of!(am_preprocess_meta_t, valid),
        44,
        "am_preprocess_meta_t.valid"
    );
}
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
