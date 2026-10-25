// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_infer_params_t {
    pub struct_size: u32,
    pub confidence_threshold: f32,
    pub nms_iou_threshold: f32,
    pub max_det: u32,
    pub sync_deadline_ms: u32,
    pub slot_key: u64,
    pub raw_preprocess: *const am_raw_preprocess_t,
    pub reserved: [u32; 2],
}
impl Default for am_infer_params_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            confidence_threshold: Default::default(),
            nms_iou_threshold: Default::default(),
            max_det: Default::default(),
            sync_deadline_ms: Default::default(),
            slot_key: Default::default(),
            raw_preprocess: Default::default(),
            reserved: [Default::default(); 2],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_tensor_in_t {
    pub name: *const c_char,
    pub dtype: u32,
    pub shape: *const i64,
    pub shape_len: u32,
    pub data: *const c_void,
    pub data_size: u64,
    pub flags: u32,
    pub assoc: *const am_frame_assoc_t,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_batch_item_t {
    pub struct_size: u32,
    pub image: *const am_image_input_t,
    pub tensor_count: u32,
    pub tensors: *const am_tensor_in_t,
    pub slot_key: u64,
    pub raw_preprocess: *const am_raw_preprocess_t,
}
impl Default for am_batch_item_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            image: Default::default(),
            tensor_count: Default::default(),
            tensors: Default::default(),
            slot_key: Default::default(),
            raw_preprocess: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_batch_options_t {
    pub struct_size: u32,
    pub batch_mode: u32,
    pub wait_deadline_ms: u32,
}
impl Default for am_batch_options_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            batch_mode: Default::default(),
            wait_deadline_ms: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_batch_receipt_t {
    pub struct_size: u32,
    pub accepted_count: u32,
    pub request_ids: *mut u64,
    pub rejects: *const am_status_t,
}
impl Default for am_batch_receipt_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            accepted_count: Default::default(),
            request_ids: Default::default(),
            rejects: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_preprocess_meta_t {
    pub struct_size: u32,
    pub src_w: u32,
    pub src_h: u32,
    pub dst_w: u32,
    pub dst_h: u32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub crop_x: i32,
    pub crop_y: i32,
    pub pad_left: u32,
    pub pad_top: u32,
    pub valid: u32,
}
impl Default for am_preprocess_meta_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            src_w: Default::default(),
            src_h: Default::default(),
            dst_w: Default::default(),
            dst_h: Default::default(),
            scale_x: Default::default(),
            scale_y: Default::default(),
            crop_x: Default::default(),
            crop_y: Default::default(),
            pad_left: Default::default(),
            pad_top: Default::default(),
            valid: Default::default(),
        }
    }
}
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
