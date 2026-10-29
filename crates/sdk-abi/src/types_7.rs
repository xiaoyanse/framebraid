// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
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
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_segment_result_t {
    pub count: u32,
    pub instances: *const am_segment_instance_t,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_obb_instance_t {
    pub vertices_xy: [f32; 8],
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    pub angle: f32,
    pub confidence: f32,
    pub class_id: i32,
    pub angle_normalized: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_obb_result_t {
    pub count: u32,
    pub instances: *const am_obb_instance_t,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct amv_result_t {
    pub struct_size: u32,
    pub request_id: u64,
    pub task: am_task_e,
    pub frame: *const am_frame_assoc_t,
    pub model: am_model_identity_t,
    pub execution: am_backend_actual_t,
    pub transform: *const am_preprocess_meta_t,
    pub pose: am_pose_result_t,
    pub detect: am_detect_result_t,
    pub classify: am_classify_result_t,
    pub segment: am_segment_result_t,
    pub obb: am_obb_result_t,
    pub raw_output_count: u32,
    pub raw_outputs: *const am_tensor_out_t,
    pub infer_start_ms: i64,
    pub infer_end_ms: i64,
}
impl Default for amv_result_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            request_id: Default::default(),
            task: Default::default(),
            frame: Default::default(),
            model: Default::default(),
            execution: Default::default(),
            transform: Default::default(),
            pose: Default::default(),
            detect: Default::default(),
            classify: Default::default(),
            segment: Default::default(),
            obb: Default::default(),
            raw_output_count: Default::default(),
            raw_outputs: Default::default(),
            infer_start_ms: Default::default(),
            infer_end_ms: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_realtime_policy_t {
    pub struct_size: u32,
    pub max_inflight: u32,
    pub replace_oldest: u32,
    pub slot_count: u32,
    pub fair_round_robin: u32,
}
impl Default for am_realtime_policy_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            max_inflight: Default::default(),
            replace_oldest: Default::default(),
            slot_count: Default::default(),
            fair_round_robin: Default::default(),
        }
    }
}
