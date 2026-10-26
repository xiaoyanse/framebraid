// 从公开 ctypes ABI 选取生成；运行 scripts/sdk/generate-csharp-types.py 更新。
using System;
using System.Runtime.InteropServices;
namespace AmrtSdk {
    [StructLayout(LayoutKind.Sequential)]
    public struct amc_init_params_t {
        public uint @struct_size;
        public uint @abi_version;
        public IntPtr @runtime_dirs_utf8;
        public uint @runtime_dir_count;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public uint[] @reserved;
        public ulong @feature_flags;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct amv_init_params_t {
        public uint @struct_size;
        public uint @abi_version;
        public IntPtr @runtime_dirs_utf8;
        public uint @runtime_dir_count;
        public uint @infer_threads;
        public ulong @domain_infer_slots;
        public uint @total_thread_budget;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 3)]
        public uint[] @reserved;
        public ulong @feature_flags;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_model_desc_t {
        public uint @struct_size;
        public IntPtr @path_utf8;
        public uint @load_mode;
        public IntPtr @profile_id;
        public IntPtr @backend;
        public uint @intra_threads;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_backend_policy_t {
        public uint @struct_size;
        public uint @requested;
        public uint @strict;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_image_input_t {
        public uint @struct_size;
        public uint @kind;
        public IntPtr @encoded;
        public ulong @encoded_size;
        public IntPtr @view;
        public IntPtr @assoc;
        public uint @flags;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_infer_params_t {
        public uint @struct_size;
        public float @confidence_threshold;
        public float @nms_iou_threshold;
        public uint @max_det;
        public uint @sync_deadline_ms;
        public ulong @slot_key;
        public IntPtr @raw_preprocess;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 2)]
        public uint[] @reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_raw_preprocess_t {
        public uint @struct_size;
        public uint @resize;
        public uint @target_w;
        public uint @target_h;
        public float @fill_value;
        public uint @color_order;
        public uint @normalize;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 3)]
        public float[] @mean;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 3)]
        public float[] @std;
        public uint @layout;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_tensor_out_t {
        public IntPtr @name;
        public uint @dtype;
        public IntPtr @shape;
        public uint @shape_len;
        public IntPtr @data;
        public ulong @data_size;
        public uint @is_contiguous;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct amv_result_t {
        public uint @struct_size;
        public ulong @request_id;
        public int @task;
        public IntPtr @frame;
        public am_model_identity_t @model;
        public am_backend_actual_t @execution;
        public IntPtr @transform;
        public am_pose_result_t @pose;
        public am_detect_result_t @detect;
        public am_classify_result_t @classify;
        public am_segment_result_t @segment;
        public am_obb_result_t @obb;
        public uint @raw_output_count;
        public IntPtr @raw_outputs;
        public long @infer_start_ms;
        public long @infer_end_ms;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_model_identity_t {
        public ulong @model_uid;
        public uint @model_version;
        public IntPtr @profile_id;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_backend_actual_t {
        public uint @actual;
        public IntPtr @device_utf8;
        public int @subgraph_cpu_nodes;
        public IntPtr @fallback_reason_utf8;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_pose_result_t {
        public uint @count;
        public IntPtr @detections;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_detect_result_t {
        public uint @count;
        public IntPtr @boxes;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_classify_result_t {
        public uint @count;
        public IntPtr @entries;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_segment_result_t {
        public uint @count;
        public IntPtr @instances;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_obb_result_t {
        public uint @count;
        public IntPtr @instances;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_request_snapshot_t {
        public uint @struct_size;
        public uint @state;
        public uint @terminal;
        public IntPtr @error;
        public long @created_at_ms;
        public long @finished_at_ms;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public uint[] @reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_error_info_t {
        public uint @struct_size;
        public int @code;
        public IntPtr @message_utf8;
        public IntPtr @source_utf8;
        public uint @retryable;
        public uint @fault_class;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public uint[] @reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_frame_view_t {
        public uint @struct_size;
        public uint @width;
        public uint @height;
        public uint @format;
        public uint @plane_count;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public IntPtr[] @planes;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public uint[] @strides;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public ulong[] @plane_size_bytes;
        public uint @color_matrix;
        public uint @color_range;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_frame_meta_t {
        public uint @struct_size;
        public ulong @source_id;
        public ulong @frame_id;
        public ulong @seq;
        public uint @source_generation;
        public uint @config_generation;
        public long @pts;
        public uint @time_base_num;
        public uint @time_base_den;
        public long @received_at_ms;
        public long @captured_at_ms;
        public uint @clock_mask;
        public uint @clock_quality;
        public am_spatial_transform_t @transform;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_spatial_transform_t {
        public float @scale_x;
        public float @scale_y;
        public int @offset_x;
        public int @offset_y;
        public uint @rotation;
        public uint @valid;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_pixel_view_t {
        public uint @struct_size;
        public uint @magic;
        public IntPtr @owner_handle;
        public IntPtr @retain_owner;
        public IntPtr @release_owner;
        public IntPtr @view;
        public IntPtr @assoc;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_frame_assoc_t {
        public uint @struct_size;
        public ulong @source_id;
        public ulong @frame_id;
        public uint @source_generation;
        public uint @config_generation;
        public long @pts;
        public uint @time_base_num;
        public uint @time_base_den;
        public long @captured_at_ms;
        public long @received_at_ms;
        public uint @clock_mask;
        public uint @clock_quality;
        public uint @valid;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_source_desc_t {
        public uint @struct_size;
        public uint @kind;
        public IntPtr @url_utf8;
        public IntPtr @stream_kind_utf8;
        public IntPtr @creds;
        public IntPtr @provider;
        public uint @reuse;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public uint[] @reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_credential_data_t {
        public uint @struct_size;
        public IntPtr @username_utf8;
        public IntPtr @password_utf8;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_capture_request_t {
        public uint @struct_size;
        public uint @output_kind;
        public uint @deadline_ms;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public uint[] @reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_captured_image_t {
        public uint @struct_size;
        public ulong @request_id;
        public uint @output_kind;
        public IntPtr @encoded;
        public ulong @encoded_size;
        public IntPtr @view;
        public IntPtr @meta;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_frame_sub_desc_t {
        public uint @struct_size;
        public uint @pixel_format;
        public uint @sampling_max_fps;
        public uint @preset_count;
        public IntPtr @presets;
        public uint @mode;
        public uint @queue_max_items;
        public ulong @queue_max_bytes;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_shared_pool_config_t {
        public uint @struct_size;
        public uint @slot_count;
        public uint @max_frame_bytes;
        public ulong @total_bytes;
        public uint @max_consumers;
        public uint @max_outstanding_borrows;
        public uint @drop_policy;
        public uint @publish_wait_ms;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public uint[] @reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_shared_pool_attach_desc_t {
        public uint @struct_size;
        public uint @protocol_version;
        public ulong @pool_id;
        public ulong @owner_pid;
        public ulong @owner_started_at_100ns;
        public uint @pool_generation;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 32)]
        public byte[] @identity_tag;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_shared_frame_view_t {
        public uint @struct_size;
        public am_shared_frame_desc_t @desc;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public IntPtr[] @plane_data;
        public ulong @borrow_id;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_shared_frame_desc_t {
        public uint @protocol_version;
        public uint @pixel_format;
        public uint @width;
        public uint @height;
        public uint @plane_count;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public uint[] @stride_bytes;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public uint[] @offset_bytes;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public uint[] @plane_valid_bytes;
        public uint @color_matrix;
        public uint @color_range;
        public ulong @publication_id;
        public ulong @frame_id;
        public ulong @source_id;
        public uint @source_generation;
        public uint @config_generation;
        public uint @assoc_valid;
        public uint @pool_generation;
        public uint @time_flags;
        public long @pts;
        public uint @time_base_num;
        public uint @time_base_den;
        public long @captured_at_ms;
        public long @received_at_ms;
        public uint @clock_quality;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public uint[] @reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_backend_candidate_t {
        public uint @backend;
        public uint @device_id;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_backend_selection_t {
        public uint @struct_size;
        public uint @candidate_count;
        public IntPtr @candidates;
        public uint @allow_fallback;
        public uint @budget_ms;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct am_model_memory_desc_t {
        public uint @struct_size;
        public IntPtr @bytes;
        public ulong @size;
        public uint @load_mode;
        public IntPtr @profile_id;
        public IntPtr @backend;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct amv_capability_t {
        public uint @struct_size;
        public uint @abi_minor;
        public uint @feature_bits;
        public uint @dtype_bits;
        public uint @max_batch_items;
        public uint @max_models;
        public uint @infer_parallelism;
        public ulong @result_retention_default_ms;
        public uint @shared_pool_state;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct amc_capability_t {
        public uint @struct_size;
        public uint @abi_minor;
        public uint @feature_bits;
        public uint @pixel_formats;
        public uint @max_sources;
        public uint @max_decoders;
        public uint @max_subscribers_per_source;
        public ulong @frame_budget_bytes;
        public uint @shared_pool_state;
    }

    public static class VisionCapabilities {
        public const uint OrderedSelection = 512U;
    }
}
