// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
impl AbiLayout for am_model_identity_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_model_identity_t, model_uid),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_model_identity_t, model_version),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_identity_t, profile_id),
            core::mem::size_of::<*const c_char>(),
        ),
    ];
}
impl AbiLayout for am_model_info_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_model_info_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, identity)
                + core::mem::offset_of!(am_model_identity_t, model_uid),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, identity)
                + core::mem::offset_of!(am_model_identity_t, model_version),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, identity)
                + core::mem::offset_of!(am_model_identity_t, profile_id),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, load_mode),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, input_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, output_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, inputs),
            core::mem::size_of::<*const am_io_spec_t>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, outputs),
            core::mem::size_of::<*const am_io_spec_t>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, native_batch),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, execution)
                + core::mem::offset_of!(am_backend_actual_t, actual),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, execution)
                + core::mem::offset_of!(am_backend_actual_t, device_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, execution)
                + core::mem::offset_of!(am_backend_actual_t, subgraph_cpu_nodes),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_model_info_t, execution)
                + core::mem::offset_of!(am_backend_actual_t, fallback_reason_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
    ];
}
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
impl AbiLayout for am_gpu_frame_desc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, width),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, height),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, d3d_format),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, subresource),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, shared_nt_handle),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, flags),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, source_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, frame_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, source_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, config_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, pts),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, time_base_num),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, time_base_den),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, captured_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, received_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, clock_mask),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, clock_quality),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_desc_t, assoc)
                + core::mem::offset_of!(am_frame_assoc_t, valid),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_gpu_frame_stats_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_gpu_frame_stats_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_stats_t, gpu_frames_delivered),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_stats_t, cpu_fallbacks),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_stats_t, bytes_transferred_est),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for am_gpu_frame_ref_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_gpu_frame_ref_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_ref_t, width),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_ref_t, height),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_ref_t, d3d_format),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_ref_t, subresource),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_ref_t, shared_nt_handle),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_ref_t, texture),
            core::mem::size_of::<*mut c_void>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_ref_t, flags),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_ref_t, reserved),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_gpu_frame_ref_t, assoc),
            core::mem::size_of::<*const am_frame_assoc_t>(),
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
        (
            core::mem::offset_of!(am_image_input_t, gpu),
            core::mem::size_of::<*const am_gpu_frame_ref_t>(),
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
