// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_backend_candidate_t {
    pub backend: u32,
    pub device_id: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_backend_selection_t {
    pub struct_size: u32,
    pub candidate_count: u32,
    pub candidates: *const am_backend_candidate_t,
    pub allow_fallback: u32,
    pub budget_ms: u32,
}
impl Default for am_backend_selection_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            candidate_count: Default::default(),
            candidates: Default::default(),
            allow_fallback: Default::default(),
            budget_ms: Default::default(),
        }
    }
}
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
pub struct am_encoder_config_t {
    pub struct_size: u32,
    pub width: u32,
    pub height: u32,
    pub bitrate_kbps: u32,
    pub gop_size: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub queue_max_items: u32,
    pub queue_max_bytes: u32,
}
impl Default for am_encoder_config_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            width: Default::default(),
            height: Default::default(),
            bitrate_kbps: Default::default(),
            gop_size: Default::default(),
            fps_num: Default::default(),
            fps_den: Default::default(),
            queue_max_items: Default::default(),
            queue_max_bytes: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_model_swap_desc_t {
    pub struct_size: u32,
    pub slot_key: u64,
    pub flags: u32,
    pub reserved: u32,
}
impl Default for am_model_swap_desc_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            slot_key: Default::default(),
            flags: Default::default(),
            reserved: Default::default(),
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
