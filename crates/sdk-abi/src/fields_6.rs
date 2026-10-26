// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
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
