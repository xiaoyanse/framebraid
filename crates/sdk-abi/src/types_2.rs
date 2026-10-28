// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_probe_result_t {
    pub struct_size: u32,
    pub candidate_count: u32,
    pub candidates: *const am_probe_candidate_t,
}
impl Default for am_probe_result_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            candidate_count: Default::default(),
            candidates: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_encoded_packet_t {
    pub struct_size: u32,
    pub codec: u32,
    pub bitstream_format: u32,
    pub flags: u32,
    pub config_generation: u32,
    pub source_generation: u32,
    pub source_id: u64,
    pub seq: u64,
    pub pts: i64,
    pub dts: i64,
    pub time_base_num: u32,
    pub time_base_den: u32,
    pub data: *const u8,
    pub size: u64,
}
impl Default for am_encoded_packet_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            codec: Default::default(),
            bitstream_format: Default::default(),
            flags: Default::default(),
            config_generation: Default::default(),
            source_generation: Default::default(),
            source_id: Default::default(),
            seq: Default::default(),
            pts: Default::default(),
            dts: Default::default(),
            time_base_num: Default::default(),
            time_base_den: Default::default(),
            data: Default::default(),
            size: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_encoded_sub_desc_t {
    pub struct_size: u32,
    pub codec_mask: u32,
    pub mode: u32,
    pub queue_max_items: u32,
    pub queue_max_bytes: u64,
}
impl Default for am_encoded_sub_desc_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            codec_mask: Default::default(),
            mode: Default::default(),
            queue_max_items: Default::default(),
            queue_max_bytes: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_frame_view_t {
    pub struct_size: u32,
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub plane_count: u32,
    pub planes: [*const u8; 4],
    pub strides: [u32; 4],
    pub plane_size_bytes: [u64; 4],
    pub color_matrix: u32,
    pub color_range: u32,
}
impl Default for am_frame_view_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            width: Default::default(),
            height: Default::default(),
            format: Default::default(),
            plane_count: Default::default(),
            planes: [Default::default(); 4],
            strides: [Default::default(); 4],
            plane_size_bytes: [Default::default(); 4],
            color_matrix: Default::default(),
            color_range: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_spatial_transform_t {
    pub scale_x: f32,
    pub scale_y: f32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub rotation: u32,
    pub valid: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_frame_meta_t {
    pub struct_size: u32,
    pub source_id: u64,
    pub frame_id: u64,
    pub seq: u64,
    pub source_generation: u32,
    pub config_generation: u32,
    pub pts: i64,
    pub time_base_num: u32,
    pub time_base_den: u32,
    pub received_at_ms: i64,
    pub captured_at_ms: i64,
    pub clock_mask: u32,
    pub clock_quality: u32,
    pub transform: am_spatial_transform_t,
}
impl Default for am_frame_meta_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            source_id: Default::default(),
            frame_id: Default::default(),
            seq: Default::default(),
            source_generation: Default::default(),
            config_generation: Default::default(),
            pts: Default::default(),
            time_base_num: Default::default(),
            time_base_den: Default::default(),
            received_at_ms: Default::default(),
            captured_at_ms: Default::default(),
            clock_mask: Default::default(),
            clock_quality: Default::default(),
            transform: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_scale_preset_t {
    pub width: u32,
    pub height: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_frame_sub_desc_t {
    pub struct_size: u32,
    pub pixel_format: u32,
    pub sampling_max_fps: u32,
    pub preset_count: u32,
    pub presets: *const am_scale_preset_t,
    pub mode: u32,
    pub queue_max_items: u32,
    pub queue_max_bytes: u64,
    pub frame_kind: u32,
}
impl Default for am_frame_sub_desc_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            pixel_format: Default::default(),
            sampling_max_fps: Default::default(),
            preset_count: Default::default(),
            presets: Default::default(),
            mode: Default::default(),
            queue_max_items: Default::default(),
            queue_max_bytes: Default::default(),
            frame_kind: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_decoder_config_t {
    pub struct_size: u32,
    pub codec: u32,
    pub bitstream_format: u32,
    pub output_pixel_format: u32,
    pub hw_policy: u32,
    pub extradata: *const u8,
    pub extradata_size: u64,
    pub time_base_num: u32,
    pub time_base_den: u32,
    pub queue_max_items: u32,
    pub queue_max_bytes: u64,
    pub reserved: [u32; 4],
}
impl Default for am_decoder_config_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            codec: Default::default(),
            bitstream_format: Default::default(),
            output_pixel_format: Default::default(),
            hw_policy: Default::default(),
            extradata: Default::default(),
            extradata_size: Default::default(),
            time_base_num: Default::default(),
            time_base_den: Default::default(),
            queue_max_items: Default::default(),
            queue_max_bytes: Default::default(),
            reserved: [Default::default(); 4],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_decoder_input_t {
    pub struct_size: u32,
    pub flags: u32,
    pub size: u64,
    pub data: *const u8,
    pub pts: i64,
    pub dts: i64,
}
impl Default for am_decoder_input_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            flags: Default::default(),
            size: Default::default(),
            data: Default::default(),
            pts: Default::default(),
            dts: Default::default(),
        }
    }
}
