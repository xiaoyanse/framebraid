// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_backend_policy_t {
    pub struct_size: u32,
    pub requested: u32,
    pub strict: u32,
}
impl Default for am_backend_policy_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            requested: Default::default(),
            strict: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_raw_preprocess_t {
    pub struct_size: u32,
    pub resize: u32,
    pub target_w: u32,
    pub target_h: u32,
    pub fill_value: f32,
    pub color_order: u32,
    pub normalize: u32,
    pub mean: [f32; 3],
    pub std: [f32; 3],
    pub layout: u32,
}
impl Default for am_raw_preprocess_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            resize: Default::default(),
            target_w: Default::default(),
            target_h: Default::default(),
            fill_value: Default::default(),
            color_order: Default::default(),
            normalize: Default::default(),
            mean: [Default::default(); 3],
            std: [Default::default(); 3],
            layout: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_model_desc_t {
    pub struct_size: u32,
    pub path_utf8: *const c_char,
    pub load_mode: u32,
    pub profile_id: *const c_char,
    pub backend: *const am_backend_policy_t,
    pub intra_threads: u32,
}
impl Default for am_model_desc_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            path_utf8: Default::default(),
            load_mode: Default::default(),
            profile_id: Default::default(),
            backend: Default::default(),
            intra_threads: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_model_memory_desc_t {
    pub struct_size: u32,
    pub bytes: *const u8,
    pub size: u64,
    pub load_mode: u32,
    pub profile_id: *const c_char,
    pub backend: *const am_backend_policy_t,
}
impl Default for am_model_memory_desc_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            bytes: Default::default(),
            size: Default::default(),
            load_mode: Default::default(),
            profile_id: Default::default(),
            backend: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_io_spec_t {
    pub struct_size: u32,
    pub name: *const c_char,
    pub dtype: u32,
    pub shape: *const i64,
    pub shape_len: u32,
    pub dyn_min: *const i64,
    pub dyn_max: *const i64,
}
impl Default for am_io_spec_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            name: Default::default(),
            dtype: Default::default(),
            shape: Default::default(),
            shape_len: Default::default(),
            dyn_min: Default::default(),
            dyn_max: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_backend_actual_t {
    pub actual: u32,
    pub device_utf8: *const c_char,
    pub subgraph_cpu_nodes: i32,
    pub fallback_reason_utf8: *const c_char,
}
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
pub struct am_image_input_t {
    pub struct_size: u32,
    pub kind: u32,
    pub encoded: *const u8,
    pub encoded_size: u64,
    pub view: *const am_pixel_view_t,
    pub assoc: *const am_frame_assoc_t,
    pub flags: u32,
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
        }
    }
}
