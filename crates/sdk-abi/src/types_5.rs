// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
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
