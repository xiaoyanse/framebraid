// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
#[test]
fn am_probe_result_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_probe_result_t>(), 16);
    assert_eq!(core::mem::align_of::<am_probe_result_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_probe_result_t, struct_size),
        0,
        "am_probe_result_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_result_t, candidate_count),
        4,
        "am_probe_result_t.candidate_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_probe_result_t, candidates),
        8,
        "am_probe_result_t.candidates"
    );
}
#[test]
fn am_encoded_packet_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_encoded_packet_t>(), 80);
    assert_eq!(core::mem::align_of::<am_encoded_packet_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, struct_size),
        0,
        "am_encoded_packet_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, codec),
        4,
        "am_encoded_packet_t.codec"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, bitstream_format),
        8,
        "am_encoded_packet_t.bitstream_format"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, flags),
        12,
        "am_encoded_packet_t.flags"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, config_generation),
        16,
        "am_encoded_packet_t.config_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, source_generation),
        20,
        "am_encoded_packet_t.source_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, source_id),
        24,
        "am_encoded_packet_t.source_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, seq),
        32,
        "am_encoded_packet_t.seq"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, pts),
        40,
        "am_encoded_packet_t.pts"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, dts),
        48,
        "am_encoded_packet_t.dts"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, time_base_num),
        56,
        "am_encoded_packet_t.time_base_num"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, time_base_den),
        60,
        "am_encoded_packet_t.time_base_den"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, data),
        64,
        "am_encoded_packet_t.data"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_packet_t, size),
        72,
        "am_encoded_packet_t.size"
    );
}
#[test]
fn am_encoded_sub_desc_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_encoded_sub_desc_t>(), 24);
    assert_eq!(core::mem::align_of::<am_encoded_sub_desc_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_encoded_sub_desc_t, struct_size),
        0,
        "am_encoded_sub_desc_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_sub_desc_t, codec_mask),
        4,
        "am_encoded_sub_desc_t.codec_mask"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_sub_desc_t, mode),
        8,
        "am_encoded_sub_desc_t.mode"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_sub_desc_t, queue_max_items),
        12,
        "am_encoded_sub_desc_t.queue_max_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_encoded_sub_desc_t, queue_max_bytes),
        16,
        "am_encoded_sub_desc_t.queue_max_bytes"
    );
}
#[test]
fn am_frame_view_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_frame_view_t>(), 112);
    assert_eq!(core::mem::align_of::<am_frame_view_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_frame_view_t, struct_size),
        0,
        "am_frame_view_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_view_t, width),
        4,
        "am_frame_view_t.width"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_view_t, height),
        8,
        "am_frame_view_t.height"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_view_t, format),
        12,
        "am_frame_view_t.format"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_view_t, plane_count),
        16,
        "am_frame_view_t.plane_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_view_t, planes),
        24,
        "am_frame_view_t.planes"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_view_t, strides),
        56,
        "am_frame_view_t.strides"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_view_t, plane_size_bytes),
        72,
        "am_frame_view_t.plane_size_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_view_t, color_matrix),
        104,
        "am_frame_view_t.color_matrix"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_view_t, color_range),
        108,
        "am_frame_view_t.color_range"
    );
}
#[test]
fn am_spatial_transform_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_spatial_transform_t>(), 24);
    assert_eq!(core::mem::align_of::<am_spatial_transform_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_spatial_transform_t, scale_x),
        0,
        "am_spatial_transform_t.scale_x"
    );
    assert_eq!(
        core::mem::offset_of!(am_spatial_transform_t, scale_y),
        4,
        "am_spatial_transform_t.scale_y"
    );
    assert_eq!(
        core::mem::offset_of!(am_spatial_transform_t, offset_x),
        8,
        "am_spatial_transform_t.offset_x"
    );
    assert_eq!(
        core::mem::offset_of!(am_spatial_transform_t, offset_y),
        12,
        "am_spatial_transform_t.offset_y"
    );
    assert_eq!(
        core::mem::offset_of!(am_spatial_transform_t, rotation),
        16,
        "am_spatial_transform_t.rotation"
    );
    assert_eq!(
        core::mem::offset_of!(am_spatial_transform_t, valid),
        20,
        "am_spatial_transform_t.valid"
    );
}
#[test]
fn am_frame_meta_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_frame_meta_t>(), 104);
    assert_eq!(core::mem::align_of::<am_frame_meta_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, struct_size),
        0,
        "am_frame_meta_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, source_id),
        8,
        "am_frame_meta_t.source_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, frame_id),
        16,
        "am_frame_meta_t.frame_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, seq),
        24,
        "am_frame_meta_t.seq"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, source_generation),
        32,
        "am_frame_meta_t.source_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, config_generation),
        36,
        "am_frame_meta_t.config_generation"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, pts),
        40,
        "am_frame_meta_t.pts"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, time_base_num),
        48,
        "am_frame_meta_t.time_base_num"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, time_base_den),
        52,
        "am_frame_meta_t.time_base_den"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, received_at_ms),
        56,
        "am_frame_meta_t.received_at_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, captured_at_ms),
        64,
        "am_frame_meta_t.captured_at_ms"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, clock_mask),
        72,
        "am_frame_meta_t.clock_mask"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, clock_quality),
        76,
        "am_frame_meta_t.clock_quality"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_meta_t, transform),
        80,
        "am_frame_meta_t.transform"
    );
}
#[test]
fn am_scale_preset_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_scale_preset_t>(), 8);
    assert_eq!(core::mem::align_of::<am_scale_preset_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_scale_preset_t, width),
        0,
        "am_scale_preset_t.width"
    );
    assert_eq!(
        core::mem::offset_of!(am_scale_preset_t, height),
        4,
        "am_scale_preset_t.height"
    );
}
#[test]
fn am_frame_sub_desc_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_frame_sub_desc_t>(), 40);
    assert_eq!(core::mem::align_of::<am_frame_sub_desc_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_frame_sub_desc_t, struct_size),
        0,
        "am_frame_sub_desc_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_sub_desc_t, pixel_format),
        4,
        "am_frame_sub_desc_t.pixel_format"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_sub_desc_t, sampling_max_fps),
        8,
        "am_frame_sub_desc_t.sampling_max_fps"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_sub_desc_t, preset_count),
        12,
        "am_frame_sub_desc_t.preset_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_sub_desc_t, presets),
        16,
        "am_frame_sub_desc_t.presets"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_sub_desc_t, mode),
        24,
        "am_frame_sub_desc_t.mode"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_sub_desc_t, queue_max_items),
        28,
        "am_frame_sub_desc_t.queue_max_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_frame_sub_desc_t, queue_max_bytes),
        32,
        "am_frame_sub_desc_t.queue_max_bytes"
    );
}
#[test]
fn am_decoder_config_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_decoder_config_t>(), 80);
    assert_eq!(core::mem::align_of::<am_decoder_config_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, struct_size),
        0,
        "am_decoder_config_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, codec),
        4,
        "am_decoder_config_t.codec"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, bitstream_format),
        8,
        "am_decoder_config_t.bitstream_format"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, output_pixel_format),
        12,
        "am_decoder_config_t.output_pixel_format"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, hw_policy),
        16,
        "am_decoder_config_t.hw_policy"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, extradata),
        24,
        "am_decoder_config_t.extradata"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, extradata_size),
        32,
        "am_decoder_config_t.extradata_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, time_base_num),
        40,
        "am_decoder_config_t.time_base_num"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, time_base_den),
        44,
        "am_decoder_config_t.time_base_den"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, queue_max_items),
        48,
        "am_decoder_config_t.queue_max_items"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, queue_max_bytes),
        56,
        "am_decoder_config_t.queue_max_bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_config_t, reserved),
        64,
        "am_decoder_config_t.reserved"
    );
}
#[test]
fn am_decoder_input_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_decoder_input_t>(), 40);
    assert_eq!(core::mem::align_of::<am_decoder_input_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_decoder_input_t, struct_size),
        0,
        "am_decoder_input_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_input_t, flags),
        4,
        "am_decoder_input_t.flags"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_input_t, size),
        8,
        "am_decoder_input_t.size"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_input_t, data),
        16,
        "am_decoder_input_t.data"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_input_t, pts),
        24,
        "am_decoder_input_t.pts"
    );
    assert_eq!(
        core::mem::offset_of!(am_decoder_input_t, dts),
        32,
        "am_decoder_input_t.dts"
    );
}
