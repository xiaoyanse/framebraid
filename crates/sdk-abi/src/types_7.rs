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
#[derive(Clone, Copy)]
pub struct am_track_params_t {
    pub struct_size: u32,
    pub iou_threshold: f32,
    pub ttl_frames: u32,
    pub confirm_hits: u32,
    pub max_tracks: u32,
}
impl Default for am_track_params_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            iou_threshold: Default::default(),
            ttl_frames: Default::default(),
            confirm_hits: Default::default(),
            max_tracks: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_tracked_box_t {
    pub bounds: [f32; 4],
    pub confidence: f32,
    pub class_id: u32,
    pub track_id: u64,
    pub age_frames: u32,
    pub hits: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_detect_result_t {
    pub count: u32,
    pub boxes: *const am_detect_box_t,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_vocab_entry_t {
    pub struct_size: u32,
    pub label_utf8: *const c_char,
    pub token_ids: *const i64,
    pub token_rows: u32,
    pub token_cols: u32,
}
impl Default for am_vocab_entry_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            label_utf8: Default::default(),
            token_ids: Default::default(),
            token_rows: Default::default(),
            token_cols: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_vocab_hit_t {
    pub entry_index: i32,
    pub prompt_index: i32,
}
