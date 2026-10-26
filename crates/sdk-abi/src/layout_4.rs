// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
#[test]
fn am_backend_candidate_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_backend_candidate_t>(), 8);
    assert_eq!(core::mem::align_of::<am_backend_candidate_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_backend_candidate_t, backend),
        0,
        "am_backend_candidate_t.backend"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_candidate_t, device_id),
        4,
        "am_backend_candidate_t.device_id"
    );
}
#[test]
fn am_backend_selection_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_backend_selection_t>(), 24);
    assert_eq!(core::mem::align_of::<am_backend_selection_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_backend_selection_t, struct_size),
        0,
        "am_backend_selection_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_selection_t, candidate_count),
        4,
        "am_backend_selection_t.candidate_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_selection_t, candidates),
        8,
        "am_backend_selection_t.candidates"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_selection_t, allow_fallback),
        16,
        "am_backend_selection_t.allow_fallback"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_selection_t, budget_ms),
        20,
        "am_backend_selection_t.budget_ms"
    );
}
#[test]
fn am_backend_policy_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_backend_policy_t>(), 12);
    assert_eq!(core::mem::align_of::<am_backend_policy_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_backend_policy_t, struct_size),
        0,
        "am_backend_policy_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_policy_t, requested),
        4,
        "am_backend_policy_t.requested"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_policy_t, strict),
        8,
        "am_backend_policy_t.strict"
    );
}
#[test]
fn am_raw_preprocess_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_raw_preprocess_t>(), 56);
    assert_eq!(core::mem::align_of::<am_raw_preprocess_t>(), 4);
    assert_eq!(
        core::mem::offset_of!(am_raw_preprocess_t, struct_size),
        0,
        "am_raw_preprocess_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_raw_preprocess_t, resize),
        4,
        "am_raw_preprocess_t.resize"
    );
    assert_eq!(
        core::mem::offset_of!(am_raw_preprocess_t, target_w),
        8,
        "am_raw_preprocess_t.target_w"
    );
    assert_eq!(
        core::mem::offset_of!(am_raw_preprocess_t, target_h),
        12,
        "am_raw_preprocess_t.target_h"
    );
    assert_eq!(
        core::mem::offset_of!(am_raw_preprocess_t, fill_value),
        16,
        "am_raw_preprocess_t.fill_value"
    );
    assert_eq!(
        core::mem::offset_of!(am_raw_preprocess_t, color_order),
        20,
        "am_raw_preprocess_t.color_order"
    );
    assert_eq!(
        core::mem::offset_of!(am_raw_preprocess_t, normalize),
        24,
        "am_raw_preprocess_t.normalize"
    );
    assert_eq!(
        core::mem::offset_of!(am_raw_preprocess_t, mean),
        28,
        "am_raw_preprocess_t.mean"
    );
    assert_eq!(
        core::mem::offset_of!(am_raw_preprocess_t, std),
        40,
        "am_raw_preprocess_t.std"
    );
    assert_eq!(
        core::mem::offset_of!(am_raw_preprocess_t, layout),
        52,
        "am_raw_preprocess_t.layout"
    );
}
#[test]
fn am_model_desc_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_model_desc_t>(), 48);
    assert_eq!(core::mem::align_of::<am_model_desc_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_model_desc_t, struct_size),
        0,
        "am_model_desc_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_desc_t, path_utf8),
        8,
        "am_model_desc_t.path_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_desc_t, load_mode),
        16,
        "am_model_desc_t.load_mode"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_desc_t, profile_id),
        24,
        "am_model_desc_t.profile_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_desc_t, backend),
        32,
        "am_model_desc_t.backend"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_desc_t, intra_threads),
        40,
        "am_model_desc_t.intra_threads"
    );
}
#[test]
fn am_model_memory_desc_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_model_memory_desc_t>(), 48);
    assert_eq!(core::mem::align_of::<am_model_memory_desc_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_model_memory_desc_t, struct_size),
        0,
        "am_model_memory_desc_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_memory_desc_t, bytes),
        8,
        "am_model_memory_desc_t.bytes"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_memory_desc_t, size),
        16,
        "am_model_memory_desc_t.size"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_memory_desc_t, load_mode),
        24,
        "am_model_memory_desc_t.load_mode"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_memory_desc_t, profile_id),
        32,
        "am_model_memory_desc_t.profile_id"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_memory_desc_t, backend),
        40,
        "am_model_memory_desc_t.backend"
    );
}
#[test]
fn am_io_spec_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_io_spec_t>(), 56);
    assert_eq!(core::mem::align_of::<am_io_spec_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_io_spec_t, struct_size),
        0,
        "am_io_spec_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_io_spec_t, name),
        8,
        "am_io_spec_t.name"
    );
    assert_eq!(
        core::mem::offset_of!(am_io_spec_t, dtype),
        16,
        "am_io_spec_t.dtype"
    );
    assert_eq!(
        core::mem::offset_of!(am_io_spec_t, shape),
        24,
        "am_io_spec_t.shape"
    );
    assert_eq!(
        core::mem::offset_of!(am_io_spec_t, shape_len),
        32,
        "am_io_spec_t.shape_len"
    );
    assert_eq!(
        core::mem::offset_of!(am_io_spec_t, dyn_min),
        40,
        "am_io_spec_t.dyn_min"
    );
    assert_eq!(
        core::mem::offset_of!(am_io_spec_t, dyn_max),
        48,
        "am_io_spec_t.dyn_max"
    );
}
#[test]
fn am_backend_actual_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_backend_actual_t>(), 32);
    assert_eq!(core::mem::align_of::<am_backend_actual_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_backend_actual_t, actual),
        0,
        "am_backend_actual_t.actual"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_actual_t, device_utf8),
        8,
        "am_backend_actual_t.device_utf8"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_actual_t, subgraph_cpu_nodes),
        16,
        "am_backend_actual_t.subgraph_cpu_nodes"
    );
    assert_eq!(
        core::mem::offset_of!(am_backend_actual_t, fallback_reason_utf8),
        24,
        "am_backend_actual_t.fallback_reason_utf8"
    );
}
#[test]
fn am_model_identity_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_model_identity_t>(), 24);
    assert_eq!(core::mem::align_of::<am_model_identity_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_model_identity_t, model_uid),
        0,
        "am_model_identity_t.model_uid"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_identity_t, model_version),
        8,
        "am_model_identity_t.model_version"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_identity_t, profile_id),
        16,
        "am_model_identity_t.profile_id"
    );
}
#[test]
fn am_model_info_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_model_info_t>(), 104);
    assert_eq!(core::mem::align_of::<am_model_info_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, struct_size),
        0,
        "am_model_info_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, identity),
        8,
        "am_model_info_t.identity"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, load_mode),
        32,
        "am_model_info_t.load_mode"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, input_count),
        36,
        "am_model_info_t.input_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, output_count),
        40,
        "am_model_info_t.output_count"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, inputs),
        48,
        "am_model_info_t.inputs"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, outputs),
        56,
        "am_model_info_t.outputs"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, native_batch),
        64,
        "am_model_info_t.native_batch"
    );
    assert_eq!(
        core::mem::offset_of!(am_model_info_t, execution),
        72,
        "am_model_info_t.execution"
    );
}
