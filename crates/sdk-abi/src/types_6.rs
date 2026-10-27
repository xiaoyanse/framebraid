// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_tensor_out_t {
    pub name: *const c_char,
    pub dtype: u32,
    pub shape: *const i64,
    pub shape_len: u32,
    pub data: *const c_void,
    pub data_size: u64,
    pub is_contiguous: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_keypoint_t {
    pub x: f32,
    pub y: f32,
    pub confidence: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_pose_detection_t {
    pub bounds: [f32; 4],
    pub confidence: f32,
    pub kpt_count: u32,
    pub keypoints: *const am_keypoint_t,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_pose_result_t {
    pub count: u32,
    pub detections: *const am_pose_detection_t,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_detect_box_t {
    pub bounds: [f32; 4],
    pub confidence: f32,
    pub class_id: i32,
    pub class_name_utf8: *const c_char,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_detect_result_t {
    pub count: u32,
    pub boxes: *const am_detect_box_t,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_classify_entry_t {
    pub class_id: i32,
    pub class_name_utf8: *const c_char,
    pub logit: f32,
    pub probability: f32,
    pub has_probability: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_classify_result_t {
    pub count: u32,
    pub entries: *const am_classify_entry_t,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_roi_map_t {
    pub scale_x: f32,
    pub scale_y: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub valid: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_segment_instance_t {
    pub bounds: [f32; 4],
    pub confidence: f32,
    pub class_id: i32,
    pub mask_width: u32,
    pub mask_height: u32,
    pub mask_stride_bytes: u32,
    pub mask_bits: *const u8,
    pub roi_map: am_roi_map_t,
}
