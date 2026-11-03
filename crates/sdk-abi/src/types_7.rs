// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
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
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_vocab_hit_map_t {
    pub struct_size: u32,
    pub count: u32,
    pub hits: *const am_vocab_hit_t,
}
impl Default for am_vocab_hit_map_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            count: Default::default(),
            hits: Default::default(),
        }
    }
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
