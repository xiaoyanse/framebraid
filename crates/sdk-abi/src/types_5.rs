// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_model_identity_t {
    pub model_uid: u64,
    pub model_version: u32,
    pub profile_id: *const c_char,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_model_info_t {
    pub struct_size: u32,
    pub identity: am_model_identity_t,
    pub load_mode: u32,
    pub input_count: u32,
    pub output_count: u32,
    pub inputs: *const am_io_spec_t,
    pub outputs: *const am_io_spec_t,
    pub native_batch: u32,
    pub execution: am_backend_actual_t,
}
impl Default for am_model_info_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            identity: Default::default(),
            load_mode: Default::default(),
            input_count: Default::default(),
            output_count: Default::default(),
            inputs: Default::default(),
            outputs: Default::default(),
            native_batch: Default::default(),
            execution: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_frame_assoc_t {
    pub struct_size: u32,
    pub source_id: u64,
    pub frame_id: u64,
    pub source_generation: u32,
    pub config_generation: u32,
    pub pts: i64,
    pub time_base_num: u32,
    pub time_base_den: u32,
    pub captured_at_ms: i64,
    pub received_at_ms: i64,
    pub clock_mask: u32,
    pub clock_quality: u32,
    pub valid: u32,
}
impl Default for am_frame_assoc_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            source_id: Default::default(),
            frame_id: Default::default(),
            source_generation: Default::default(),
            config_generation: Default::default(),
            pts: Default::default(),
            time_base_num: Default::default(),
            time_base_den: Default::default(),
            captured_at_ms: Default::default(),
            received_at_ms: Default::default(),
            clock_mask: Default::default(),
            clock_quality: Default::default(),
            valid: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_gpu_frame_desc_t {
    pub struct_size: u32,
    pub width: u32,
    pub height: u32,
    pub d3d_format: u32,
    pub subresource: u32,
    pub shared_nt_handle: u64,
    pub flags: u32,
    pub assoc: am_frame_assoc_t,
}
impl Default for am_gpu_frame_desc_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            width: Default::default(),
            height: Default::default(),
            d3d_format: Default::default(),
            subresource: Default::default(),
            shared_nt_handle: Default::default(),
            flags: Default::default(),
            assoc: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_gpu_frame_stats_t {
    pub struct_size: u32,
    pub gpu_frames_delivered: u64,
    pub cpu_fallbacks: u64,
    pub bytes_transferred_est: u64,
}
impl Default for am_gpu_frame_stats_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            gpu_frames_delivered: Default::default(),
            cpu_fallbacks: Default::default(),
            bytes_transferred_est: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_gpu_frame_ref_t {
    pub struct_size: u32,
    pub width: u32,
    pub height: u32,
    pub d3d_format: u32,
    pub subresource: u32,
    pub shared_nt_handle: u64,
    pub texture: *mut c_void,
    pub flags: u32,
    pub reserved: u32,
    pub assoc: *const am_frame_assoc_t,
}
impl Default for am_gpu_frame_ref_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            width: Default::default(),
            height: Default::default(),
            d3d_format: Default::default(),
            subresource: Default::default(),
            shared_nt_handle: Default::default(),
            texture: Default::default(),
            flags: Default::default(),
            reserved: Default::default(),
            assoc: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_image_input_t {
    pub struct_size: u32,
    pub kind: u32,
    pub encoded: *const u8,
    pub encoded_size: u64,
    pub view: *const am_pixel_view_t,
    pub assoc: *const am_frame_assoc_t,
    pub flags: u32,
    pub gpu: *const am_gpu_frame_ref_t,
}
impl Default for am_image_input_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            kind: Default::default(),
            encoded: Default::default(),
            encoded_size: Default::default(),
            view: Default::default(),
            assoc: Default::default(),
            flags: Default::default(),
            gpu: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_cascade_rule_t {
    pub struct_size: u32,
    pub source_class_id: i32,
    pub expand: f32,
    pub min_roi_size: f32,
}
impl Default for am_cascade_rule_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            source_class_id: Default::default(),
            expand: Default::default(),
            min_roi_size: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_cascade_request_t {
    pub struct_size: u32,
    pub static_rois: *const f32,
    pub static_roi_count: u32,
    pub rules: *const am_cascade_rule_t,
    pub rule_count: u32,
    pub secondary_confidence: f32,
}
impl Default for am_cascade_request_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            static_rois: Default::default(),
            static_roi_count: Default::default(),
            rules: Default::default(),
            rule_count: Default::default(),
            secondary_confidence: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_cascade_hit_t {
    pub primary_index: i32,
    pub roi: [f32; 4],
}
