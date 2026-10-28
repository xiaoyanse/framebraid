# 由scripts/sdk/generate-python-abi.mjs生成，与C头文件同源，勿手改。
from ctypes import *

class am_version_info_t(Structure):
    pass

class amc_init_params_t(Structure):
    pass

class amv_init_params_t(Structure):
    pass

class am_error_info_t(Structure):
    pass

class am_init_error_t(Structure):
    pass

class am_request_snapshot_t(Structure):
    pass

class am_queue_policy_t(Structure):
    pass

class am_credential_data_t(Structure):
    pass

class am_credential_provider_t(Structure):
    pass

class am_shared_frame_desc_t(Structure):
    pass

class am_shared_pool_config_t(Structure):
    pass

class am_shared_pool_attach_desc_t(Structure):
    pass

class am_shared_frame_view_t(Structure):
    pass

class am_shared_pool_stats_t(Structure):
    pass

class amc_capability_t(Structure):
    pass

class am_source_desc_t(Structure):
    pass

class am_source_status_t(Structure):
    pass

class am_reconnect_policy_t(Structure):
    pass

class am_probe_candidate_t(Structure):
    pass

class am_probe_request_t(Structure):
    pass

class am_probe_result_t(Structure):
    pass

class am_encoded_packet_t(Structure):
    pass

class am_encoded_sub_desc_t(Structure):
    pass

class am_frame_view_t(Structure):
    pass

class am_spatial_transform_t(Structure):
    pass

class am_frame_meta_t(Structure):
    pass

class am_scale_preset_t(Structure):
    pass

class am_frame_sub_desc_t(Structure):
    pass

class am_decoder_config_t(Structure):
    pass

class am_decoder_input_t(Structure):
    pass

class am_decoder_status_t(Structure):
    pass

class am_decoder_info_t(Structure):
    pass

class am_capture_request_t(Structure):
    pass

class am_captured_image_t(Structure):
    pass

class am_capture_continuous_desc_t(Structure):
    pass

class am_capture_stats_t(Structure):
    pass

class am_event_detail_t(Structure):
    pass

class amv_capability_t(Structure):
    pass

class am_backend_row_t(Structure):
    pass

class am_backend_matrix_t(Structure):
    pass

class am_backend_candidate_t(Structure):
    pass

class am_backend_selection_t(Structure):
    pass

class am_backend_policy_t(Structure):
    pass

class am_raw_preprocess_t(Structure):
    pass

class am_model_desc_t(Structure):
    pass

class am_model_memory_desc_t(Structure):
    pass

class am_io_spec_t(Structure):
    pass

class am_backend_actual_t(Structure):
    pass

class am_model_identity_t(Structure):
    pass

class am_model_info_t(Structure):
    pass

class am_frame_assoc_t(Structure):
    pass

class am_gpu_frame_desc_t(Structure):
    pass

class am_gpu_frame_stats_t(Structure):
    pass

class am_gpu_frame_ref_t(Structure):
    pass

class am_image_input_t(Structure):
    pass

class am_infer_params_t(Structure):
    pass

class am_tensor_in_t(Structure):
    pass

class am_batch_item_t(Structure):
    pass

class am_batch_options_t(Structure):
    pass

class am_batch_receipt_t(Structure):
    pass

class am_batching_options_t(Structure):
    pass

class am_batching_stats_t(Structure):
    pass

class am_preprocess_meta_t(Structure):
    pass

class am_tensor_out_t(Structure):
    pass

class am_keypoint_t(Structure):
    pass

class am_pose_detection_t(Structure):
    pass

class am_pose_result_t(Structure):
    pass

class am_detect_box_t(Structure):
    pass

class am_detect_result_t(Structure):
    pass

class am_classify_entry_t(Structure):
    pass

class am_classify_result_t(Structure):
    pass

class am_roi_map_t(Structure):
    pass

class am_segment_instance_t(Structure):
    pass

class am_segment_result_t(Structure):
    pass

class am_obb_instance_t(Structure):
    pass

class am_obb_result_t(Structure):
    pass

class amv_result_t(Structure):
    pass

class am_realtime_policy_t(Structure):
    pass

class am_limits_t(Structure):
    pass

class am_stats_t(Structure):
    pass

class am_pixel_view_t(Structure):
    pass

class amc_context_t(Structure):
    pass

class amc_source_t(Structure):
    pass

class amv_context_t(Structure):
    pass

class amc_encoded_sub_t(Structure):
    pass

class amc_frame_sub_t(Structure):
    pass

class amc_frame_t(Structure):
    pass

class amc_gpu_frame_pool_t(Structure):
    pass

class amc_gpu_frame_t(Structure):
    pass

class amc_capture_sub_t(Structure):
    pass

class amc_decoder_t(Structure):
    pass

class amv_model_t(Structure):
    pass

class amc_shared_pool_t(Structure):
    pass

class amv_shared_pool_t(Structure):
    pass

am_status_t = c_int32
am_lifecycle_state_t = c_uint32
am_queue_class_e = c_int32
AM_Q_REALTIME_FRAMES = 0
AM_Q_ENCODED_STREAM = 1
AM_Q_REALTIME_INFER = 2
AM_Q_OFFLINE_BATCH = 3
AM_Q_RESULTS = 4
AM_Q_CONTROL = 5

am_shared_pool_state_e = c_int32
AM_SFP_ENABLED = 0
AM_SFP_DISABLED = 1
AM_SFP_NOT_COMPILED = 2
AM_SFP_PLATFORM_UNSUPPORTED = 3

am_source_kind_e = c_int32
AM_SRC_RTSP_URL = 0
AM_SRC_ISAPI_URL = 1
AM_SRC_DEVICE_REF = 2

am_source_state_e = c_int32
AM_SRC_WAITING_CREDENTIALS = 0
AM_SRC_CONNECTING = 1
AM_SRC_LIVE = 2
AM_SRC_RECONNECT_WAIT = 3
AM_SRC_AUTH_WAITING = 4
AM_SRC_CLOSING = 5
AM_SRC_CLOSED = 6

am_fault_class_e = c_int32
AM_FAULT_NONE = 0
AM_FAULT_NETWORK = 1
AM_FAULT_AUTH = 2
AM_FAULT_RESOURCE = 3
AM_FAULT_KEYFRAME_WAIT = 4
AM_FAULT_DECODE = 5

am_probe_status_e = c_int32
AM_PROBE_OK = 0
AM_PROBE_TIMEOUT = 1
AM_PROBE_AUTH_FAILED = 2
AM_PROBE_UNREACHABLE = 3
AM_PROBE_UNSUPPORTED = 4
AM_PROBE_PENDING = 5

am_pixel_format_e = c_int32
AM_PF_RGBA8 = 0
AM_PF_RGB8 = 1
AM_PF_BGR8 = 2
AM_PF_NV12 = 3

am_color_matrix_e = c_int32
AM_CM_BT601 = 0
AM_CM_BT709 = 1
AM_CM_BT2020_NCL = 2

am_color_range_e = c_int32
AM_CR_LIMITED = 0
AM_CR_FULL = 1

am_clock_quality_e = c_int32
AM_CQ_UNKNOWN = 0
AM_CQ_LOCAL_RECEIPT = 1
AM_CQ_DEVICE_SYNCED = 2
AM_CQ_EXTRAPOLATED = 3

am_decoder_hw_policy_e = c_int32
AM_DHW_PREFER_HW = 0
AM_DHW_SOFTWARE_ONLY = 1

am_decoder_state_e = c_int32
AM_DEC_READY = 0
AM_DEC_DRAINING = 1
AM_DEC_DRAINED = 2
AM_DEC_RESET_PENDING = 3
AM_DEC_CLOSING = 4
AM_DEC_CLOSED = 5

am_event_scope_e = c_int32
AM_EVENT_SCOPE_SINGLE_SUB = 0
AM_EVENT_SCOPE_SOURCE = 1
AM_EVENT_SCOPE_ALL_CONSUMERS = 2

am_backend_e = c_int32
AM_BACKEND_CPU = 0
AM_BACKEND_OPENVINO = 1
AM_BACKEND_CUDA = 2
AM_BACKEND_DIRECTML = 3

am_frame_kind_e = c_int32
AM_FRAME_KIND_PIXELS = 0
AM_FRAME_KIND_GPU = 1

am_dtype_e = c_int32
AM_DTYPE_F32 = 0
AM_DTYPE_F16 = 1
AM_DTYPE_I64 = 2
AM_DTYPE_I32 = 3
AM_DTYPE_U8 = 4

am_task_e = c_int32
AM_TASK_NONE = 0
AM_TASK_POSE = 1
AM_TASK_DETECT = 2
AM_TASK_CLASSIFY = 3
AM_TASK_SEGMENT = 4
AM_TASK_OBB = 5
AM_TASK_GENERIC = 6

am_source_event_e = c_int32
AM_EVENT_STATE_CHANGED = 0
AM_EVENT_GENERATION_CHANGED = 1
AM_EVENT_ENCODED_RESYNC = 2
AM_EVENT_SPATIAL_CHANGED = 3
AM_EVENT_AUTH_WAITING = 4

am_lifecycle_state_e = c_int32
AM_LIFECYCLE_RUNNING = 0
AM_LIFECYCLE_CANCEL_REQUESTED = 1
AM_LIFECYCLE_CLOSING = 2
AM_LIFECYCLE_CLOSED = 3

am_request_state_e = c_int32
AM_REQ_PENDING = 0
AM_REQ_RUNNING = 1
AM_REQ_DONE = 2
AM_REQ_FAILED = 3
AM_REQ_CANCELED = 4
AM_REQ_EXPIRED = 5
AM_REQ_REPLACED = 6
AM_REQ_UNKNOWN = 7

am_credential_fetch_fn = CFUNCTYPE(am_status_t, c_void_p, c_uint32, POINTER(am_credential_data_t), POINTER(c_uint32))
am_credential_drop_fn = CFUNCTYPE(None, c_void_p)
am_encoded_cb_fn = CFUNCTYPE(None, POINTER(amc_encoded_sub_t), POINTER(am_encoded_packet_t), c_void_p)
am_frame_cb_fn = CFUNCTYPE(None, POINTER(amc_frame_sub_t), POINTER(amc_frame_t), c_void_p)
am_capture_cb_fn = CFUNCTYPE(None, POINTER(amc_capture_sub_t), POINTER(am_captured_image_t), c_void_p)
am_source_event_fn = CFUNCTYPE(None, POINTER(amc_source_t), am_source_event_e, POINTER(am_event_detail_t), c_void_p)
am_result_notify_fn = CFUNCTYPE(None, POINTER(amv_model_t), c_uint64, c_uint32, c_void_p)

am_version_info_t._fields_ = [
    ("struct_size", c_uint32),
    ("abi_major", c_uint32),
    ("abi_minor", c_uint32),
    ("package_version_utf8", c_char_p),
    ("build_target_utf8", c_char_p),
    ("runtime_notes_utf8", c_char_p),
]

amv_init_params_t._fields_ = [
    ("struct_size", c_uint32),
    ("abi_version", c_uint32),
    ("runtime_dirs_utf8", POINTER(c_char_p)),
    ("runtime_dir_count", c_uint32),
    ("infer_threads", c_uint32),
    ("domain_infer_slots", c_uint64),
    ("total_thread_budget", c_uint32),
    ("reserved", c_uint32 * 3),
    ("feature_flags", c_uint64),
]

am_init_error_t._fields_ = [
    ("struct_size", c_uint32),
    ("code", am_status_t),
    ("message", c_char * 256),
    ("source_utf8", c_char_p),
    ("failed_at_ms", c_int64),
    ("attempt_seq", c_uint64),
]

am_queue_policy_t._fields_ = [
    ("struct_size", c_uint32),
    ("max_items", c_uint32),
    ("max_bytes", c_uint64),
    ("max_waiters", c_uint32),
    ("wait_deadline_ms", c_uint32),
    ("ttl_ms", c_uint32),
    ("reserved", c_uint32 * 4),
]

am_credential_provider_t._fields_ = [
    ("struct_size", c_uint32),
    ("fetch", am_credential_fetch_fn),
    ("on_drop", am_credential_drop_fn),
    ("user_data", c_void_p),
    ("fetch_timeout_ms", c_uint32),
]

am_shared_pool_config_t._fields_ = [
    ("struct_size", c_uint32),
    ("slot_count", c_uint32),
    ("max_frame_bytes", c_uint32),
    ("total_bytes", c_uint64),
    ("max_consumers", c_uint32),
    ("max_outstanding_borrows", c_uint32),
    ("drop_policy", c_uint32),
    ("publish_wait_ms", c_uint32),
    ("reserved", c_uint32 * 4),
]

am_shared_pool_stats_t._fields_ = [
    ("struct_size", c_uint32),
    ("published", c_uint64),
    ("dropped_stale", c_uint64),
    ("dropped_full", c_uint64),
    ("wait_timeouts", c_uint64),
    ("slots_in_use", c_uint32),
    ("slots_borrowed", c_uint32),
    ("outstanding_borrows", c_uint32),
    ("consumers_attached", c_uint32),
    ("bytes_in_use", c_uint64),
    ("metadata_bytes", c_uint64),
    ("reserved", c_uint32 * 4),
]

am_source_status_t._fields_ = [
    ("struct_size", c_uint32),
    ("state", am_source_state_e),
    ("source_id", c_uint64),
    ("generation", c_uint32),
    ("config_generation", c_uint32),
    ("fault_class", c_uint32),
    ("reconnect_attempts", c_uint32),
    ("last_frame_at_ms", c_int64),
]

am_probe_candidate_t._fields_ = [
    ("struct_size", c_uint32),
    ("reference_utf8", c_char_p),
    ("status", am_probe_status_e),
    ("codec_utf8", c_char_p),
    ("width", c_uint32),
    ("height", c_uint32),
    ("fps", c_double),
    ("observed_at_ms", c_int64),
    ("valid_until_ms", c_int64),
]

am_probe_result_t._fields_ = [
    ("struct_size", c_uint32),
    ("candidate_count", c_uint32),
    ("candidates", POINTER(am_probe_candidate_t)),
]

am_encoded_sub_desc_t._fields_ = [
    ("struct_size", c_uint32),
    ("codec_mask", c_uint32),
    ("mode", c_uint32),
    ("queue_max_items", c_uint32),
    ("queue_max_bytes", c_uint64),
]

am_spatial_transform_t._fields_ = [
    ("scale_x", c_float),
    ("scale_y", c_float),
    ("offset_x", c_int32),
    ("offset_y", c_int32),
    ("rotation", c_uint32),
    ("valid", c_uint32),
]

am_scale_preset_t._fields_ = [
    ("width", c_uint32),
    ("height", c_uint32),
]

am_decoder_config_t._fields_ = [
    ("struct_size", c_uint32),
    ("codec", c_uint32),
    ("bitstream_format", c_uint32),
    ("output_pixel_format", c_uint32),
    ("hw_policy", c_uint32),
    ("extradata", POINTER(c_uint8)),
    ("extradata_size", c_uint64),
    ("time_base_num", c_uint32),
    ("time_base_den", c_uint32),
    ("queue_max_items", c_uint32),
    ("queue_max_bytes", c_uint64),
    ("reserved", c_uint32 * 4),
]

am_decoder_status_t._fields_ = [
    ("struct_size", c_uint32),
    ("state", am_decoder_state_e),
    ("decoder_generation", c_uint32),
    ("config_generation", c_uint32),
    ("queue_items", c_uint32),
    ("queue_bytes", c_uint64),
]

am_capture_request_t._fields_ = [
    ("struct_size", c_uint32),
    ("output_kind", c_uint32),
    ("deadline_ms", c_uint32),
    ("reserved", c_uint32 * 4),
]

am_capture_continuous_desc_t._fields_ = [
    ("struct_size", c_uint32),
    ("target_interval_ms", c_uint32),
    ("max_inflight", c_uint32),
    ("request_timeout_ms", c_uint32),
    ("output_kind", c_uint32),
    ("mode", c_uint32),
    ("queue_max_items", c_uint32),
    ("queue_max_bytes", c_uint64),
    ("reserved", c_uint32 * 4),
]

am_event_detail_t._fields_ = [
    ("struct_size", c_uint32),
    ("generation", c_uint32),
    ("scope", am_event_scope_e),
    ("affected_handle", c_void_p),
    ("fault_class", c_uint32),
    ("reason_utf8", c_char_p),
]

am_backend_row_t._fields_ = [
    ("backend", c_uint32),
    ("hw_detected", c_uint32),
    ("deps_present", c_uint32),
    ("provider_loadable", c_uint32),
    ("verified", c_uint32),
    ("note_utf8", c_char_p),
]

am_backend_candidate_t._fields_ = [
    ("backend", c_uint32),
    ("device_id", c_uint32),
]

am_backend_policy_t._fields_ = [
    ("struct_size", c_uint32),
    ("requested", c_uint32),
    ("strict", c_uint32),
]

am_model_desc_t._fields_ = [
    ("struct_size", c_uint32),
    ("path_utf8", c_char_p),
    ("load_mode", c_uint32),
    ("profile_id", c_char_p),
    ("backend", POINTER(am_backend_policy_t)),
    ("intra_threads", c_uint32),
]

am_io_spec_t._fields_ = [
    ("struct_size", c_uint32),
    ("name", c_char_p),
    ("dtype", c_uint32),
    ("shape", POINTER(c_int64)),
    ("shape_len", c_uint32),
    ("dyn_min", POINTER(c_int64)),
    ("dyn_max", POINTER(c_int64)),
]

am_model_identity_t._fields_ = [
    ("model_uid", c_uint64),
    ("model_version", c_uint32),
    ("profile_id", c_char_p),
]

am_frame_assoc_t._fields_ = [
    ("struct_size", c_uint32),
    ("source_id", c_uint64),
    ("frame_id", c_uint64),
    ("source_generation", c_uint32),
    ("config_generation", c_uint32),
    ("pts", c_int64),
    ("time_base_num", c_uint32),
    ("time_base_den", c_uint32),
    ("captured_at_ms", c_int64),
    ("received_at_ms", c_int64),
    ("clock_mask", c_uint32),
    ("clock_quality", c_uint32),
    ("valid", c_uint32),
]

am_gpu_frame_stats_t._fields_ = [
    ("struct_size", c_uint32),
    ("gpu_frames_delivered", c_uint64),
    ("cpu_fallbacks", c_uint64),
    ("bytes_transferred_est", c_uint64),
]

am_tensor_in_t._fields_ = [
    ("name", c_char_p),
    ("dtype", c_uint32),
    ("shape", POINTER(c_int64)),
    ("shape_len", c_uint32),
    ("data", c_void_p),
    ("data_size", c_uint64),
    ("flags", c_uint32),
    ("assoc", POINTER(am_frame_assoc_t)),
]

am_batch_options_t._fields_ = [
    ("struct_size", c_uint32),
    ("batch_mode", c_uint32),
    ("wait_deadline_ms", c_uint32),
]

am_batching_options_t._fields_ = [
    ("struct_size", c_uint32),
    ("abi_minor", c_uint32),
    ("max_batch_items", c_uint32),
    ("max_wait_ms", c_uint32),
    ("flags", c_uint32),
]

am_preprocess_meta_t._fields_ = [
    ("struct_size", c_uint32),
    ("src_w", c_uint32),
    ("src_h", c_uint32),
    ("dst_w", c_uint32),
    ("dst_h", c_uint32),
    ("scale_x", c_float),
    ("scale_y", c_float),
    ("crop_x", c_int32),
    ("crop_y", c_int32),
    ("pad_left", c_uint32),
    ("pad_top", c_uint32),
    ("valid", c_uint32),
]

am_keypoint_t._fields_ = [
    ("x", c_float),
    ("y", c_float),
    ("confidence", c_float),
]

am_detect_box_t._fields_ = [
    ("bounds", c_float * 4),
    ("confidence", c_float),
    ("class_id", c_int32),
    ("class_name_utf8", c_char_p),
]

am_classify_entry_t._fields_ = [
    ("class_id", c_int32),
    ("class_name_utf8", c_char_p),
    ("logit", c_float),
    ("probability", c_float),
    ("has_probability", c_uint32),
]

am_roi_map_t._fields_ = [
    ("scale_x", c_float),
    ("scale_y", c_float),
    ("offset_x", c_float),
    ("offset_y", c_float),
    ("valid", c_uint32),
]

am_obb_instance_t._fields_ = [
    ("vertices_xy", c_float * 8),
    ("center_x", c_float),
    ("center_y", c_float),
    ("width", c_float),
    ("height", c_float),
    ("angle", c_float),
    ("confidence", c_float),
    ("class_id", c_int32),
    ("angle_normalized", c_uint32),
]

am_realtime_policy_t._fields_ = [
    ("struct_size", c_uint32),
    ("max_inflight", c_uint32),
    ("replace_oldest", c_uint32),
    ("slot_count", c_uint32),
    ("fair_round_robin", c_uint32),
]

am_stats_t._fields_ = [
    ("struct_size", c_uint32),
    ("scope_utf8", c_char_p),
    ("queue_items", c_uint64),
    ("queue_bytes", c_uint64),
    ("external_held_items", c_uint64),
    ("external_held_bytes", c_uint64),
    ("accepted", c_uint64),
    ("rejected_backpressure", c_uint64),
    ("replaced", c_uint64),
    ("expired", c_uint64),
    ("dropped", c_uint64),
    ("frames_decoded", c_uint64),
    ("frames_dropped", c_uint64),
    ("newest_frame_age_ms", c_int64),
    ("reconnect_count", c_uint64),
    ("resync_count", c_uint64),
    ("infer_executed", c_uint64),
    ("infer_failed", c_uint64),
    ("infer_queue_wait_avg_ms", c_int64),
    ("infer_exec_avg_ms", c_int64),
    ("actual_capture_fps", c_double),
    ("native_rss_estimate_bytes", c_uint64),
    ("threads_active", c_uint32),
    ("threads_peak", c_uint32),
    ("shared_mapped_bytes", c_uint64),
    ("shared_metadata_bytes", c_uint64),
    ("shared_pool_count", c_uint32),
    ("shared_outstanding_borrows", c_uint32),
    ("reserved", c_uint32 * 6),
]

amc_init_params_t._fields_ = [
    ("struct_size", c_uint32),
    ("abi_version", c_uint32),
    ("runtime_dirs_utf8", POINTER(c_char_p)),
    ("runtime_dir_count", c_uint32),
    ("reserved", c_uint32 * 4),
    ("feature_flags", c_uint64),
]

am_credential_data_t._fields_ = [
    ("struct_size", c_uint32),
    ("username_utf8", c_char_p),
    ("password_utf8", c_char_p),
]

am_shared_pool_attach_desc_t._fields_ = [
    ("struct_size", c_uint32),
    ("protocol_version", c_uint32),
    ("pool_id", c_uint64),
    ("owner_pid", c_uint64),
    ("owner_started_at_100ns", c_uint64),
    ("pool_generation", c_uint32),
    ("identity_tag", c_uint8 * 32),
]

amc_capability_t._fields_ = [
    ("struct_size", c_uint32),
    ("abi_minor", c_uint32),
    ("feature_bits", c_uint32),
    ("pixel_formats", c_uint32),
    ("max_sources", c_uint32),
    ("max_decoders", c_uint32),
    ("max_subscribers_per_source", c_uint32),
    ("frame_budget_bytes", c_uint64),
    ("shared_pool_state", c_uint32),
]

am_reconnect_policy_t._fields_ = [
    ("struct_size", c_uint32),
    ("initial_backoff_ms", c_uint32),
    ("max_backoff_ms", c_uint32),
    ("jitter_ms", c_uint32),
    ("max_attempts", c_uint32),
]

am_encoded_packet_t._fields_ = [
    ("struct_size", c_uint32),
    ("codec", c_uint32),
    ("bitstream_format", c_uint32),
    ("flags", c_uint32),
    ("config_generation", c_uint32),
    ("source_generation", c_uint32),
    ("source_id", c_uint64),
    ("seq", c_uint64),
    ("pts", c_int64),
    ("dts", c_int64),
    ("time_base_num", c_uint32),
    ("time_base_den", c_uint32),
    ("data", POINTER(c_uint8)),
    ("size", c_uint64),
]

am_frame_meta_t._fields_ = [
    ("struct_size", c_uint32),
    ("source_id", c_uint64),
    ("frame_id", c_uint64),
    ("seq", c_uint64),
    ("source_generation", c_uint32),
    ("config_generation", c_uint32),
    ("pts", c_int64),
    ("time_base_num", c_uint32),
    ("time_base_den", c_uint32),
    ("received_at_ms", c_int64),
    ("captured_at_ms", c_int64),
    ("clock_mask", c_uint32),
    ("clock_quality", c_uint32),
    ("transform", am_spatial_transform_t),
]

am_decoder_input_t._fields_ = [
    ("struct_size", c_uint32),
    ("flags", c_uint32),
    ("size", c_uint64),
    ("data", POINTER(c_uint8)),
    ("pts", c_int64),
    ("dts", c_int64),
]

am_capture_stats_t._fields_ = [
    ("struct_size", c_uint32),
    ("actual_fps", c_double),
    ("last_received_at_ms", c_int64),
    ("consecutive_failures", c_uint32),
    ("newest_age_ms", c_int64),
    ("skipped_beats", c_uint64),
    ("shared_consumers", c_uint64),
]

am_backend_matrix_t._fields_ = [
    ("struct_size", c_uint32),
    ("row_count", c_uint32),
    ("rows", POINTER(am_backend_row_t)),
]

am_raw_preprocess_t._fields_ = [
    ("struct_size", c_uint32),
    ("resize", c_uint32),
    ("target_w", c_uint32),
    ("target_h", c_uint32),
    ("fill_value", c_float),
    ("color_order", c_uint32),
    ("normalize", c_uint32),
    ("mean", c_float * 3),
    ("std", c_float * 3),
    ("layout", c_uint32),
]

am_backend_actual_t._fields_ = [
    ("actual", c_uint32),
    ("device_utf8", c_char_p),
    ("subgraph_cpu_nodes", c_int32),
    ("fallback_reason_utf8", c_char_p),
]

am_gpu_frame_desc_t._fields_ = [
    ("struct_size", c_uint32),
    ("width", c_uint32),
    ("height", c_uint32),
    ("d3d_format", c_uint32),
    ("subresource", c_uint32),
    ("shared_nt_handle", c_uint64),
    ("flags", c_uint32),
    ("assoc", am_frame_assoc_t),
]

am_infer_params_t._fields_ = [
    ("struct_size", c_uint32),
    ("confidence_threshold", c_float),
    ("nms_iou_threshold", c_float),
    ("max_det", c_uint32),
    ("sync_deadline_ms", c_uint32),
    ("slot_key", c_uint64),
    ("raw_preprocess", POINTER(am_raw_preprocess_t)),
    ("reserved", c_uint32 * 2),
]

am_batch_receipt_t._fields_ = [
    ("struct_size", c_uint32),
    ("accepted_count", c_uint32),
    ("request_ids", POINTER(c_uint64)),
    ("rejects", POINTER(am_status_t)),
]

am_tensor_out_t._fields_ = [
    ("name", c_char_p),
    ("dtype", c_uint32),
    ("shape", POINTER(c_int64)),
    ("shape_len", c_uint32),
    ("data", c_void_p),
    ("data_size", c_uint64),
    ("is_contiguous", c_uint32),
]

am_detect_result_t._fields_ = [
    ("count", c_uint32),
    ("boxes", POINTER(am_detect_box_t)),
]

am_segment_instance_t._fields_ = [
    ("bounds", c_float * 4),
    ("confidence", c_float),
    ("class_id", c_int32),
    ("mask_width", c_uint32),
    ("mask_height", c_uint32),
    ("mask_stride_bytes", c_uint32),
    ("mask_bits", POINTER(c_uint8)),
    ("roi_map", am_roi_map_t),
]

am_obb_result_t._fields_ = [
    ("count", c_uint32),
    ("instances", POINTER(am_obb_instance_t)),
]

am_limits_t._fields_ = [
    ("struct_size", c_uint32),
    ("max_sources", c_uint32),
    ("max_subscribers_per_source", c_uint32),
    ("max_decoders", c_uint32),
    ("max_capture_continuous", c_uint32),
    ("frame_budget_bytes", c_uint64),
    ("external_hold_max_frames", c_uint32),
    ("external_hold_max_bytes", c_uint64),
    ("max_models", c_uint32),
    ("infer_parallelism", c_uint32),
    ("realtime_max_inflight", c_uint32),
    ("result_budget_bytes", c_uint64),
    ("result_max_items", c_uint32),
    ("control_max_items", c_uint32),
    ("preprocess_budget_bytes", c_uint64),
    ("shared_pool_max_slots", c_uint32),
    ("shared_pool_max_frame_bytes", c_uint32),
    ("shared_pool_max_total_bytes", c_uint64),
    ("shared_pool_max_consumers", c_uint32),
    ("shared_pool_max_outstanding_borrows", c_uint32),
    ("shared_pool_max_pools", c_uint32),
    ("shared_pool_metadata_bytes", c_uint64),
    ("reserved", c_uint32 * 8),
]

am_error_info_t._fields_ = [
    ("struct_size", c_uint32),
    ("code", am_status_t),
    ("message_utf8", c_char_p),
    ("source_utf8", c_char_p),
    ("retryable", c_uint32),
    ("fault_class", c_uint32),
    ("reserved", c_uint32 * 4),
]

am_shared_frame_desc_t._fields_ = [
    ("protocol_version", c_uint32),
    ("pixel_format", c_uint32),
    ("width", c_uint32),
    ("height", c_uint32),
    ("plane_count", c_uint32),
    ("stride_bytes", c_uint32 * 4),
    ("offset_bytes", c_uint32 * 4),
    ("plane_valid_bytes", c_uint32 * 4),
    ("color_matrix", c_uint32),
    ("color_range", c_uint32),
    ("publication_id", c_uint64),
    ("frame_id", c_uint64),
    ("source_id", c_uint64),
    ("source_generation", c_uint32),
    ("config_generation", c_uint32),
    ("assoc_valid", c_uint32),
    ("pool_generation", c_uint32),
    ("time_flags", c_uint32),
    ("pts", c_int64),
    ("time_base_num", c_uint32),
    ("time_base_den", c_uint32),
    ("captured_at_ms", c_int64),
    ("received_at_ms", c_int64),
    ("clock_quality", c_uint32),
    ("reserved", c_uint32 * 4),
]

am_source_desc_t._fields_ = [
    ("struct_size", c_uint32),
    ("kind", c_uint32),
    ("url_utf8", c_char_p),
    ("stream_kind_utf8", c_char_p),
    ("creds", POINTER(am_credential_data_t)),
    ("provider", POINTER(am_credential_provider_t)),
    ("reuse", c_uint32),
    ("reserved", c_uint32 * 4),
]

am_frame_view_t._fields_ = [
    ("struct_size", c_uint32),
    ("width", c_uint32),
    ("height", c_uint32),
    ("format", c_uint32),
    ("plane_count", c_uint32),
    ("planes", POINTER(c_uint8) * 4),
    ("strides", c_uint32 * 4),
    ("plane_size_bytes", c_uint64 * 4),
    ("color_matrix", c_uint32),
    ("color_range", c_uint32),
]

am_decoder_info_t._fields_ = [
    ("struct_size", c_uint32),
    ("codec", c_uint32),
    ("bitstream_format", c_uint32),
    ("output_pixel_format", c_uint32),
    ("hw_policy", c_uint32),
    ("hw_accelerated", c_uint32),
    ("backend_name_utf8", c_char_p),
    ("config_generation", c_uint32),
]

amv_capability_t._fields_ = [
    ("struct_size", c_uint32),
    ("abi_minor", c_uint32),
    ("feature_bits", c_uint32),
    ("dtype_bits", c_uint32),
    ("max_batch_items", c_uint32),
    ("max_models", c_uint32),
    ("infer_parallelism", c_uint32),
    ("result_retention_default_ms", c_uint64),
    ("shared_pool_state", c_uint32),
]

am_model_memory_desc_t._fields_ = [
    ("struct_size", c_uint32),
    ("bytes", POINTER(c_uint8)),
    ("size", c_uint64),
    ("load_mode", c_uint32),
    ("profile_id", c_char_p),
    ("backend", POINTER(am_backend_policy_t)),
]

am_gpu_frame_ref_t._fields_ = [
    ("struct_size", c_uint32),
    ("width", c_uint32),
    ("height", c_uint32),
    ("d3d_format", c_uint32),
    ("subresource", c_uint32),
    ("shared_nt_handle", c_uint64),
    ("texture", c_void_p),
    ("flags", c_uint32),
    ("reserved", c_uint32),
    ("assoc", POINTER(am_frame_assoc_t)),
]

am_batching_stats_t._fields_ = [
    ("struct_size", c_uint32),
    ("abi_minor", c_uint32),
    ("formed_batches", c_uint64),
    ("batched_items", c_uint64),
    ("timeout_batches", c_uint64),
    ("rejected_items", c_uint64),
]

am_classify_result_t._fields_ = [
    ("count", c_uint32),
    ("entries", POINTER(am_classify_entry_t)),
]

am_pixel_view_t._fields_ = [
    ("struct_size", c_uint32),
    ("magic", c_uint32),
    ("owner_handle", c_void_p),
    ("retain_owner", CFUNCTYPE(None, c_void_p)),
    ("release_owner", CFUNCTYPE(None, c_void_p)),
    ("view", POINTER(am_frame_view_t)),
    ("assoc", POINTER(am_frame_assoc_t)),
]

am_request_snapshot_t._fields_ = [
    ("struct_size", c_uint32),
    ("state", c_uint32),
    ("terminal", c_uint32),
    ("error", POINTER(am_error_info_t)),
    ("created_at_ms", c_int64),
    ("finished_at_ms", c_int64),
    ("reserved", c_uint32 * 4),
]

am_probe_request_t._fields_ = [
    ("struct_size", c_uint32),
    ("candidate_count", c_uint32),
    ("references_utf8", POINTER(c_char_p)),
    ("creds", POINTER(am_credential_data_t)),
    ("force_refresh", c_uint32),
    ("timeout_ms", c_uint32),
]

am_captured_image_t._fields_ = [
    ("struct_size", c_uint32),
    ("request_id", c_uint64),
    ("output_kind", c_uint32),
    ("encoded", POINTER(c_uint8)),
    ("encoded_size", c_uint64),
    ("view", POINTER(am_frame_view_t)),
    ("meta", POINTER(am_frame_meta_t)),
]

am_model_info_t._fields_ = [
    ("struct_size", c_uint32),
    ("identity", am_model_identity_t),
    ("load_mode", c_uint32),
    ("input_count", c_uint32),
    ("output_count", c_uint32),
    ("inputs", POINTER(am_io_spec_t)),
    ("outputs", POINTER(am_io_spec_t)),
    ("native_batch", c_uint32),
    ("execution", am_backend_actual_t),
]

am_pose_detection_t._fields_ = [
    ("bounds", c_float * 4),
    ("confidence", c_float),
    ("kpt_count", c_uint32),
    ("keypoints", POINTER(am_keypoint_t)),
]

am_segment_result_t._fields_ = [
    ("count", c_uint32),
    ("instances", POINTER(am_segment_instance_t)),
]

am_shared_frame_view_t._fields_ = [
    ("struct_size", c_uint32),
    ("desc", am_shared_frame_desc_t),
    ("plane_data", c_void_p * 4),
    ("borrow_id", c_uint64),
]

am_backend_selection_t._fields_ = [
    ("struct_size", c_uint32),
    ("candidate_count", c_uint32),
    ("candidates", POINTER(am_backend_candidate_t)),
    ("allow_fallback", c_uint32),
    ("budget_ms", c_uint32),
]

am_pose_result_t._fields_ = [
    ("count", c_uint32),
    ("detections", POINTER(am_pose_detection_t)),
]

am_frame_sub_desc_t._fields_ = [
    ("struct_size", c_uint32),
    ("pixel_format", c_uint32),
    ("sampling_max_fps", c_uint32),
    ("preset_count", c_uint32),
    ("presets", POINTER(am_scale_preset_t)),
    ("mode", c_uint32),
    ("queue_max_items", c_uint32),
    ("queue_max_bytes", c_uint64),
    ("frame_kind", c_uint32),
]

amv_result_t._fields_ = [
    ("struct_size", c_uint32),
    ("request_id", c_uint64),
    ("task", am_task_e),
    ("frame", POINTER(am_frame_assoc_t)),
    ("model", am_model_identity_t),
    ("execution", am_backend_actual_t),
    ("transform", POINTER(am_preprocess_meta_t)),
    ("pose", am_pose_result_t),
    ("detect", am_detect_result_t),
    ("classify", am_classify_result_t),
    ("segment", am_segment_result_t),
    ("obb", am_obb_result_t),
    ("raw_output_count", c_uint32),
    ("raw_outputs", POINTER(am_tensor_out_t)),
    ("infer_start_ms", c_int64),
    ("infer_end_ms", c_int64),
]

am_image_input_t._fields_ = [
    ("struct_size", c_uint32),
    ("kind", c_uint32),
    ("encoded", POINTER(c_uint8)),
    ("encoded_size", c_uint64),
    ("view", POINTER(am_pixel_view_t)),
    ("assoc", POINTER(am_frame_assoc_t)),
    ("flags", c_uint32),
    ("gpu", POINTER(am_gpu_frame_ref_t)),
]

am_batch_item_t._fields_ = [
    ("struct_size", c_uint32),
    ("image", POINTER(am_image_input_t)),
    ("tensor_count", c_uint32),
    ("tensors", POINTER(am_tensor_in_t)),
    ("slot_key", c_uint64),
    ("raw_preprocess", POINTER(am_raw_preprocess_t)),
]

AM_ABI_VERSION = 65537
AM_OK = 0
AM_CODEC_H264 = 1
AM_CODEC_H265 = 2
AM_CODEC_MJPEG = 4
AM_BSF_ANNEXB = 0
AM_BSF_LENGTH_PREFIX = 1
AM_SUB_PUSH = 0
AM_SUB_PULL = 1
AM_IMG_JPEG = 0
AM_IMG_PNG = 1
AM_IMG_PIXEL_VIEW = 2
AM_LOAD_PROFILE = 0
AM_LOAD_RAW = 1
AM_CAP_ENCODED_JPEG = 0
AM_CAP_CPU_FRAME = 1
AM_CLK_PTS = 1
AM_CLK_RECEIVED = 2
AM_CLK_CAPTURED = 4
AM_PKT_KEYFRAME = 1
AM_PKT_PARAMSETS = 2
AM_PKT_AU_COMPLETE = 4
AM_ASSOC_SOURCE = 1
AM_ASSOC_FRAME = 2
AM_ASSOC_GENERATION = 4
AM_ASSOC_PTS = 8
AM_ASSOC_CAPTURED = 16
AM_INPUT_BORROW_RETAINED = 1
AM_TENSOR_CONTIGUOUS = 1
AM_BATCH_NATIVE = 0
AM_BATCH_SPLIT = 1
AM_BATCH_AUTO = 2
AM_BATCHING_ENABLE = 1
AM_PF_ANY = 4294967295
AM_REUSE_ALLOWED = 1
AM_PIXEL_VIEW_MAGIC = 1095585880
AM_RESIZE_LETTERBOX = 0
AM_RESIZE_STRETCH = 1
AM_RESIZE_CENTER_CROP = 2
AM_COLOR_RGB = 0
AM_COLOR_BGR = 1
AM_NORM_NONE = 0
AM_NORM_DIV255 = 1
AM_NORM_MEAN_STD = 2
AM_LAYOUT_NCHW = 0
AM_LAYOUT_NHWC = 1
AM_SFP_DROP_OLDEST = 0
AM_SFP_DROP_WAIT = 1
AM_INIT_SHARED_FRAMES = 1
AM_SHARED_PROTOCOL_VERSION = 1
AM_MEDIA_WORKER_LIMIT = 64
AM_ERR_SDK_OK = 0
AM_ERR_SDK_ABI_UNSUPPORTED = 1
AM_ERR_SDK_STRUCT_SIZE_INVALID = 2
AM_ERR_SDK_INVALID_ARGUMENT = 3
AM_ERR_SDK_NULL_ARGUMENT = 4
AM_ERR_SDK_UTF8_INVALID = 5
AM_ERR_SDK_NOT_SUPPORTED = 6
AM_ERR_SDK_QUEUE_FULL = 7
AM_ERR_SDK_RESOURCE_EXHAUSTED = 8
AM_ERR_SDK_DEADLINE_EXCEEDED = 9
AM_ERR_SDK_TRY_AGAIN = 10
AM_ERR_SDK_DRAINED = 11
AM_ERR_SDK_RESULT_EXPIRED = 12
AM_ERR_SDK_HANDLE_INVALID = 20
AM_ERR_SDK_HANDLE_CLOSED = 21
AM_ERR_SDK_REQUEST_UNKNOWN = 22
AM_ERR_SDK_LIMIT_OUT_OF_RANGE = 23
AM_ERR_SDK_OUT_OF_MEMORY = 24
AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION = 25
AM_ERR_SDK_DOMAIN_CONFLICT = 26
AM_ERR_SDK_GENERATION_MISMATCH = 27
AM_ERR_SDK_INTERNAL_ERROR = 28
AM_ERR_CAMERA_URL_INVALID = 500
AM_ERR_CAMERA_CREDENTIAL_REQUIRED = 501
AM_ERR_CAMERA_CREDENTIAL_INVALID = 502
AM_ERR_CAMERA_AUTH_FAILED = 503
AM_ERR_CAMERA_AUTH_FINAL_FAILED = 504
AM_ERR_CAMERA_TIMEOUT = 505
AM_ERR_CAMERA_UNREACHABLE = 506
AM_ERR_CAMERA_PROBE_PENDING = 507
AM_ERR_CAMERA_PROBE_EXPIRED = 508
AM_ERR_CAMERA_CODEC_UNSUPPORTED = 509
AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED = 510
AM_ERR_CAMERA_SOURCE_LIMIT_REACHED = 511
AM_ERR_CAMERA_SUBSCRIBER_LIMIT_REACHED = 512
AM_ERR_CAMERA_CAPTURE_CAPACITY = 513
AM_ERR_CAMERA_CAPTURE_FAILED = 514
AM_ERR_CAMERA_MODE_CONFLICT = 515
AM_ERR_CAMERA_PERMISSION_DENIED = 516
AM_ERR_CAMERA_STREAM_ENDED = 517
AM_ERR_CAMERA_DECODER_STATE_INVALID = 518
AM_ERR_VISION_MODEL_LOAD_FAILED = 1000
AM_ERR_VISION_FORMAT_INCOMPATIBLE = 1001
AM_ERR_VISION_PROFILE_REQUIRED = 1002
AM_ERR_VISION_PROFILE_UNSUPPORTED = 1003
AM_ERR_VISION_PREPROCESS_REQUIRED = 1004
AM_ERR_VISION_EXTERNAL_DATA_UNSUPPORTED = 1005
AM_ERR_VISION_IO_UNSUPPORTED = 1006
AM_ERR_VISION_TENSOR_TYPE_UNSUPPORTED = 1007
AM_ERR_VISION_INPUT_NAME_UNKNOWN = 1008
AM_ERR_VISION_DTYPE_MISMATCH = 1009
AM_ERR_VISION_INPUT_SHAPE_MISMATCH = 1010
AM_ERR_VISION_INPUT_BYTES_MISMATCH = 1011
AM_ERR_VISION_DYNAMIC_RANGE_EXCEEDED = 1012
AM_ERR_VISION_IMAGE_VIEW_INVALID = 1013
AM_ERR_VISION_IMAGE_DECODE_FAILED = 1014
AM_ERR_VISION_IMAGE_TOO_LARGE = 1015
AM_ERR_VISION_IMAGE_FORMAT_UNKNOWN = 1016
AM_ERR_VISION_ORIENTATION_INVALID = 1017
AM_ERR_VISION_CONFIDENCE_RANGE = 1018
AM_ERR_VISION_INFERENCE_FAILED = 1019
AM_ERR_VISION_OUTPUT_SHAPE_MISMATCH = 1020
AM_ERR_VISION_KEYPOINT_META_MISSING = 1021
AM_ERR_VISION_KEYPOINT_NAMES_MISMATCH = 1022
AM_ERR_VISION_MODEL_TOO_LARGE = 1023
AM_ERR_VISION_BATCH_UNSUPPORTED = 1024
AM_ERR_VISION_BACKEND_STRICT_UNAVAILABLE = 1025
AM_ERR_VISION_BACKEND_CONFLICT = 1026
AM_ERR_VISION_RUNTIME_MISSING = 1027
AM_ERR_VISION_RUNTIME_LOAD_FAILED = 1028
AM_ERR_VISION_GPU_RUNTIME_MISSING = 1029
AM_ERR_VISION_RESULT_EXPIRED = 1030
AM_ERR_VISION_REQUEST_STATE_INVALID = 1031
AM_ERR_VISION_INSUFFICIENT_BATCH_REJECTED = 1032
AMC_CAP_ENCODED_H264 = 1
AMC_CAP_ENCODED_H265 = 2
AMC_CAP_DECODE_H264 = 4
AMC_CAP_DECODE_H265 = 8
AMC_CAP_DECODE_MJPEG = 16
AMC_CAP_DECODER_STANDALONE = 32
AMC_CAP_CAPTURE_ISAPI = 64
AMC_CAP_CAPTURE_CONTINUOUS = 128
AMC_CAP_PUSH = 256
AMC_CAP_PULL = 512
AMC_CAP_REUSE = 1024
AMC_CAP_SCALE = 2048
AMC_CAP_BSF_ANNEXB = 4096
AMC_CAP_BSF_LENGTH_PREFIX = 8192
AMC_CAP_SHARED_FRAME_POOL = 16384
AMC_CAP_WORKER_BUDGET = 32768
AMC_CAP_GPU_FRAMES = 65536
AMV_CAP_IMAGE_INPUT = 1
AMV_CAP_TENSOR_INPUT = 2
AMV_CAP_RAW_MODEL = 4
AMV_CAP_MEMORY_MODEL = 8
AMV_CAP_IMAGE_PREPROCESS_RAW = 16
AMV_CAP_BATCH = 32
AMV_CAP_RESULT_RAW_TENSOR = 64
AMV_CAP_PUSH_NOTIFY = 128
AMV_CAP_SHARED_FRAME_POOL = 256
AMV_CAP_ORDERED_SELECTION = 512

FUNCTIONS = {
    "amc_probe_decode_hardware": (am_status_t, [POINTER(amc_context_t), c_uint32, c_uint32]),
    "amc_get_decode_hardware_report": (am_status_t, [c_void_p, c_char_p, c_uint64, POINTER(c_uint64)]),
    "amc_source_set_decode_policy": (am_status_t, [POINTER(amc_source_t), c_uint32]),
    "amv_probe_inference_hardware": (am_status_t, [POINTER(amv_context_t), c_uint32]),
    "amv_get_inference_hardware_report": (am_status_t, [POINTER(amv_context_t), c_char_p, c_uint64, POINTER(c_uint64)]),
    "amc_get_abi_version": (c_uint32, []),
    "amv_get_abi_version": (c_uint32, []),
    "amc_get_version_info": (am_status_t, [POINTER(am_version_info_t)]),
    "amv_get_version_info": (am_status_t, [POINTER(am_version_info_t)]),
    "amc_init": (am_status_t, [POINTER(amc_init_params_t), POINTER(POINTER(amc_context_t))]),
    "amv_init": (am_status_t, [POINTER(amv_init_params_t), POINTER(POINTER(amv_context_t))]),
    "amc_shutdown": (am_status_t, [POINTER(amc_context_t), c_uint32, POINTER(am_lifecycle_state_t)]),
    "amv_shutdown": (am_status_t, [POINTER(amv_context_t), c_uint32, POINTER(am_lifecycle_state_t)]),
    "amc_shutdown_query": (am_status_t, [POINTER(amc_context_t), POINTER(am_lifecycle_state_t)]),
    "amv_shutdown_query": (am_status_t, [POINTER(amv_context_t), POINTER(am_lifecycle_state_t)]),
    "amc_get_init_error": (am_status_t, [POINTER(am_init_error_t)]),
    "amv_get_init_error": (am_status_t, [POINTER(am_init_error_t)]),
    "amc_query_capability": (am_status_t, [POINTER(amc_context_t), POINTER(amc_capability_t)]),
    "amc_credentials_set": (am_status_t, [POINTER(amc_source_t), POINTER(am_credential_data_t)]),
    "amc_credentials_set_provider": (am_status_t, [POINTER(amc_source_t), POINTER(am_credential_provider_t)]),
    "amc_credentials_invalidate": (am_status_t, [POINTER(amc_source_t), c_uint32]),
    "amc_probe_streams": (am_status_t, [POINTER(amc_context_t), POINTER(am_probe_request_t), POINTER(POINTER(am_probe_result_t))]),
    "amc_probe_result_release": (am_status_t, [POINTER(am_probe_result_t)]),
    "amc_source_open": (am_status_t, [POINTER(amc_context_t), POINTER(am_source_desc_t), POINTER(POINTER(amc_source_t))]),
    "amc_source_close": (am_status_t, [POINTER(amc_source_t), c_uint32, POINTER(am_lifecycle_state_t)]),
    "amc_source_get_status": (am_status_t, [POINTER(amc_source_t), POINTER(am_source_status_t)]),
    "amc_source_set_reconnect_policy": (am_status_t, [POINTER(amc_source_t), POINTER(am_reconnect_policy_t)]),
    "amc_subscribe_encoded": (am_status_t, [POINTER(amc_source_t), POINTER(am_encoded_sub_desc_t), POINTER(POINTER(amc_encoded_sub_t))]),
    "amc_encoded_unsubscribe": (am_status_t, [POINTER(amc_encoded_sub_t), c_uint32, POINTER(am_lifecycle_state_t)]),
    "amc_encoded_set_callback": (am_status_t, [POINTER(amc_encoded_sub_t), am_encoded_cb_fn, c_void_p]),
    "amc_encoded_pull": (am_status_t, [POINTER(amc_encoded_sub_t), c_uint32, POINTER(POINTER(am_encoded_packet_t))]),
    "amc_encoded_packet_retain": (am_status_t, [POINTER(am_encoded_packet_t)]),
    "amc_encoded_packet_release": (am_status_t, [POINTER(am_encoded_packet_t)]),
    "amc_subscribe_frames": (am_status_t, [POINTER(amc_source_t), POINTER(am_frame_sub_desc_t), POINTER(POINTER(amc_frame_sub_t))]),
    "amc_frame_unsubscribe": (am_status_t, [POINTER(amc_frame_sub_t), c_uint32, POINTER(am_lifecycle_state_t)]),
    "amc_frame_set_callback": (am_status_t, [POINTER(amc_frame_sub_t), am_frame_cb_fn, c_void_p]),
    "amc_frame_pull": (am_status_t, [POINTER(amc_frame_sub_t), c_uint32, POINTER(POINTER(amc_frame_t))]),
    "amc_frame_retain": (am_status_t, [POINTER(amc_frame_t)]),
    "amc_frame_release": (am_status_t, [POINTER(amc_frame_t)]),
    "amc_frame_get_view": (am_status_t, [POINTER(amc_frame_t), POINTER(am_frame_view_t)]),
    "amc_frame_get_meta": (am_status_t, [POINTER(amc_frame_t), POINTER(am_frame_meta_t)]),
    "amc_frame_get_scaled": (am_status_t, [POINTER(amc_frame_t), c_uint32, c_uint32, c_uint32, POINTER(POINTER(amc_frame_t))]),
    "amc_frame_as_pixel_view": (am_status_t, [POINTER(amc_frame_t), POINTER(am_pixel_view_t)]),
    "amc_gpu_frame_pool_create": (am_status_t, [POINTER(amc_context_t), c_void_p, POINTER(POINTER(amc_gpu_frame_pool_t))]),
    "amc_gpu_frame_borrow": (am_status_t, [POINTER(amc_gpu_frame_t)]),
    "amc_gpu_frame_return": (am_status_t, [POINTER(amc_gpu_frame_t)]),
    "amc_gpu_frame_release": (am_status_t, [POINTER(amc_gpu_frame_t)]),
    "amc_gpu_frame_stats": (am_status_t, [POINTER(amc_gpu_frame_t), POINTER(am_gpu_frame_stats_t)]),
    "amc_gpu_frame_desc": (am_status_t, [POINTER(amc_gpu_frame_t), POINTER(am_gpu_frame_desc_t)]),
    "amc_gpu_frame_texture": (am_status_t, [POINTER(amc_gpu_frame_t), POINTER(c_void_p)]),
    "amc_capture_sync": (am_status_t, [POINTER(amc_source_t), POINTER(am_capture_request_t), POINTER(POINTER(am_captured_image_t))]),
    "amc_capture_async": (am_status_t, [POINTER(amc_source_t), POINTER(am_capture_request_t), POINTER(c_uint64)]),
    "amc_captured_image_fetch": (am_status_t, [POINTER(amc_context_t), c_uint64, POINTER(POINTER(am_captured_image_t))]),
    "amc_captured_image_ack": (am_status_t, [POINTER(amc_context_t), c_uint64]),
    "amc_image_retain": (am_status_t, [POINTER(am_captured_image_t)]),
    "amc_image_release": (am_status_t, [POINTER(am_captured_image_t)]),
    "amc_capture_continuous_start": (am_status_t, [POINTER(amc_source_t), POINTER(am_capture_continuous_desc_t), POINTER(POINTER(amc_capture_sub_t))]),
    "amc_capture_continuous_stop": (am_status_t, [POINTER(amc_capture_sub_t), c_uint32, POINTER(am_lifecycle_state_t)]),
    "amc_capture_sub_set_callback": (am_status_t, [POINTER(amc_capture_sub_t), am_capture_cb_fn, c_void_p]),
    "amc_capture_sub_pull": (am_status_t, [POINTER(amc_capture_sub_t), c_uint32, POINTER(POINTER(am_captured_image_t))]),
    "amc_capture_sub_get_stats": (am_status_t, [POINTER(amc_capture_sub_t), POINTER(am_capture_stats_t)]),
    "amc_decoder_create": (am_status_t, [POINTER(amc_context_t), POINTER(am_decoder_config_t), POINTER(POINTER(amc_decoder_t))]),
    "amc_decoder_submit": (am_status_t, [POINTER(amc_decoder_t), POINTER(am_decoder_input_t)]),
    "amc_decoder_receive": (am_status_t, [POINTER(amc_decoder_t), c_uint32, POINTER(POINTER(amc_frame_t))]),
    "amc_decoder_drain": (am_status_t, [POINTER(amc_decoder_t)]),
    "amc_decoder_reset": (am_status_t, [POINTER(amc_decoder_t), POINTER(am_decoder_config_t)]),
    "amc_decoder_close": (am_status_t, [POINTER(amc_decoder_t), c_uint32, POINTER(am_lifecycle_state_t)]),
    "amc_decoder_get_status": (am_status_t, [POINTER(amc_decoder_t), POINTER(am_decoder_status_t)]),
    "amc_decoder_get_info": (am_status_t, [POINTER(amc_decoder_t), POINTER(am_decoder_info_t)]),
    "amc_source_set_event_callback": (am_status_t, [POINTER(amc_source_t), am_source_event_fn, c_void_p]),
    "amc_request_query": (am_status_t, [POINTER(amc_context_t), c_uint64, POINTER(POINTER(am_request_snapshot_t))]),
    "amc_request_snapshot_release": (am_status_t, [POINTER(am_request_snapshot_t)]),
    "amc_request_cancel": (am_status_t, [POINTER(amc_context_t), c_uint64]),
    "amc_get_limits": (am_status_t, [POINTER(amc_context_t), POINTER(am_limits_t)]),
    "amc_set_limits": (am_status_t, [POINTER(amc_context_t), POINTER(am_limits_t)]),
    "amc_get_stats": (am_status_t, [POINTER(amc_context_t), c_char_p, POINTER(POINTER(am_stats_t))]),
    "amc_stats_release": (am_status_t, [POINTER(am_stats_t)]),
    "amc_get_last_error": (am_status_t, [c_void_p, POINTER(am_error_info_t)]),
    "amc_status_name": (c_char_p, [am_status_t]),
    "amv_query_capability": (am_status_t, [POINTER(amv_context_t), POINTER(amv_capability_t)]),
    "amv_query_backends": (am_status_t, [POINTER(amv_context_t), POINTER(am_backend_matrix_t)]),
    "amv_backend_retry": (am_status_t, [POINTER(amv_init_params_t)]),
    "amv_model_load": (am_status_t, [POINTER(amv_context_t), POINTER(am_model_desc_t), POINTER(POINTER(amv_model_t))]),
    "amv_model_load_memory": (am_status_t, [POINTER(amv_context_t), POINTER(am_model_memory_desc_t), POINTER(POINTER(amv_model_t))]),
    "amv_model_load_selected": (am_status_t, [POINTER(amv_context_t), POINTER(am_model_desc_t), POINTER(am_backend_selection_t), POINTER(POINTER(amv_model_t)), POINTER(c_uint64)]),
    "amv_model_load_memory_selected": (am_status_t, [POINTER(amv_context_t), POINTER(am_model_memory_desc_t), POINTER(am_backend_selection_t), c_uint32, POINTER(POINTER(amv_model_t)), POINTER(c_uint64)]),
    "amv_get_model_selection_report": (am_status_t, [POINTER(amv_model_t), c_char_p, c_uint64, POINTER(c_uint64)]),
    "amv_get_selection_attempt_report": (am_status_t, [POINTER(amv_context_t), c_uint64, c_char_p, c_uint64, POINTER(c_uint64)]),
    "amv_model_close": (am_status_t, [POINTER(amv_model_t), c_uint32, POINTER(am_lifecycle_state_t)]),
    "amv_model_get_info": (am_status_t, [POINTER(amv_model_t), POINTER(am_model_info_t)]),
    "amv_infer_image_sync": (am_status_t, [POINTER(amv_model_t), POINTER(am_image_input_t), POINTER(am_infer_params_t), POINTER(POINTER(amv_result_t)), POINTER(c_uint64)]),
    "amv_infer_image_async": (am_status_t, [POINTER(amv_model_t), POINTER(am_image_input_t), POINTER(am_infer_params_t), POINTER(c_uint64)]),
    "amv_infer_tensors_sync": (am_status_t, [POINTER(amv_model_t), c_uint32, POINTER(am_tensor_in_t), POINTER(am_infer_params_t), POINTER(POINTER(amv_result_t)), POINTER(c_uint64)]),
    "amv_infer_tensors_async": (am_status_t, [POINTER(amv_model_t), c_uint32, POINTER(am_tensor_in_t), POINTER(am_infer_params_t), POINTER(c_uint64)]),
    "amv_infer_batch": (am_status_t, [POINTER(amv_model_t), c_uint32, POINTER(am_batch_item_t), POINTER(am_batch_options_t), POINTER(am_batch_receipt_t)]),
    "amv_receipt_release": (am_status_t, [POINTER(am_batch_receipt_t)]),
    "amv_model_configure_batching": (am_status_t, [POINTER(amv_model_t), POINTER(am_batching_options_t)]),
    "amv_model_get_batching": (am_status_t, [POINTER(amv_model_t), POINTER(am_batching_options_t), POINTER(am_batching_stats_t)]),
    "amv_result_notify_callback_set": (am_status_t, [POINTER(amv_model_t), am_result_notify_fn, c_void_p]),
    "amv_result_fetch": (am_status_t, [POINTER(amv_context_t), c_uint64, POINTER(POINTER(amv_result_t))]),
    "amv_result_ack": (am_status_t, [POINTER(amv_context_t), c_uint64]),
    "amv_result_retain": (am_status_t, [POINTER(amv_result_t)]),
    "amv_result_release": (am_status_t, [POINTER(amv_result_t)]),
    "amv_request_query": (am_status_t, [POINTER(amv_context_t), c_uint64, POINTER(POINTER(am_request_snapshot_t))]),
    "amv_request_snapshot_release": (am_status_t, [POINTER(am_request_snapshot_t)]),
    "amv_request_cancel": (am_status_t, [POINTER(amv_context_t), c_uint64]),
    "amv_set_realtime_policy": (am_status_t, [POINTER(amv_model_t), POINTER(am_realtime_policy_t)]),
    "amv_get_limits": (am_status_t, [POINTER(amv_context_t), POINTER(am_limits_t)]),
    "amv_set_limits": (am_status_t, [POINTER(amv_context_t), POINTER(am_limits_t)]),
    "amv_get_stats": (am_status_t, [POINTER(amv_context_t), c_char_p, POINTER(POINTER(am_stats_t))]),
    "amv_stats_release": (am_status_t, [POINTER(am_stats_t)]),
    "amv_get_last_error": (am_status_t, [c_void_p, POINTER(am_error_info_t)]),
    "amv_status_name": (c_char_p, [am_status_t]),
    "amc_object_query_state": (am_status_t, [c_void_p, POINTER(am_lifecycle_state_t)]),
    "amv_object_query_state": (am_status_t, [c_void_p, POINTER(am_lifecycle_state_t)]),
    "amc_object_request_close": (am_status_t, [c_void_p]),
    "amv_object_request_close": (am_status_t, [c_void_p]),
    "amc_object_destroy": (am_status_t, [c_void_p]),
    "amv_object_destroy": (am_status_t, [c_void_p]),
    "amc_get_queue_policy": (am_status_t, [POINTER(amc_context_t), c_uint32, POINTER(am_queue_policy_t)]),
    "amc_set_queue_policy": (am_status_t, [POINTER(amc_context_t), c_uint32, POINTER(am_queue_policy_t)]),
    "amv_get_queue_policy": (am_status_t, [POINTER(amv_context_t), c_uint32, POINTER(am_queue_policy_t)]),
    "amv_set_queue_policy": (am_status_t, [POINTER(amv_context_t), c_uint32, POINTER(am_queue_policy_t)]),
    "amc_frame_retain_bridge": (None, [c_void_p]),
    "amc_frame_release_bridge": (None, [c_void_p]),
    "amc_shared_pool_create": (am_status_t, [POINTER(amc_context_t), POINTER(am_shared_pool_config_t), POINTER(am_shared_pool_attach_desc_t), POINTER(POINTER(amc_shared_pool_t))]),
    "amc_shared_pool_publish": (am_status_t, [POINTER(amc_shared_pool_t), POINTER(amc_frame_t), POINTER(c_uint64)]),
    "amc_shared_pool_publish_view": (am_status_t, [POINTER(amc_shared_pool_t), POINTER(am_pixel_view_t), POINTER(c_uint64)]),
    "amc_shared_pool_get_attach": (am_status_t, [POINTER(amc_shared_pool_t), POINTER(am_shared_pool_attach_desc_t)]),
    "amc_shared_pool_close": (am_status_t, [POINTER(amc_shared_pool_t), c_uint32, POINTER(am_lifecycle_state_t)]),
    "amc_shared_pool_get_stats": (am_status_t, [POINTER(amc_shared_pool_t), POINTER(am_shared_pool_stats_t)]),
    "amv_shared_pool_attach": (am_status_t, [POINTER(amv_context_t), POINTER(am_shared_pool_attach_desc_t), POINTER(POINTER(amv_shared_pool_t))]),
    "amv_shared_pool_borrow": (am_status_t, [POINTER(amv_shared_pool_t), c_uint64, c_uint32, POINTER(am_shared_frame_view_t)]),
    "amv_shared_frame_as_pixel_view": (am_status_t, [POINTER(amv_shared_pool_t), POINTER(am_shared_frame_view_t), POINTER(am_pixel_view_t)]),
    "amv_shared_frame_retain_bridge": (None, [c_void_p]),
    "amv_shared_frame_release_bridge": (None, [c_void_p]),
    "amv_shared_pool_return": (am_status_t, [POINTER(amv_shared_pool_t), POINTER(am_shared_frame_view_t)]),
    "amv_shared_pool_detach": (am_status_t, [POINTER(amv_shared_pool_t), c_uint32, POINTER(am_lifecycle_state_t)]),
    "amv_shared_pool_get_stats": (am_status_t, [POINTER(amv_shared_pool_t), POINTER(am_shared_pool_stats_t)]),
    "amc_set_worker_budget": (am_status_t, [POINTER(amc_context_t), c_uint32]),
    "amc_get_worker_budget": (am_status_t, [POINTER(amc_context_t), POINTER(c_uint32)]),
}

def bind(library, prefix, require_all=False):
    """绑定实际导出；开发子集返回缺失清单，最终交付用require_all强制齐全。"""
    missing = []
    for name, (result, arguments) in FUNCTIONS.items():
        if not name.startswith(prefix):
            continue
        try:
            function = getattr(library, name)
        except AttributeError:
            missing.append(name)
            continue
        function.restype = result
        function.argtypes = arguments
    if require_all and missing:
        raise RuntimeError("SDK symbols missing: " + ", ".join(missing))
    return missing
