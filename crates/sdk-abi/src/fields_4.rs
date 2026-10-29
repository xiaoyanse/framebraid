// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
impl AbiLayout for am_backend_candidate_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_backend_candidate_t, backend),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_candidate_t, device_id),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_backend_selection_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_backend_selection_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_selection_t, candidate_count),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_selection_t, candidates),
            core::mem::size_of::<*const am_backend_candidate_t>(),
        ),
        (
            core::mem::offset_of!(am_backend_selection_t, allow_fallback),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_selection_t, budget_ms),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_backend_policy_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_backend_policy_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_policy_t, requested),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_policy_t, strict),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_raw_preprocess_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_raw_preprocess_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_raw_preprocess_t, resize),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_raw_preprocess_t, target_w),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_raw_preprocess_t, target_h),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_raw_preprocess_t, fill_value),
            core::mem::size_of::<f32>(),
        ),
        (
            core::mem::offset_of!(am_raw_preprocess_t, color_order),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_raw_preprocess_t, normalize),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_raw_preprocess_t, mean),
            core::mem::size_of::<[f32; 3]>(),
        ),
        (
            core::mem::offset_of!(am_raw_preprocess_t, std),
            core::mem::size_of::<[f32; 3]>(),
        ),
        (
            core::mem::offset_of!(am_raw_preprocess_t, layout),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_model_desc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_model_desc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_desc_t, path_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_model_desc_t, load_mode),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_desc_t, profile_id),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_model_desc_t, backend),
            core::mem::size_of::<*const am_backend_policy_t>(),
        ),
        (
            core::mem::offset_of!(am_model_desc_t, intra_threads),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_model_memory_desc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_model_memory_desc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_memory_desc_t, bytes),
            core::mem::size_of::<*const u8>(),
        ),
        (
            core::mem::offset_of!(am_model_memory_desc_t, size),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_model_memory_desc_t, load_mode),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_memory_desc_t, profile_id),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_model_memory_desc_t, backend),
            core::mem::size_of::<*const am_backend_policy_t>(),
        ),
    ];
}
impl AbiLayout for am_encoder_config_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_encoder_config_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoder_config_t, width),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoder_config_t, height),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoder_config_t, bitrate_kbps),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoder_config_t, gop_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoder_config_t, fps_num),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoder_config_t, fps_den),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoder_config_t, queue_max_items),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_encoder_config_t, queue_max_bytes),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_model_swap_desc_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_model_swap_desc_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_swap_desc_t, slot_key),
            core::mem::size_of::<u64>(),
        ),
        (
            core::mem::offset_of!(am_model_swap_desc_t, flags),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_model_swap_desc_t, reserved),
            core::mem::size_of::<u32>(),
        ),
    ];
}
impl AbiLayout for am_io_spec_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_io_spec_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_io_spec_t, name),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_io_spec_t, dtype),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_io_spec_t, shape),
            core::mem::size_of::<*const i64>(),
        ),
        (
            core::mem::offset_of!(am_io_spec_t, shape_len),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_io_spec_t, dyn_min),
            core::mem::size_of::<*const i64>(),
        ),
        (
            core::mem::offset_of!(am_io_spec_t, dyn_max),
            core::mem::size_of::<*const i64>(),
        ),
    ];
}
impl AbiLayout for am_backend_actual_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_backend_actual_t, actual),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_backend_actual_t, device_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
        (
            core::mem::offset_of!(am_backend_actual_t, subgraph_cpu_nodes),
            core::mem::size_of::<i32>(),
        ),
        (
            core::mem::offset_of!(am_backend_actual_t, fallback_reason_utf8),
            core::mem::size_of::<*const c_char>(),
        ),
    ];
}
