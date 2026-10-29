// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
impl AbiLayout for am_batch_receipt_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_batch_receipt_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batch_receipt_t, accepted_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batch_receipt_t, request_ids),
            core::mem::size_of::<*mut u64>(),
        ),
        (
            core::mem::offset_of!(am_batch_receipt_t, rejects),
            core::mem::size_of::<*const am_status_t>(),
        ),
    ];
}
impl AbiLayout for am_batching_options_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_batching_options_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batching_options_t, abi_minor),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batching_options_t, max_batch_items),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batching_options_t, max_wait_ms),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batching_options_t, flags),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_batching_stats_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_batching_stats_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batching_stats_t, abi_minor),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batching_stats_t, formed_batches),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_batching_stats_t, batched_items),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_batching_stats_t, timeout_batches),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_batching_stats_t, rejected_items),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for am_preprocess_meta_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_preprocess_meta_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_preprocess_meta_t, src_w),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_preprocess_meta_t, src_h),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_preprocess_meta_t, dst_w),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_preprocess_meta_t, dst_h),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_preprocess_meta_t, scale_x),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_preprocess_meta_t, scale_y),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_preprocess_meta_t, crop_x),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_preprocess_meta_t, crop_y),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_preprocess_meta_t, pad_left),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_preprocess_meta_t, pad_top),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_preprocess_meta_t, valid),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_tensor_out_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_tensor_out_t, name),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_tensor_out_t, dtype),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_tensor_out_t, shape),
            core::mem::size_of::<*const i64>(),
        ),
        (
            core::mem::offset_of!(am_tensor_out_t, shape_len),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_tensor_out_t, data),
            core::mem::size_of::<*const c_void>(),
        ),
        (
            core::mem::offset_of!(am_tensor_out_t, data_size),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_tensor_out_t, is_contiguous),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_keypoint_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_keypoint_t, x),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_keypoint_t, y),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_keypoint_t, confidence),
            core::mem::size_of::<f32>(),
        ),
    ];
}
impl AbiLayout for am_pose_detection_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_pose_detection_t, bounds),
            core::mem::size_of::<[f32; 4]>(),
        ),
        (
            core::mem::offset_of!(am_pose_detection_t, confidence),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_pose_detection_t, kpt_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_pose_detection_t, keypoints),
            core::mem::size_of::<*const am_keypoint_t>(),
        ),
    ];
}
impl AbiLayout for am_pose_result_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_pose_result_t, count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_pose_result_t, detections),
            core::mem::size_of::<*const am_pose_detection_t>(),
        ),
    ];
}
impl AbiLayout for am_detect_box_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_detect_box_t, bounds),
            core::mem::size_of::<[f32; 4]>(),
        ),
        (
            core::mem::offset_of!(am_detect_box_t, confidence),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_detect_box_t, class_id),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_detect_box_t, class_name_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
    ];
}
impl AbiLayout for am_detect_result_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_detect_result_t, count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_detect_result_t, boxes),
            core::mem::size_of::<*const am_detect_box_t>(),
        ),
    ];
}
