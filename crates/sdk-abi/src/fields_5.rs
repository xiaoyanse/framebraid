// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
impl AbiLayout for am_frame_assoc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_frame_assoc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, source_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, frame_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, source_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, config_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, pts),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, time_base_num),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, time_base_den),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, captured_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, received_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, clock_mask),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, clock_quality),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_assoc_t, valid),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_image_input_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_image_input_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_image_input_t, kind),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_image_input_t, encoded),
            core::mem::size_of::<*const u8>(),
        ),
        (
            core::mem::offset_of!(am_image_input_t, encoded_size),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_image_input_t, view),
            core::mem::size_of::<*const am_pixel_view_t>(),
        ),
        (
            core::mem::offset_of!(am_image_input_t, assoc),
            core::mem::size_of::<*const am_frame_assoc_t>(),
        ),
        (
            core::mem::offset_of!(am_image_input_t, flags),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_infer_params_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_infer_params_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_infer_params_t, confidence_threshold),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_infer_params_t, nms_iou_threshold),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_infer_params_t, max_det),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_infer_params_t, sync_deadline_ms),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_infer_params_t, slot_key),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_infer_params_t, raw_preprocess),
            core::mem::size_of::<*const am_raw_preprocess_t>(),
        ),
        (
            core::mem::offset_of!(am_infer_params_t, reserved),
            core::mem::size_of::<[u32; 2]>(),
        ),
    ];
}
impl AbiLayout for am_tensor_in_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_tensor_in_t, name),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_tensor_in_t, dtype),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_tensor_in_t, shape),
            core::mem::size_of::<*const i64>(),
        ),
        (
            core::mem::offset_of!(am_tensor_in_t, shape_len),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_tensor_in_t, data),
            core::mem::size_of::<*const c_void>(),
        ),
        (
            core::mem::offset_of!(am_tensor_in_t, data_size),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_tensor_in_t, flags),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_tensor_in_t, assoc),
            core::mem::size_of::<*const am_frame_assoc_t>(),
        ),
    ];
}
impl AbiLayout for am_batch_item_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_batch_item_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batch_item_t, image),
            core::mem::size_of::<*const am_image_input_t>(),
        ),
        (
            core::mem::offset_of!(am_batch_item_t, tensor_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batch_item_t, tensors),
            core::mem::size_of::<*const am_tensor_in_t>(),
        ),
        (
            core::mem::offset_of!(am_batch_item_t, slot_key),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_batch_item_t, raw_preprocess),
            core::mem::size_of::<*const am_raw_preprocess_t>(),
        ),
    ];
}
impl AbiLayout for am_batch_options_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_batch_options_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batch_options_t, batch_mode),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_batch_options_t, wait_deadline_ms),
            core::mem::size_of::<u32>(),
        ),
    ];
}
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
