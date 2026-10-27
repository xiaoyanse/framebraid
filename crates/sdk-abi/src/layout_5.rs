// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
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
fn am_image_input_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_image_input_t>(), 48);
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
}
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
fn am_batching_options_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_batching_options_t>(), 20);
    assert_eq!(core::mem::align_of::<am_batching_options_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_batching_options_t, struct_size),
        0,
        "am_batching_options_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_batching_options_t, abi_minor),
        4,
        "am_batching_options_t.abi_minor"
    );
    assert_eq!(
        core::mem::offset_of!(am_batching_options_t, max_batch_items),
        8,
        "am_batching_options_t.max_batch_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_batching_options_t, max_wait_ms),
        12,
        "am_batching_options_t.max_wait_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_batching_options_t, flags),
        16,
        "am_batching_options_t.flags"
    );
}
#[test]
fn am_batching_stats_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_batching_stats_t>(), 40);
    assert_eq!(core::mem::align_of::<am_batching_stats_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_batching_stats_t, struct_size),
        0,
        "am_batching_stats_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_batching_stats_t, abi_minor),
        4,
        "am_batching_stats_t.abi_minor"
    );
    assert_eq!(
        core::mem::offset_of!(am_batching_stats_t, formed_batches),
        8,
        "am_batching_stats_t.formed_batches"
    );
    assert_eq!(
        core::mem::offset_of!(am_batching_stats_t, batched_items),
        16,
        "am_batching_stats_t.batched_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_batching_stats_t, timeout_batches),
        24,
        "am_batching_stats_t.timeout_batches"
    );
    assert_eq!(
        core::mem::offset_of!(am_batching_stats_t, rejected_items),
        32,
        "am_batching_stats_t.rejected_items"
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
