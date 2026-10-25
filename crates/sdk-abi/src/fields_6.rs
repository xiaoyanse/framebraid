// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
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
impl AbiLayout for am_classify_entry_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_classify_entry_t, class_id),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_classify_entry_t, class_name_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_classify_entry_t, logit),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_classify_entry_t, probability),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_classify_entry_t, has_probability),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_classify_result_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_classify_result_t, count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_classify_result_t, entries),
            core::mem::size_of::<*const am_classify_entry_t>(),
        ),
    ];
}
impl AbiLayout for am_roi_map_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_roi_map_t, scale_x),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_roi_map_t, scale_y),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_roi_map_t, offset_x),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_roi_map_t, offset_y),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_roi_map_t, valid),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_segment_instance_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_segment_instance_t, bounds),
            core::mem::size_of::<[f32; 4]>(),
        ),
        (
            core::mem::offset_of!(am_segment_instance_t, confidence),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_segment_instance_t, class_id),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_segment_instance_t, mask_width),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_segment_instance_t, mask_height),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_segment_instance_t, mask_stride_bytes),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_segment_instance_t, mask_bits),
            core::mem::size_of::<*const u8>(),
        ),
        (
            core::mem::offset_of!(am_segment_instance_t, roi_map)
                + core::mem::offset_of!(am_roi_map_t, scale_x),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_segment_instance_t, roi_map)
                + core::mem::offset_of!(am_roi_map_t, scale_y),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_segment_instance_t, roi_map)
                + core::mem::offset_of!(am_roi_map_t, offset_x),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_segment_instance_t, roi_map)
                + core::mem::offset_of!(am_roi_map_t, offset_y),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_segment_instance_t, roi_map)
                + core::mem::offset_of!(am_roi_map_t, valid),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_segment_result_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_segment_result_t, count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_segment_result_t, instances),
            core::mem::size_of::<*const am_segment_instance_t>(),
        ),
    ];
}
impl AbiLayout for am_obb_instance_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_obb_instance_t, vertices_xy),
            core::mem::size_of::<[f32; 8]>(),
        ),
        (
            core::mem::offset_of!(am_obb_instance_t, center_x),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_obb_instance_t, center_y),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_obb_instance_t, width),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_obb_instance_t, height),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_obb_instance_t, angle),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_obb_instance_t, confidence),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_obb_instance_t, class_id),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_obb_instance_t, angle_normalized),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_obb_result_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_obb_result_t, count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_obb_result_t, instances),
            core::mem::size_of::<*const am_obb_instance_t>(),
        ),
    ];
}
impl AbiLayout for amv_result_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(amv_result_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, request_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, task),
            core::mem::size_of::<am_task_e>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, frame),
            core::mem::size_of::<*const am_frame_assoc_t>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, model)
                + core::mem::offset_of!(am_model_identity_t, model_uid),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, model)
                + core::mem::offset_of!(am_model_identity_t, model_version),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, model)
                + core::mem::offset_of!(am_model_identity_t, profile_id),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, execution)
                + core::mem::offset_of!(am_backend_actual_t, actual),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, execution)
                + core::mem::offset_of!(am_backend_actual_t, device_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, execution)
                + core::mem::offset_of!(am_backend_actual_t, subgraph_cpu_nodes),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, execution)
                + core::mem::offset_of!(am_backend_actual_t, fallback_reason_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, transform),
            core::mem::size_of::<*const am_preprocess_meta_t>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, pose)
                + core::mem::offset_of!(am_pose_result_t, count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, pose)
                + core::mem::offset_of!(am_pose_result_t, detections),
            core::mem::size_of::<*const am_pose_detection_t>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, detect)
                + core::mem::offset_of!(am_detect_result_t, count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, detect)
                + core::mem::offset_of!(am_detect_result_t, boxes),
            core::mem::size_of::<*const am_detect_box_t>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, classify)
                + core::mem::offset_of!(am_classify_result_t, count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, classify)
                + core::mem::offset_of!(am_classify_result_t, entries),
            core::mem::size_of::<*const am_classify_entry_t>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, segment)
                + core::mem::offset_of!(am_segment_result_t, count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, segment)
                + core::mem::offset_of!(am_segment_result_t, instances),
            core::mem::size_of::<*const am_segment_instance_t>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, obb)
                + core::mem::offset_of!(am_obb_result_t, count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, obb)
                + core::mem::offset_of!(am_obb_result_t, instances),
            core::mem::size_of::<*const am_obb_instance_t>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, raw_output_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, raw_outputs),
            core::mem::size_of::<*const am_tensor_out_t>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, infer_start_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(amv_result_t, infer_end_ms),
            core::mem::size_of::<i64>(),
        ),
    ];
}
