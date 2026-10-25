// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
impl AbiLayout for am_probe_result_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_probe_result_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_probe_result_t, candidate_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_probe_result_t, candidates),
            core::mem::size_of::<*const am_probe_candidate_t>(),
        ),
    ];
}
impl AbiLayout for am_encoded_packet_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_encoded_packet_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, codec),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, bitstream_format),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, flags),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, config_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, source_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, source_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, seq),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, pts),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, dts),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, time_base_num),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, time_base_den),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, data),
            core::mem::size_of::<*const u8>(),
        ),
        (
            core::mem::offset_of!(am_encoded_packet_t, size),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for am_encoded_sub_desc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_encoded_sub_desc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_sub_desc_t, codec_mask),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_sub_desc_t, mode),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_sub_desc_t, queue_max_items),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoded_sub_desc_t, queue_max_bytes),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for am_frame_view_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_frame_view_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_view_t, width),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_view_t, height),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_view_t, format),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_view_t, plane_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_view_t, planes),
            core::mem::size_of::<[*const u8; 4]>(),
        ),
        (
            core::mem::offset_of!(am_frame_view_t, strides),
            core::mem::size_of::<[u32; 4]>(),
        ),
        (
            core::mem::offset_of!(am_frame_view_t, plane_size_bytes),
            core::mem::size_of::<[u64; 4]>(),
        ),
        (
            core::mem::offset_of!(am_frame_view_t, color_matrix),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_view_t, color_range),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_spatial_transform_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_spatial_transform_t, scale_x),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_spatial_transform_t, scale_y),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_spatial_transform_t, offset_x),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_spatial_transform_t, offset_y),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_spatial_transform_t, rotation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_spatial_transform_t, valid),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_frame_meta_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_frame_meta_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, source_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, frame_id),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, seq),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, source_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, config_generation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, pts),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, time_base_num),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, time_base_den),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, received_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, captured_at_ms),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, clock_mask),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, clock_quality),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, transform)
                + core::mem::offset_of!(am_spatial_transform_t, scale_x),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, transform)
                + core::mem::offset_of!(am_spatial_transform_t, scale_y),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, transform)
                + core::mem::offset_of!(am_spatial_transform_t, offset_x),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, transform)
                + core::mem::offset_of!(am_spatial_transform_t, offset_y),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, transform)
                + core::mem::offset_of!(am_spatial_transform_t, rotation),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_meta_t, transform)
                + core::mem::offset_of!(am_spatial_transform_t, valid),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_scale_preset_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_scale_preset_t, width),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_scale_preset_t, height),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_frame_sub_desc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_frame_sub_desc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_sub_desc_t, pixel_format),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_sub_desc_t, sampling_max_fps),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_sub_desc_t, preset_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_sub_desc_t, presets),
            core::mem::size_of::<*const am_scale_preset_t>(),
        ),
        (
            core::mem::offset_of!(am_frame_sub_desc_t, mode),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_sub_desc_t, queue_max_items),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_frame_sub_desc_t, queue_max_bytes),
            core::mem::size_of::<u64>(),
        ),
    ];
}
impl AbiLayout for am_decoder_config_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_decoder_config_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_config_t, codec),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_config_t, bitstream_format),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_config_t, output_pixel_format),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_config_t, hw_policy),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_config_t, extradata),
            core::mem::size_of::<*const u8>(),
        ),
        (
            core::mem::offset_of!(am_decoder_config_t, extradata_size),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_decoder_config_t, time_base_num),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_config_t, time_base_den),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_config_t, queue_max_items),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_config_t, queue_max_bytes),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_decoder_config_t, reserved),
            core::mem::size_of::<[u32; 4]>(),
        ),
    ];
}
impl AbiLayout for am_decoder_input_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_decoder_input_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_input_t, flags),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_decoder_input_t, size),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_decoder_input_t, data),
            core::mem::size_of::<*const u8>(),
        ),
        (
            core::mem::offset_of!(am_decoder_input_t, pts),
            core::mem::size_of::<i64>(),
        ),
        (
            core::mem::offset_of!(am_decoder_input_t, dts),
            core::mem::size_of::<i64>(),
        ),
    ];
}
