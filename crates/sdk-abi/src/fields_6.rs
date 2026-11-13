// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
impl AbiLayout for am_cascade_hit_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_cascade_hit_t, primary_index),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_cascade_hit_t, roi),
            core::mem::size_of::<[f32; 4]>(),
        ),
    ];
}
impl AbiLayout for am_cascade_result_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_cascade_result_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_cascade_result_t, secondary_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_cascade_result_t, secondary),
            core::mem::size_of::<*const am_detect_box_t>(),
        ),
        (
            core::mem::offset_of!(am_cascade_result_t, hits),
            core::mem::size_of::<*const am_cascade_hit_t>(),
        ),
        (
            core::mem::offset_of!(am_cascade_result_t, skipped_count),
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
impl AbiLayout for am_pool_desc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_pool_desc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_pool_desc_t, model),
            core::mem::size_of::<*const am_model_desc_t>(),
        ),
        (
            core::mem::offset_of!(am_pool_desc_t, model_memory),
            core::mem::size_of::<*const am_model_memory_desc_t>(),
        ),
        (
            core::mem::offset_of!(am_pool_desc_t, shards),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_pool_desc_t, lanes),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_pool_desc_t, batching),
            core::mem::size_of::<*const am_batching_options_t>(),
        ),
        (
            core::mem::offset_of!(am_pool_desc_t, reserved),
            core::mem::size_of::<u32>(),
        ),
    ];
}
