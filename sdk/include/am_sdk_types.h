/* C ABI公开头文件；由scripts/sdk/generate-abi.mjs生成，勿手工修改。 */
#ifndef AM_SDK_TYPES_H
#define AM_SDK_TYPES_H
#include <stdint.h>
#include <stddef.h>
#include <limits.h>
#ifndef AM_API
#if defined(_WIN32) && defined(AM_SDK_BUILD)
#define AM_API __declspec(dllexport)
#else
#define AM_API
#endif
#endif
#ifdef __cplusplus
extern "C" {
#endif
typedef struct am_version_info_s am_version_info_t;
typedef struct amc_init_params_s amc_init_params_t;
typedef struct amv_init_params_s amv_init_params_t;
typedef struct am_error_info_s am_error_info_t;
typedef struct am_init_error_s am_init_error_t;
typedef struct am_request_snapshot_s am_request_snapshot_t;
typedef struct am_queue_policy_s am_queue_policy_t;
typedef struct am_credential_data_s am_credential_data_t;
typedef struct am_credential_provider_s am_credential_provider_t;
typedef struct am_shared_frame_desc_s am_shared_frame_desc_t;
typedef struct am_shared_pool_config_s am_shared_pool_config_t;
typedef struct am_shared_pool_attach_desc_s am_shared_pool_attach_desc_t;
typedef struct am_shared_frame_view_s am_shared_frame_view_t;
typedef struct am_shared_pool_stats_s am_shared_pool_stats_t;
typedef struct amc_capability_s amc_capability_t;
typedef struct am_source_desc_s am_source_desc_t;
typedef struct am_source_status_s am_source_status_t;
typedef struct am_reconnect_policy_s am_reconnect_policy_t;
typedef struct am_probe_candidate_s am_probe_candidate_t;
typedef struct am_probe_request_s am_probe_request_t;
typedef struct am_probe_result_s am_probe_result_t;
typedef struct am_encoded_packet_s am_encoded_packet_t;
typedef struct am_encoded_sub_desc_s am_encoded_sub_desc_t;
typedef struct am_frame_view_s am_frame_view_t;
typedef struct am_spatial_transform_s am_spatial_transform_t;
typedef struct am_frame_meta_s am_frame_meta_t;
typedef struct am_scale_preset_s am_scale_preset_t;
typedef struct am_frame_sub_desc_s am_frame_sub_desc_t;
typedef struct am_decoder_config_s am_decoder_config_t;
typedef struct am_decoder_input_s am_decoder_input_t;
typedef struct am_decoder_status_s am_decoder_status_t;
typedef struct am_decoder_info_s am_decoder_info_t;
typedef struct am_capture_request_s am_capture_request_t;
typedef struct am_captured_image_s am_captured_image_t;
typedef struct am_capture_continuous_desc_s am_capture_continuous_desc_t;
typedef struct am_capture_stats_s am_capture_stats_t;
typedef struct am_event_detail_s am_event_detail_t;
typedef struct amv_capability_s amv_capability_t;
typedef struct am_backend_row_s am_backend_row_t;
typedef struct am_backend_matrix_s am_backend_matrix_t;
typedef struct am_backend_policy_s am_backend_policy_t;
typedef struct am_raw_preprocess_s am_raw_preprocess_t;
typedef struct am_model_desc_s am_model_desc_t;
typedef struct am_model_memory_desc_s am_model_memory_desc_t;
typedef struct am_io_spec_s am_io_spec_t;
typedef struct am_backend_actual_s am_backend_actual_t;
typedef struct am_model_identity_s am_model_identity_t;
typedef struct am_model_info_s am_model_info_t;
typedef struct am_frame_assoc_s am_frame_assoc_t;
typedef struct am_image_input_s am_image_input_t;
typedef struct am_infer_params_s am_infer_params_t;
typedef struct am_tensor_in_s am_tensor_in_t;
typedef struct am_batch_item_s am_batch_item_t;
typedef struct am_batch_options_s am_batch_options_t;
typedef struct am_batch_receipt_s am_batch_receipt_t;
typedef struct am_preprocess_meta_s am_preprocess_meta_t;
typedef struct am_tensor_out_s am_tensor_out_t;
typedef struct am_keypoint_s am_keypoint_t;
typedef struct am_pose_detection_s am_pose_detection_t;
typedef struct am_pose_result_s am_pose_result_t;
typedef struct am_detect_box_s am_detect_box_t;
typedef struct am_detect_result_s am_detect_result_t;
typedef struct am_classify_entry_s am_classify_entry_t;
typedef struct am_classify_result_s am_classify_result_t;
typedef struct am_roi_map_s am_roi_map_t;
typedef struct am_segment_instance_s am_segment_instance_t;
typedef struct am_segment_result_s am_segment_result_t;
typedef struct am_obb_instance_s am_obb_instance_t;
typedef struct am_obb_result_s am_obb_result_t;
typedef struct amv_result_s amv_result_t;
typedef struct am_realtime_policy_s am_realtime_policy_t;
typedef struct am_limits_s am_limits_t;
typedef struct am_stats_s am_stats_t;
typedef struct am_pixel_view_s am_pixel_view_t;
typedef struct amc_context_s amc_context_t;
typedef struct amv_context_s amv_context_t;
typedef struct amc_source_s amc_source_t;
typedef struct amc_encoded_sub_s amc_encoded_sub_t;
typedef struct amc_frame_sub_s amc_frame_sub_t;
typedef struct amc_frame_s amc_frame_t;
typedef struct amc_capture_sub_s amc_capture_sub_t;
typedef struct amc_decoder_s amc_decoder_t;
typedef struct amv_model_s amv_model_t;
typedef struct amc_shared_pool_s amc_shared_pool_t;
typedef struct amv_shared_pool_s amv_shared_pool_t;
typedef enum am_source_event_e { AM_EVENT_STATE_CHANGED = 0, AM_EVENT_GENERATION_CHANGED,
  AM_EVENT_ENCODED_RESYNC, AM_EVENT_SPATIAL_CHANGED, AM_EVENT_AUTH_WAITING } am_source_event_e;
typedef enum am_lifecycle_state_e { AM_LIFECYCLE_RUNNING = 0, AM_LIFECYCLE_CANCEL_REQUESTED,
  AM_LIFECYCLE_CLOSING, AM_LIFECYCLE_CLOSED } am_lifecycle_state_e;
typedef enum am_request_state_e { AM_REQ_PENDING = 0, AM_REQ_RUNNING, AM_REQ_DONE, AM_REQ_FAILED,
  AM_REQ_CANCELED, AM_REQ_EXPIRED, AM_REQ_REPLACED, AM_REQ_UNKNOWN } am_request_state_e;
#define AM_ABI_VERSION UINT32_C(65536)
#define AM_OK UINT32_C(0)
#define AM_BSF_ANNEXB UINT32_C(0)
#define AM_BSF_LENGTH_PREFIX UINT32_C(1)
#define AM_SUB_PUSH UINT32_C(0)
#define AM_SUB_PULL UINT32_C(1)
#define AM_IMG_JPEG UINT32_C(0)
#define AM_IMG_PNG UINT32_C(1)
#define AM_IMG_PIXEL_VIEW UINT32_C(2)
#define AM_LOAD_PROFILE UINT32_C(0)
#define AM_LOAD_RAW UINT32_C(1)
#define AM_CAP_ENCODED_JPEG UINT32_C(0)
#define AM_CAP_CPU_FRAME UINT32_C(1)
#define AM_CLK_PTS UINT32_C(1)
#define AM_CLK_RECEIVED UINT32_C(2)
#define AM_CLK_CAPTURED UINT32_C(4)
#define AM_PKT_KEYFRAME UINT32_C(1)
#define AM_PKT_PARAMSETS UINT32_C(2)
#define AM_PKT_AU_COMPLETE UINT32_C(4)
#define AM_ASSOC_SOURCE UINT32_C(1)
#define AM_ASSOC_FRAME UINT32_C(2)
#define AM_ASSOC_GENERATION UINT32_C(4)
#define AM_ASSOC_PTS UINT32_C(8)
#define AM_ASSOC_CAPTURED UINT32_C(16)
#define AM_INPUT_BORROW_RETAINED UINT32_C(1)
#define AM_TENSOR_CONTIGUOUS UINT32_C(1)
#define AM_BATCH_NATIVE UINT32_C(0)
#define AM_BATCH_SPLIT UINT32_C(1)
#define AM_BATCH_AUTO UINT32_C(2)
#define AM_REUSE_ALLOWED UINT32_C(1)
#define AM_PIXEL_VIEW_MAGIC UINT32_C(1095585880)
#define AM_RESIZE_LETTERBOX UINT32_C(0)
#define AM_RESIZE_STRETCH UINT32_C(1)
#define AM_RESIZE_CENTER_CROP UINT32_C(2)
#define AM_COLOR_RGB UINT32_C(0)
#define AM_COLOR_BGR UINT32_C(1)
#define AM_NORM_NONE UINT32_C(0)
#define AM_NORM_DIV255 UINT32_C(1)
#define AM_NORM_MEAN_STD UINT32_C(2)
#define AM_LAYOUT_NCHW UINT32_C(0)
#define AM_LAYOUT_NHWC UINT32_C(1)
#define AM_SFP_DROP_OLDEST UINT32_C(0)
#define AM_SFP_DROP_WAIT UINT32_C(1)
#define AM_INIT_SHARED_FRAMES UINT32_C(1)
#define AM_SHARED_PROTOCOL_VERSION UINT32_C(1)
#define AM_ERR_SDK_OK INT32_C(0)
#define AM_ERR_SDK_ABI_UNSUPPORTED INT32_C(1)
#define AM_ERR_SDK_STRUCT_SIZE_INVALID INT32_C(2)
#define AM_ERR_SDK_INVALID_ARGUMENT INT32_C(3)
#define AM_ERR_SDK_NULL_ARGUMENT INT32_C(4)
#define AM_ERR_SDK_UTF8_INVALID INT32_C(5)
#define AM_ERR_SDK_NOT_SUPPORTED INT32_C(6)
#define AM_ERR_SDK_QUEUE_FULL INT32_C(7)
#define AM_ERR_SDK_RESOURCE_EXHAUSTED INT32_C(8)
#define AM_ERR_SDK_DEADLINE_EXCEEDED INT32_C(9)
#define AM_ERR_SDK_TRY_AGAIN INT32_C(10)
#define AM_ERR_SDK_DRAINED INT32_C(11)
#define AM_ERR_SDK_RESULT_EXPIRED INT32_C(12)
#define AM_ERR_SDK_HANDLE_INVALID INT32_C(20)
#define AM_ERR_SDK_HANDLE_CLOSED INT32_C(21)
#define AM_ERR_SDK_REQUEST_UNKNOWN INT32_C(22)
#define AM_ERR_SDK_LIMIT_OUT_OF_RANGE INT32_C(23)
#define AM_ERR_SDK_OUT_OF_MEMORY INT32_C(24)
#define AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION INT32_C(25)
#define AM_ERR_SDK_DOMAIN_CONFLICT INT32_C(26)
#define AM_ERR_SDK_GENERATION_MISMATCH INT32_C(27)
#define AM_ERR_SDK_INTERNAL_ERROR INT32_C(28)
#define AM_ERR_CAMERA_URL_INVALID INT32_C(500)
#define AM_ERR_CAMERA_CREDENTIAL_REQUIRED INT32_C(501)
#define AM_ERR_CAMERA_CREDENTIAL_INVALID INT32_C(502)
#define AM_ERR_CAMERA_AUTH_FAILED INT32_C(503)
#define AM_ERR_CAMERA_AUTH_FINAL_FAILED INT32_C(504)
#define AM_ERR_CAMERA_TIMEOUT INT32_C(505)
#define AM_ERR_CAMERA_UNREACHABLE INT32_C(506)
#define AM_ERR_CAMERA_PROBE_PENDING INT32_C(507)
#define AM_ERR_CAMERA_PROBE_EXPIRED INT32_C(508)
#define AM_ERR_CAMERA_CODEC_UNSUPPORTED INT32_C(509)
#define AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED INT32_C(510)
#define AM_ERR_CAMERA_SOURCE_LIMIT_REACHED INT32_C(511)
#define AM_ERR_CAMERA_SUBSCRIBER_LIMIT_REACHED INT32_C(512)
#define AM_ERR_CAMERA_CAPTURE_CAPACITY INT32_C(513)
#define AM_ERR_CAMERA_CAPTURE_FAILED INT32_C(514)
#define AM_ERR_CAMERA_MODE_CONFLICT INT32_C(515)
#define AM_ERR_CAMERA_PERMISSION_DENIED INT32_C(516)
#define AM_ERR_CAMERA_STREAM_ENDED INT32_C(517)
#define AM_ERR_CAMERA_DECODER_STATE_INVALID INT32_C(518)
#define AM_ERR_VISION_MODEL_LOAD_FAILED INT32_C(1000)
#define AM_ERR_VISION_FORMAT_INCOMPATIBLE INT32_C(1001)
#define AM_ERR_VISION_PROFILE_REQUIRED INT32_C(1002)
#define AM_ERR_VISION_PROFILE_UNSUPPORTED INT32_C(1003)
#define AM_ERR_VISION_PREPROCESS_REQUIRED INT32_C(1004)
#define AM_ERR_VISION_EXTERNAL_DATA_UNSUPPORTED INT32_C(1005)
#define AM_ERR_VISION_IO_UNSUPPORTED INT32_C(1006)
#define AM_ERR_VISION_TENSOR_TYPE_UNSUPPORTED INT32_C(1007)
#define AM_ERR_VISION_INPUT_NAME_UNKNOWN INT32_C(1008)
#define AM_ERR_VISION_DTYPE_MISMATCH INT32_C(1009)
#define AM_ERR_VISION_INPUT_SHAPE_MISMATCH INT32_C(1010)
#define AM_ERR_VISION_INPUT_BYTES_MISMATCH INT32_C(1011)
#define AM_ERR_VISION_DYNAMIC_RANGE_EXCEEDED INT32_C(1012)
#define AM_ERR_VISION_IMAGE_VIEW_INVALID INT32_C(1013)
#define AM_ERR_VISION_IMAGE_DECODE_FAILED INT32_C(1014)
#define AM_ERR_VISION_IMAGE_TOO_LARGE INT32_C(1015)
#define AM_ERR_VISION_IMAGE_FORMAT_UNKNOWN INT32_C(1016)
#define AM_ERR_VISION_ORIENTATION_INVALID INT32_C(1017)
#define AM_ERR_VISION_CONFIDENCE_RANGE INT32_C(1018)
#define AM_ERR_VISION_INFERENCE_FAILED INT32_C(1019)
#define AM_ERR_VISION_OUTPUT_SHAPE_MISMATCH INT32_C(1020)
#define AM_ERR_VISION_KEYPOINT_META_MISSING INT32_C(1021)
#define AM_ERR_VISION_KEYPOINT_NAMES_MISMATCH INT32_C(1022)
#define AM_ERR_VISION_MODEL_TOO_LARGE INT32_C(1023)
#define AM_ERR_VISION_BATCH_UNSUPPORTED INT32_C(1024)
#define AM_ERR_VISION_BACKEND_STRICT_UNAVAILABLE INT32_C(1025)
#define AM_ERR_VISION_BACKEND_CONFLICT INT32_C(1026)
#define AM_ERR_VISION_RUNTIME_MISSING INT32_C(1027)
#define AM_ERR_VISION_RUNTIME_LOAD_FAILED INT32_C(1028)
#define AM_ERR_VISION_GPU_RUNTIME_MISSING INT32_C(1029)
#define AM_ERR_VISION_RESULT_EXPIRED INT32_C(1030)
#define AM_ERR_VISION_REQUEST_STATE_INVALID INT32_C(1031)
#define AM_ERR_VISION_INSUFFICIENT_BATCH_REJECTED INT32_C(1032)
#define AMC_CAP_ENCODED_H264 (UINT64_C(1) << 0)
#define AMC_CAP_ENCODED_H265 (UINT64_C(1) << 1)
#define AMC_CAP_DECODE_H264 (UINT64_C(1) << 2)
#define AMC_CAP_DECODE_H265 (UINT64_C(1) << 3)
#define AMC_CAP_DECODE_MJPEG (UINT64_C(1) << 4)
#define AMC_CAP_DECODER_STANDALONE (UINT64_C(1) << 5)
#define AMC_CAP_CAPTURE_ISAPI (UINT64_C(1) << 6)
#define AMC_CAP_CAPTURE_CONTINUOUS (UINT64_C(1) << 7)
#define AMC_CAP_PUSH (UINT64_C(1) << 8)
#define AMC_CAP_PULL (UINT64_C(1) << 9)
#define AMC_CAP_REUSE (UINT64_C(1) << 10)
#define AMC_CAP_SCALE (UINT64_C(1) << 11)
#define AMC_CAP_BSF_ANNEXB (UINT64_C(1) << 12)
#define AMC_CAP_BSF_LENGTH_PREFIX (UINT64_C(1) << 13)
#define AMC_CAP_SHARED_FRAME_POOL (UINT64_C(1) << 14)
#define AMV_CAP_IMAGE_INPUT (UINT64_C(1) << 0)
#define AMV_CAP_TENSOR_INPUT (UINT64_C(1) << 1)
#define AMV_CAP_RAW_MODEL (UINT64_C(1) << 2)
#define AMV_CAP_MEMORY_MODEL (UINT64_C(1) << 3)
#define AMV_CAP_IMAGE_PREPROCESS_RAW (UINT64_C(1) << 4)
#define AMV_CAP_BATCH (UINT64_C(1) << 5)
#define AMV_CAP_RESULT_RAW_TENSOR (UINT64_C(1) << 6)
#define AMV_CAP_PUSH_NOTIFY (UINT64_C(1) << 7)
#define AMV_CAP_SHARED_FRAME_POOL (UINT64_C(1) << 8)
typedef int32_t am_status_t;  /* 0=AM_OK；非 0 为稳定数值错误码（§6/主文件 §7），字符串名可查 */
typedef uint32_t am_lifecycle_state_t; /* 关闭查询输出；枚举常量见主文件§5.4，固定4字节 */

typedef struct am_version_info_s {   /* 字符串 DLL 拥有，随进程生命周期有效 */
  uint32_t struct_size, abi_major, abi_minor;
  const char* package_version_utf8;   /* 交付包版本 */
  const char* build_target_utf8;      /* 如 windows-x64 */
  const char* runtime_notes_utf8;     /* 运行库摘要（FFmpeg/ORT 口径与位数），不含路径凭据 */
} am_version_info_t;

typedef struct amc_init_params_s {
  uint32_t struct_size;
  uint32_t abi_version;                  /* 请求版本，主版本不符拒绝 */
  const char* const* runtime_dirs_utf8;  /* 显式运行库目录（SDK-G3/B15），NULL=默认搜索顺序 */
  uint32_t runtime_dir_count;
  uint32_t reserved[4];
  uint64_t feature_flags;              /* 尾追加且按8字节对齐，避开旧尾填充；AM_INIT_SHARED_FRAMES=1，0关闭 */
} amc_init_params_t;

typedef struct amv_init_params_s {
  uint32_t struct_size, abi_version;
  const char* const* runtime_dirs_utf8;
  uint32_t runtime_dir_count;
  uint32_t infer_threads;           /* 推理执行线程预算，0=默认（§6） */
  uint64_t domain_infer_slots;      /* 执行器域在途预算（P4 三轮）：消费方式单选合同下（主文件 §1.2）
                                       推理执行器=进程内单例（宿主静态/DLL 二选一=部署约束（P4 四轮：
                                       部署方保证），SDK 尽力检测冲突返回 sdk.domainConflict、
                                       不承诺自动互检）；首个创建执行器的 init 定值，后续 init
                                       须一致（0=继承现值），不一致 sdk.invalidArgument */
  uint32_t total_thread_budget;     /* 总线程预算（P4）：执行器+ORT intra/inter-op+预处理线程
                                       合计上限；0=机器核数默认；stats.threads_active/peak 观测 */
  uint32_t reserved[3];
  uint64_t feature_flags;           /* 尾追加且按8字节对齐；AM_INIT_SHARED_FRAMES=1，旧 size 按0，未知位拒绝 */
} amv_init_params_t;

typedef struct am_error_info_s {    /* 对象级（get_last_error）与请求级（快照内 error 指针所指）共用 */
  uint32_t struct_size;
  am_status_t code;                 /* 错误码（§6） */
  const char* message_utf8;         /* 人类可读消息（脱敏：不含密码/令牌/含凭据地址） */
  const char* source_utf8;          /* 来源模块，如 "camera.rtsp" / "vision.ort" / "camera.decoder" */
  uint32_t retryable;               /* 1=可重试（R），0=需介入（M），按 §6 属性 */
  uint32_t fault_class;             /* am_fault_class_e（B13 分型；非相机来源=NONE） */
  uint32_t reserved[4];
} am_error_info_t;
/* 字符串随所属快照/句柄生命周期只读有效；长期持有由调用方复制（P3 边界：SDK 不提供字符串
   复制/释放函数，避免二次释放歧义——复制即调用方缓冲持有） */

typedef struct am_init_error_s {    /* 进程级 init 失败单槽快照（P3；经 am?_get_init_error 读） */
  uint32_t struct_size;
  am_status_t code;                 /* 最近一次 init 失败码；AM_OK=无失败记录 */
  char message[256];                /* P3 三轮：内嵌固定缓冲（UTF-8，NUL 结尾；超长截断至 255+NUL）
                                       ——读取返回即自包含快照，彻底消除「下个失败覆盖后字符串
                                       失效」窗口，调用方无需与 attempt_seq 对赌字符串有效性 */
  const char* source_utf8;          /* 来源模块（SDK 静态字符串，进程生命周期有效，无覆盖失效） */
  int64_t failed_at_ms;             /* 失败时刻（时间原点见 §2.5 注：进程单调钟） */
  uint64_t attempt_seq;             /* init 尝试序号，递增；读侧据此判断槽是否已被更新覆盖 */
} am_init_error_t;
/* 并发语义（P3）：单槽仅 init 失败路径写入；读为原子快照（不撕裂）；并发 init 失败后写覆盖
   前写——attempt_seq 可辨新旧；无 TTL 静默失效（记录保留至下次失败覆盖或进程结束）。
   三轮起 message 内嵌固定缓冲：结构为自包含值快照，复制/跨线程传递无失效窗口 */

typedef struct am_request_snapshot_s {  /* P3 三轮改快照句柄制+四轮分库释放：DLL 分配并拥有，
                                           经 am?_request_query 移交、回**分配方库**的释放入口
                                           （相机 amc_request_snapshot_release / 推理
                                           amv_request_snapshot_release；原公共
                                           am_request_snapshot_release 拆分删除——结构类型两库
                                           同构，但句柄归属各自 DLL，跨库传入返回
                                           sdk.handleInvalid），不再由调用方按值输出。
                                           保留期内可重复 query（每次移交独立句柄引用，
                                           内容一致）；终态内容（含 error 字符串）不可变，已移交
                                           句柄至 release 前冻结可读——请求记录 TTL 出账不影响
                                           已移交快照，彻底消除出账期间字符串失效窗口；请求出账后
                                           query 返回 sdk.resultExpired（从未存在=sdk.requestUnknown） */
  uint32_t struct_size;
  uint32_t state;              /* am_request_state_e（主文件 §5.4） */
  uint32_t terminal;           /* 1=已进入终态（Done/Failed/Canceled/Expired/Replaced）；
                                 终态一经写入不可覆盖，保留至出账（UNKNOWN） */
  const am_error_info_t* error;  /* 终态错误详情（P2 指针化：可演进结构不按值嵌入，被指对象
                                   自带 struct_size 判版）；指向快照对象内部缓冲，随句柄有效期
                                   只读（release 后不可再用）；Pending/Running/
                                   Done 时指向 code=AM_OK 的空详情 */
  int64_t created_at_ms;       /* 请求接受时刻 */
  int64_t finished_at_ms;      /* 终态达成时刻；0=未终态 */
  uint32_t reserved[4];
} am_request_snapshot_t;
/* 初始化失败详情不经本快照：见 am_init_error_t（进程级单槽，P3；message 内嵌固定缓冲） */

typedef enum am_queue_class_e { AM_Q_REALTIME_FRAMES = 0, AM_Q_ENCODED_STREAM, AM_Q_REALTIME_INFER,
  AM_Q_OFFLINE_BATCH, AM_Q_RESULTS, AM_Q_CONTROL } am_queue_class_e;
typedef struct am_queue_policy_s {
  uint32_t struct_size;
  uint32_t max_items;
  uint64_t max_bytes;
  uint32_t max_waiters, wait_deadline_ms;  /* 仅离线类 */
  uint32_t ttl_ms, reserved[4];            /* ttl 仅结果类（抓图+推理结果统一） */
} am_queue_policy_t;

typedef struct am_credential_data_s {  /* 值由调用方缓冲持有，调用期间有效；DLL 即刻复制 */
  uint32_t struct_size;
  const char* username_utf8;
  const char* password_utf8;
} am_credential_data_t;

typedef am_status_t (*am_credential_fetch_fn)(void* user_data, uint32_t known_version,
                                              am_credential_data_t* out_creds, uint32_t* out_version);
typedef void (*am_credential_drop_fn)(void* user_data);
typedef struct am_credential_provider_s {
  uint32_t struct_size;
  am_credential_fetch_fn fetch;   /* 返回新凭据+新版本号；失败返回错误码——SDK 进入/停留
                                    AUTH_WAITING，不沿旧凭据风暴重试（B13） */
  am_credential_drop_fn on_drop; void* user_data;  /* on_drop：注销屏障后回调一次，此后可释放 user_data */
  uint32_t fetch_timeout_ms;
} am_credential_provider_t;

/* 能力三态四值（两库 capability 结构共用；主文件 §2.1/§3.1 查询语义） */
typedef enum am_shared_pool_state_e {
  AM_SFP_ENABLED = 0,               /* 本 ctx 可用（能力位=1 且已启用） */
  AM_SFP_DISABLED = 1,              /* 未启用：本 ctx/init 未开启该传输（保留普通内存输入输出） */
  AM_SFP_NOT_COMPILED = 2,          /* 未编入：裁剪构建明确报不支持，不冒充同等首版配置 */
  AM_SFP_PLATFORM_UNSUPPORTED = 3   /* 平台不支持：首版仅 Windows x64 同机 CPU 帧共享 */
} am_shared_pool_state_e;

/* 统一帧描述（SDK-DEC-07 首版必须行 1）：固定宽度、不依赖 Rust 内存布局；
   固定叶结构（B09 布局策略①：无 struct_size、字段集冻结、可按值嵌入/复制）——
   演进经 protocol_version（共享段协议版本）判别，不匹配明确拒绝，不按偏移猜旧布局 */
typedef struct am_shared_frame_desc_s {
  uint32_t protocol_version;      /* 共享段协议版本=1；不符明确拒绝，不能按旧偏移猜布局 */
  uint32_t pixel_format;          /* AM_PF_*（与 am_frame_view_t 首版集合一致，§9-O4） */
  uint32_t width, height;
  uint32_t plane_count;           /* ≤4 */
  uint32_t stride_bytes[4];       /* 各平面步长（不假设行紧密排列） */
  uint32_t offset_bytes[4];       /* 各平面相对帧数据起点偏移 */
  uint32_t plane_valid_bytes[4];  /* 各平面有效字节（P5 plane_size_bytes 语义） */
  uint32_t color_matrix, color_range; /* 与普通像素视图一致；NV12 不得丢失色彩语义 */
  uint64_t publication_id;        /* 池内发布序号，非零且不复用；borrow 的选择键，不覆盖原 frame_id */
  uint64_t frame_id;              /* 原输入帧身份；不以publication_id覆盖，完整键含源/代次 */
  uint64_t source_id;             /* 源身份（多源帧身份隔离） */
  uint32_t source_generation;     /* 源代次（断流/分辨率变化递增） */
  uint32_t config_generation;     /* 与普通 am_frame_assoc_t 一致 */
  uint32_t assoc_valid;           /* AM_ASSOC_* 有效字段掩码，不把未知字段当已知 */
  uint32_t pool_generation;       /* 池身份内代次；池重建另生随机身份，不靠跨重启计数连续 */
  uint32_t time_flags;            /* AM_CLK_* 位集：本帧可用的时间语义 */
  int64_t  pts;                   /* 时间戳集合（原点/时钟质量语义同 §2.5；未知=INT64_MIN） */
  uint32_t time_base_num, time_base_den; /* PTS 有效时均非零；未知为 0/0，不擅自默认 90kHz */
  int64_t  captured_at_ms;
  int64_t  received_at_ms;
  uint32_t clock_quality;         /* AM_CQ_*（P5） */
  uint32_t reserved[4];
} am_shared_frame_desc_t;

/* 发布侧池配置（相机；O11基准值见共享帧合同§3，均可收紧，不承诺性能） */
typedef struct am_shared_pool_config_s {
  uint32_t struct_size;
  uint32_t slot_count;                /* 槽数上限 */
  uint32_t max_frame_bytes;           /* 单帧容量上限 */
  uint64_t total_bytes;               /* 总字节上限 */
  uint32_t max_consumers;             /* 消费者上限（多模型≠无限广播） */
  uint32_t max_outstanding_borrows;   /* 全池在借租约上限；每次 borrow 独立计数，内部 retain 不新增租约 */
  uint32_t drop_policy;               /* 满载策略：丢未消费旧帧或限时等待（AM_SFP_DROP_*，§6.1） */
  uint32_t publish_wait_ms;           /* 0=非阻塞；WAIT 时 0..5000ms，DROP_OLDEST 时必须为 0 */
  uint32_t reserved[4];
} am_shared_pool_config_t;

/* attach 凭据：跨进程身份显式关联（共享段+资源拥有者+协议身份）；固定宽度、可复制、
   不含本地地址/裸指针——不把本地地址或 Rust 内存布局当传输协议；调用方自行在进程间传递 */
typedef struct am_shared_pool_attach_desc_s {
  uint32_t struct_size;
  uint32_t protocol_version;          /* 版本不符=abiUnsupported，身份/代次不符=generationMismatch */
  uint64_t pool_id;                   /* 池协议身份（创建时分配） */
  uint64_t owner_pid;                 /* 仅 PID 不足以证明身份或进程退出 */
  uint64_t owner_started_at_100ns;    /* Windows 进程创建 FILETIME；结合已打开进程句柄校验身份 */
  uint32_t pool_generation;           /* 与完整池身份共同校验，重启不要求持久保存递增计数 */
  uint8_t  identity_tag[32];          /* 创建时 CSPRNG 随机身份；命名/序列化规则见共享帧专项合同 */
} am_shared_pool_attach_desc_t;

/* 消费侧只读借用视图：amv_shared_pool_borrow 输出到调用方缓冲（固定大小），
   有效期至对应 amv_shared_pool_return；plane_data 只读映射，调用方 MUST NOT 写入 */
typedef struct am_shared_frame_view_s {
  uint32_t struct_size;
  am_shared_frame_desc_t desc;        /* 固定叶结构按值嵌入（B09 策略①，可复制） */
  const void* plane_data[4];          /* 只读映射地址（消费进程内视图，有效期至 return） */
  uint64_t borrow_id;                 /* 借用凭证（return 校验；多模型消费=各自独立 borrow_id） */
} am_shared_frame_view_t;

/* 共享池统计（两库同构；占用/丢帧/等待统计，SDK-DEC-07 首版必须行 4） */
typedef struct am_shared_pool_stats_s {
  uint32_t struct_size;
  uint64_t published;                 /* 累计发布 */
  uint64_t dropped_stale;             /* 满载丢未消费旧帧计数（drop_policy 丢旧档） */
  uint64_t dropped_full;              /* 无可回收槽时的拒绝计数（wait/拒绝档） */
  uint64_t wait_timeouts;             /* 限时等待超时计数 */
  uint32_t slots_in_use;              /* 占用槽数 */
  uint32_t slots_borrowed;            /* 被借用的不同槽数（多模型借同槽不重复计槽） */
  uint32_t outstanding_borrows;       /* 尚有引用的在借租约数，与不同槽数分开 */
  uint32_t consumers_attached;        /* 在册消费者数 */
  uint64_t bytes_in_use;              /* 池内占用字节 */
  uint64_t metadata_bytes;            /* 单独计费的控制区与描述元数据 */
  uint32_t reserved[4];
} am_shared_pool_stats_t;

typedef struct amc_capability_s {
  uint32_t struct_size, abi_minor;
  uint32_t feature_bits;   /* AMC_CAP_ENCODED_H264|ENCODED_H265|DECODE_H264|DECODE_H265|DECODE_MJPEG|
                              DECODER_STANDALONE|CAPTURE_ISAPI|CAPTURE_CONTINUOUS|PUSH|PULL|REUSE|SCALE|
                              BSF_ANNEXB|BSF_LENGTH_PREFIX|SHARED_FRAME_POOL（SDK-DEC-07 发布侧；
                              封装支持集逐位声明，P7；位=0 即拒） */
  uint32_t pixel_formats;  /* AM_PF_* 位集（§6 常量表；首版集合见主文件 §9-O4） */
  uint32_t max_sources, max_decoders, max_subscribers_per_source;
  uint64_t frame_budget_bytes;   /* 帧+缩放总字节硬限（§6） */
  uint32_t shared_pool_state;    /* am_shared_pool_state_e（SDK-DEC-07：可用/未启用/未编入/平台不支持，
                                   §1.6；尾部追加字段，旧消费者按 struct_size 忽略） */
} amc_capability_t;

typedef enum am_source_kind_e { AM_SRC_RTSP_URL = 0, AM_SRC_ISAPI_URL = 1,
                                AM_SRC_DEVICE_REF = 2 } am_source_kind_e;
typedef struct am_source_desc_s {
  uint32_t struct_size, kind;      /* am_source_kind_e */
  const char* url_utf8;            /* RTSP/ISAPI 具体地址；DEVICE_REF 时=宿主已解析的具体流地址
                                     （B04：SDK 不内建设备引用解析器） */
  const char* stream_kind_utf8;    /* "main"/"sub"/"isapi"：宿主提示的类型标注，SDK 按实际行为执行 */
  const am_credential_data_t* creds;          /* 原子凭据（B04 推荐）：NULL=WAITING_CREDENTIALS 起步 */
  const am_credential_provider_t* provider;   /* 原子凭据提供器（与 creds 二选一） */
  uint32_t reuse; uint32_t reserved[4];       /* AM_REUSE_ALLOWED=1：同键复用底层连接 */
} am_source_desc_t;

typedef enum am_source_state_e { AM_SRC_WAITING_CREDENTIALS = 0, AM_SRC_CONNECTING, AM_SRC_LIVE,
  AM_SRC_RECONNECT_WAIT, AM_SRC_AUTH_WAITING, AM_SRC_CLOSING, AM_SRC_CLOSED } am_source_state_e;
/* AUTH_WAITING=最终认证失败（明确凭据拒绝且重试耗尽；正常 Digest challenge 不算，B13），
   停止重试等 credentials_set/provider 更新后自动重连 */

typedef enum am_fault_class_e { AM_FAULT_NONE = 0, AM_FAULT_NETWORK, AM_FAULT_AUTH,
  AM_FAULT_RESOURCE, AM_FAULT_KEYFRAME_WAIT, AM_FAULT_DECODE } am_fault_class_e;
/* B13 分型：网络/认证/资源不足/关键帧等待/解码失败分开上报；KEYFRAME_WAIT/DECODE+
   last_frame_at_ms 组合可辨"输入活跃但解码停顿"的假活 */

typedef struct am_source_status_s {
  uint32_t struct_size;
  am_source_state_e state;
  uint64_t source_id;              /* B12：ctx 内唯一源身份；帧/包/事件/结果关联 */
  uint32_t generation;             /* 源代次：断流恢复/码流或分辨率切换递增 */
  uint32_t config_generation;      /* 配置代次（SPS/PPS/VPS 变化递增） */
  uint32_t fault_class;            /* am_fault_class_e */
  uint32_t reconnect_attempts; int64_t last_frame_at_ms;  /* 0=尚无帧 */
} am_source_status_t;

typedef struct am_reconnect_policy_s {
  uint32_t struct_size;
  uint32_t initial_backoff_ms, max_backoff_ms;  /* 默认 1000/30000；包络上限 60000（§9-O8 统一） */
  uint32_t jitter_ms, max_attempts;   /* jitter 默认 500；max_attempts 0=不限（受关闭超时约束） */
} am_reconnect_policy_t;

typedef enum am_probe_status_e { AM_PROBE_OK = 0, AM_PROBE_TIMEOUT, AM_PROBE_AUTH_FAILED,
  AM_PROBE_UNREACHABLE, AM_PROBE_UNSUPPORTED, AM_PROBE_PENDING } am_probe_status_e;
typedef struct am_probe_candidate_s {   /* 每候选独立返回，子码流失败不伪造为主码流结果 */
  uint32_t struct_size;
  const char* reference_utf8;   /* 候选引用（调用方已解析的具体地址） */
  am_probe_status_e status;
  const char* codec_utf8;       /* 可得时如 "h264"/"h265"/"mjpeg"；未知为 NULL */
  uint32_t width, height;       /* 0=未知（缺失分辨率不伪造） */
  double fps;                   /* 负值=未知 */
  int64_t observed_at_ms, valid_until_ms;  /* 观察时刻/有效期；过期返回 PENDING 或要求刷新 */
} am_probe_candidate_t;
typedef struct am_probe_request_s {
  uint32_t struct_size, candidate_count;
  const char* const* references_utf8;
  const am_credential_data_t* creds;   /* 可选直接凭据（探测期借用） */
  uint32_t force_refresh, timeout_ms;  /* 1=绕过缓存 / 超时 */
} am_probe_request_t;
typedef struct am_probe_result_s {     /* 随 amc_probe_result_release 释放 */
  uint32_t struct_size, candidate_count;
  const am_probe_candidate_t* candidates;
} am_probe_result_t;

#define AM_CODEC_H264 0x1u
#define AM_CODEC_H265 0x2u
#define AM_CODEC_MJPEG 0x4u
typedef struct am_encoded_packet_s {  /* 内容即协议（Annex B 访问单元）；DLL 拥有，随 release 释放 */
  uint32_t struct_size;
  uint32_t codec;              /* AM_CODEC_*：真实编码不冒充 */
  uint32_t bitstream_format;   /* AM_BSF_ANNEXB（首版候选仅 Annex B；长度前缀按能力位，§9-O4） */
  uint32_t flags;              /* AM_PKT_KEYFRAME | AM_PKT_PARAMSETS（H.265 含 VPS/SPS/PPS）| AM_PKT_AU_COMPLETE */
  uint32_t config_generation;  /* 解码器配置代次（参数集变化递增） */
  uint32_t source_generation;  /* 源代次 */
  uint64_t source_id;          /* B12：源身份（与帧/事件关联） */
  uint64_t seq;                /* 订阅内单调序号（缺口=丢弃可观测） */
  int64_t pts, dts;            /* 未知=INT64_MIN */
  uint32_t time_base_num, time_base_den;  /* 默认 1/90000（RTSP） */
  const uint8_t* data;
  uint64_t size;
} am_encoded_packet_t;
typedef struct am_encoded_sub_desc_s {
  uint32_t struct_size;
  uint32_t codec_mask;       /* 想要的编码；源不支持的位订阅时即拒（camera.codecUnsupported） */
  uint32_t mode;             /* AM_SUB_PUSH / AM_SUB_PULL */
  uint32_t queue_max_items; uint64_t queue_max_bytes;  /* 默认 36 项 + 8 MiB（候选，§6/§9-O1） */
} am_encoded_sub_desc_t;

typedef enum am_pixel_format_e { AM_PF_RGBA8 = 0, AM_PF_RGB8, AM_PF_BGR8, AM_PF_NV12 } am_pixel_format_e;
#define AM_PF_ANY 0xFFFFFFFFu  /* 解码器 output_pixel_format 默认：SDK 按能力自选（P7） */
typedef enum am_color_matrix_e { AM_CM_BT601 = 0, AM_CM_BT709, AM_CM_BT2020_NCL } am_color_matrix_e;
typedef enum am_color_range_e { AM_CR_LIMITED = 0, AM_CR_FULL } am_color_range_e;
typedef enum am_clock_quality_e { AM_CQ_UNKNOWN = 0, AM_CQ_LOCAL_RECEIPT, AM_CQ_DEVICE_SYNCED,
                                  AM_CQ_EXTRAPOLATED } am_clock_quality_e;
typedef struct am_frame_view_s {   /* 布局描述：不假设行紧密排列；plane 有效长度显式 */
  uint32_t struct_size;
  uint32_t width, height;
  uint32_t format, plane_count;   /* am_pixel_format_e / 平面数 */
  const uint8_t* planes[4];
  uint32_t strides[4];            /* 每行字节数；有效行字节数 ≤ stride */
  uint64_t plane_size_bytes[4];   /* 每 plane 有效字节长度（P5）：遍历/拷贝以本值为准，
                                    不按 stride×height 猜测平面长度 */
  uint32_t color_matrix;          /* am_color_matrix_e（P5）：YUV→RGB 系数声明；NV12 必填有效值，
                                    RGB 格式忽略 */
  uint32_t color_range;           /* am_color_range_e（P5） */
} am_frame_view_t;
typedef struct am_spatial_transform_s {  /* 固定布局叶结构（P2：字段冻结、无 struct_size，
                                           演进经 am_frame_meta_t 尾部追加）；已知变换时 valid=1；
                                           视场变化仅事件提示，不承诺 ROI 沿用 */
  float scale_x, scale_y;
  int32_t offset_x, offset_y;
  uint32_t rotation;         /* 0/90/180/270 */
  uint32_t valid;
} am_spatial_transform_t;
typedef struct am_frame_meta_s {   /* B12 独立帧身份：不假设一包一帧 */
  uint32_t struct_size;
  uint64_t source_id;        /* 产生本帧的源/解码器身份 */
  uint64_t frame_id;         /* 进程内唯一，跨 DLL 关联用 */
  uint64_t seq;              /* 源内单调序号（缺口=丢弃） */
  uint32_t source_generation;
  uint32_t config_generation;
  int64_t pts;               /* 媒体时间；未知=INT64_MIN */
  uint32_t time_base_num, time_base_den;   /* PTS 时间基（默认 1/90000；ISAPI 无 PTS 时不含 PTS） */
  int64_t received_at_ms;    /* DLL 接收时刻 */
  int64_t captured_at_ms;    /* 设备采集时间；INT64_MIN=未知（不与其他时钟混同） */
  uint32_t clock_mask;       /* AM_CLK_PTS|RECEIVED|CAPTURED：有效时间来源掩码 */
  uint32_t clock_quality;    /* am_clock_quality_e（P5）：设备时钟→本地单调钟的映射质量 */
  am_spatial_transform_t transform;
} am_frame_meta_t;
/* 时间原点（P5）：全部 *_at_ms 为同机单调时基毫秒（Windows 使用原始 QPC/frequency 换算，
   不扣减进程/ctx 启动点；Unix CLOCK_MONOTONIC 的平台实现后续验证）；跨 DLL/跨进程不改原点；captured_at_ms=设备时钟
   经映射换算结果（映射质量见 clock_quality），未知=INT64_MIN，不把外推冒充同步 */
typedef struct am_scale_preset_s { uint32_t width, height; } am_scale_preset_t;
typedef struct am_frame_sub_desc_s {
  uint32_t struct_size;
  uint32_t pixel_format;
  uint32_t sampling_max_fps;   /* 检测节拍上限；0=全帧输出；显示速率由调用方决定 */
  uint32_t preset_count;       /* 缩放档：按需生成，声明即上限 */
  const am_scale_preset_t* presets;
  uint32_t mode, queue_max_items;  /* AM_SUB_PUSH/AM_SUB_PULL；实时帧队列丢旧留新 */
  uint64_t queue_max_bytes;
} am_frame_sub_desc_t;

typedef enum am_decoder_hw_policy_e { AM_DHW_PREFER_HW = 0, AM_DHW_SOFTWARE_ONLY } am_decoder_hw_policy_e;
typedef struct am_decoder_config_s {
  uint32_t struct_size;
  uint32_t codec;              /* AM_CODEC_H264 / AM_CODEC_H265（MJPEG 不经独立解码器） */
  uint32_t bitstream_format;   /* AM_BSF_ANNEXB / AM_BSF_LENGTH_PREFIX（能力位声明支持集，P7） */
  uint32_t output_pixel_format;/* am_pixel_format_e；AM_PF_ANY=SDK 按能力自选（P7），
                                  解析结果经 amc_decoder_get_info 查询 */
  uint32_t hw_policy;          /* am_decoder_hw_policy_e（P7）：PREFER_HW 硬解失败回退软解须过
                                  预算准入（主文件 §2.4）；SOFTWARE_ONLY 强制软解 */
  const uint8_t* extradata;    /* 可选参数集：H.264 SPS/PPS；H.265 VPS/SPS/PPS；NULL=由包内联携带 */
  uint64_t extradata_size;
  uint32_t time_base_num, time_base_den;  /* 输出帧 PTS 时间基 */
  uint32_t queue_max_items; uint64_t queue_max_bytes;   /* 输入队列预算（§6） */
  uint32_t reserved[4];
} am_decoder_config_t;
typedef struct am_decoder_input_s {   /* 提交单位=完整访问单元（AU）或带边界声明的包；
                                        任意切碎字节流拒绝（camera.codecUnsupported 附说明） */
  uint32_t struct_size;
  uint32_t flags;              /* AM_PKT_KEYFRAME | AM_PKT_PARAMSETS | AM_PKT_AU_COMPLETE */
  uint64_t size;
  const uint8_t* data;         /* 借用到 submit 返回：AM_OK=已接受（SDK 已复制，不可重复提交）；
                                 sdk.queueFull=未接受（未被引用，可原样重试） */
  int64_t pts, dts;            /* 透传到输出帧；未知=INT64_MIN */
} am_decoder_input_t;
typedef enum am_decoder_state_e { AM_DEC_READY = 0, AM_DEC_DRAINING, AM_DEC_DRAINED,
  AM_DEC_RESET_PENDING, AM_DEC_CLOSING, AM_DEC_CLOSED } am_decoder_state_e;
/* RESET_PENDING（P7）：reset 已请求、内部缓冲清空中；期间 submit 返回 camera.decoderStateInvalid；
   完成回 READY（decoder_generation 已递增） */
typedef struct am_decoder_status_s {
  uint32_t struct_size;
  am_decoder_state_e state;
  uint32_t decoder_generation;    /* reset/配置变化递增（输出帧 source_generation 同源） */
  uint32_t config_generation;
  uint32_t queue_items; uint64_t queue_bytes;
} am_decoder_status_t;
typedef struct am_decoder_info_s {   /* amc_decoder_get_info（P7）：配置与实际解码路径快照 */
  uint32_t struct_size;
  uint32_t codec, bitstream_format;
  uint32_t output_pixel_format;    /* 实际输出像素格式（AM_PF_ANY 解析后的具体值） */
  uint32_t hw_policy;              /* 创建声明的策略 */
  uint32_t hw_accelerated;         /* 实际软硬件路径：1=硬解；以实际为准不以配置冒充 */
  const char* backend_name_utf8;   /* 实际解码后端名（如 "d3d11va"/"nvdec"/"cpu"）；NULL=未定 */
  uint32_t config_generation;
} am_decoder_info_t;
/* receive 语义：AM_OK=取出帧；sdk.tryAgain=暂无输出且未排干（正常节奏信号）；
   sdk.drained=排空完成（非错误）。输出帧=amc_frame_t（§2.5 句柄族），source_id=解码器实例 ID */

typedef struct am_capture_request_s {
  uint32_t struct_size;
  uint32_t output_kind;    /* AM_CAP_ENCODED_JPEG / AM_CAP_CPU_FRAME */
  uint32_t deadline_ms;    /* 同步模式截止时间（超时后 SDK 不引用任何本请求输入，B06 同义） */
  uint32_t reserved[4];
} am_capture_request_t;
typedef struct am_captured_image_s {   /* fetch/pull 移交调用方；引用计数 amc_image_retain/
                                         amc_image_release（P3：回调内跨期持有必须当场 retain） */
  uint32_t struct_size;
  uint64_t request_id;         /* 单次异步来源；连续订阅项=订阅内序号 */
  uint32_t output_kind;
  const uint8_t* encoded; uint64_t encoded_size;   /* output_kind=JPEG：原 JPEG 字节 */
  const am_frame_view_t* view;                    /* output_kind=CPU_FRAME：像素视图 */
  const am_frame_meta_t* meta; /* B12 时间三元组+clock_mask+clock_quality；P2 指针化（可演进
                                  结构不按值嵌入）；ISAPI 无 PTS 时如实 */
} am_captured_image_t;
typedef struct am_capture_continuous_desc_s {
  uint32_t struct_size;
  uint32_t target_interval_ms;     /* 采样节拍（目标值，非相机实际帧率保证） */
  uint32_t max_inflight;           /* 同共享采集器并发 HTTP 抓图上限；默认 1=不堆积 */
  uint32_t request_timeout_ms;     /* 单次请求超时 */
  uint32_t output_kind;            /* AM_CAP_ENCODED_JPEG / AM_CAP_CPU_FRAME */
  uint32_t mode;                   /* AM_SUB_PUSH / AM_SUB_PULL（独立订阅语义） */
  uint32_t queue_max_items; uint64_t queue_max_bytes;
  uint32_t reserved[4];
} am_capture_continuous_desc_t;
typedef struct am_capture_stats_s {   /* 连续订阅统计（N02：实际值不冒充目标值） */
  uint32_t struct_size;
  double actual_fps;                /* 实际取图频率（滑动窗口） */
  int64_t last_received_at_ms;      /* 最近一张接收时间 */
  uint32_t consecutive_failures;
  int64_t newest_age_ms;            /* 最新图片帧龄 */
  uint64_t skipped_beats;           /* 错过节拍跳过计数（不补发） */
  uint64_t shared_consumers;        /* 当前共享采集器消费者数 */
} am_capture_stats_t;

typedef enum am_event_scope_e { AM_EVENT_SCOPE_SINGLE_SUB = 0, AM_EVENT_SCOPE_SOURCE,
  AM_EVENT_SCOPE_ALL_CONSUMERS } am_event_scope_e;
typedef struct am_event_detail_s {
  uint32_t struct_size;
  uint32_t generation;          /* 相关代次（源/配置） */
  am_event_scope_e scope;       /* 影响范围：单订阅重同步不误伤其他消费者 */
  void* affected_handle;        /* scope=SINGLE_SUB 时=受影响订阅句柄；否则 NULL */
  uint32_t fault_class;         /* am_fault_class_e（B13 分型） */
  const char* reason_utf8;      /* 脱敏后原因（不含凭据地址） */
} am_event_detail_t;

typedef struct amv_capability_s {
  uint32_t struct_size, abi_minor;
  uint32_t feature_bits;   /* AMV_CAP_IMAGE_INPUT|TENSOR_INPUT|RAW_MODEL|MEMORY_MODEL(P1)|
                              IMAGE_PREPROCESS_RAW|BATCH|RESULT_RAW_TENSOR|PUSH_NOTIFY|
                              SHARED_FRAME_POOL（SDK-DEC-07 消费侧） */
  uint32_t dtype_bits;     /* AM_DTYPE_* 位集（§6；首版 F32/F16/I64/I32/U8，§9-O5） */
  uint32_t max_batch_items, max_models, infer_parallelism;
  uint64_t result_retention_default_ms;
  uint32_t shared_pool_state;  /* am_shared_pool_state_e（SDK-DEC-07：与相机侧同构三态四值，§1.6；
                                  尾部追加字段，旧消费者按 struct_size 忽略） */
} amv_capability_t;

typedef enum am_backend_e { AM_BACKEND_CPU = 0, AM_BACKEND_OPENVINO = 1, AM_BACKEND_CUDA = 2 } am_backend_e;
/* 首版后端枚举=CPU/OpenVINO/CUDA，无 TensorRT（P9：描述与枚举对齐——TRT 列后续候选，经能力位+
   小版本演进；不描述未列枚举的降级链） */
typedef struct am_backend_row_s {
  uint32_t backend;             /* am_backend_e */
  uint32_t hw_detected;         /* 硬件探测（驱动侧信息） */
  uint32_t deps_present;        /* provider 依赖库存在 */
  uint32_t provider_loadable;   /* provider 成功加载 */
  uint32_t verified;            /* 本包是否真机验证过；未验如实标 0 */
  const char* note_utf8;
} am_backend_row_t;
/* 四级状态缺一不可：硬件探测≠依赖存在≠provider 加载≠模型实际执行（B14 另含子图回退层） */
typedef struct am_backend_matrix_s {
  uint32_t struct_size, row_count;
  const am_backend_row_t* rows;  /* 缓冲有效到下次同函数调用 */
} am_backend_matrix_t;
/* 每进程单一兼容 ORT 运行库+多 EP 会话：冲突组合在行内 note 与 vision.backendConflict 报告（B14） */

typedef struct am_backend_policy_s {
  uint32_t struct_size;
  uint32_t requested;   /* am_backend_e */
  uint32_t strict;      /* 1=严格（失败报错不回退）；0=允许回退，结果回填实际后端与原因 */
} am_backend_policy_t;
typedef struct am_raw_preprocess_s {   /* B05：RAW 模型图像入口的显式前处理声明 */
  uint32_t struct_size;
  uint32_t resize;             /* AM_RESIZE_LETTERBOX / AM_RESIZE_STRETCH / AM_RESIZE_CENTER_CROP；
                                  P8 语义：LETTERBOX=等比+pad；STRETCH=非等比直达目标（scale_x≠
                                  scale_y，无 pad）；CENTER_CROP=等比放大覆盖后居中裁切——
                                  实际变换（scale_x/y 与 crop 偏移）经结果 transform 回填（§3.5） */
  uint32_t target_w, target_h;
  float fill_value;            /* letterbox 填充（如 114） */
  uint32_t color_order;        /* AM_COLOR_RGB / AM_COLOR_BGR */
  uint32_t normalize;          /* AM_NORM_NONE / AM_NORM_DIV255 / AM_NORM_MEAN_STD */
  float mean[3], std[3];       /* normalize=MEAN_STD 时有效 */
  uint32_t layout;             /* AM_LAYOUT_NCHW / AM_LAYOUT_NHWC */
} am_raw_preprocess_t;
typedef struct am_model_desc_s {
  uint32_t struct_size;
  const char* path_utf8;   /* 模型文件路径（UTF-8） */
  uint32_t load_mode;      /* AM_LOAD_PROFILE：档案模式（B05 原语义）；
                              AM_LOAD_RAW：generic 模式，不要求档案，仅原始张量出口 */
  const char* profile_id;  /* PROFILE 模式：档案 ID；NULL=尝试元数据唯一识别，识别不了报
                              profileRequired（RAW 模式忽略本字段） */
  const am_backend_policy_t* backend; uint32_t intra_threads;  /* P2 指针化；threads 0=默认 2 */
} am_model_desc_t;
typedef struct am_model_memory_desc_s {
  uint32_t struct_size;
  const uint8_t* bytes;    /* 借用到返回；接受即复制，调用方缓冲随后可复用/释放 */
  uint64_t size;           /* 上限见主文件 §6 */
  uint32_t load_mode;      /* 与路径模式一致语义（B05） */
  const char* profile_id;
  const am_backend_policy_t* backend;   /* P2 指针化：可演进结构不按值嵌入 */
} am_model_memory_desc_t;
typedef struct am_io_spec_s {       /* 命名输入/输出规格 */
  uint32_t struct_size;
  const char* name;
  uint32_t dtype;                   /* am_dtype_e */
  const int64_t* shape;             /* 动态维为 -1 */
  uint32_t shape_len;
  const int64_t* dyn_min;           /* 动态维度公布范围（成对 min/max；固定维无效） */
  const int64_t* dyn_max;
} am_io_spec_t;
typedef struct am_backend_actual_s {   /* 固定布局子结构（P2：字段冻结、无 struct_size，演进经
                                           外层尾部追加新指针字段）；B14 四层之②③④ */
  uint32_t actual;                    /* am_backend_e */
  const char* device_utf8;            /* "CPU"/"CUDA:0"/"OpenVINO:0" 等 */
  int32_t subgraph_cpu_nodes;         /* 被 EP 分到 CPU 的节点/子图数（B14 第③层；P9）：
                                         -1=provider 未报告（不可获取），0=无回退，>0=回退数量；
                                         不把「不可获取」与「零回退」混同 */
  const char* fallback_reason_utf8;   /* 允许回退且发生回退时非 NULL（第④层） */
} am_backend_actual_t;
typedef struct am_model_identity_s { uint64_t model_uid; uint32_t model_version; const char* profile_id; } am_model_identity_t;
typedef struct am_model_info_s {
  uint32_t struct_size;
  am_model_identity_t identity;     /* 结果关联与换代区分 */
  uint32_t load_mode;               /* AM_LOAD_PROFILE / AM_LOAD_RAW（B05） */
  uint32_t input_count, output_count;
  const am_io_spec_t* inputs;
  const am_io_spec_t* outputs;
  uint32_t native_batch;            /* 模型批量能力（P6 三值口径）：0=无批量轴（单帧模型，
                                      NATIVE→batchUnsupported）；1=动态批量轴（1..dyn_max 任意 N）；
                                      N≥2=固定批量轴（每批恰 N；P6 三轮：尾批不足 N 时尾项明确拒绝
                                      vision.insufficientBatchRejected，不以 SPLIT 单帧提交——固定
                                      批量模型不接受 batch=1 输入；填充组批列后续候选） */
  am_backend_actual_t execution;    /* 实际后端+设备+子图回退+原因 */
} am_model_info_t;

typedef struct am_frame_assoc_s {   /* B12：可选输入关联元数据（图像/张量/批量入口均可带） */
  uint32_t struct_size;
  uint64_t source_id, frame_id;
  uint32_t source_generation, config_generation;
  int64_t pts; uint32_t time_base_num, time_base_den;
  int64_t captured_at_ms;
  int64_t received_at_ms;           /* P5：本地接收时刻（clock_mask 含 RECEIVED 时有效）；
                                       未知=INT64_MIN（与 meta 同原点：进程单调钟） */
  uint32_t clock_mask;              /* AM_CLK_* 有效来源掩码 */
  uint32_t clock_quality;           /* am_clock_quality_e（P5）：映射质量，见 §2.5 注 */
  uint32_t valid;                   /* AM_ASSOC_* 位集：哪些字段可信 */
} am_frame_assoc_t;
typedef struct am_image_input_s {
  uint32_t struct_size;
  uint32_t kind;               /* AM_IMG_JPEG / AM_IMG_PNG / AM_IMG_PIXEL_VIEW */
  const uint8_t* encoded;      /* kind=JPEG/PNG：借用至同步返回/异步接受复制 */
  uint64_t encoded_size;
  const am_pixel_view_t* view; /* kind=PIXEL_VIEW：跨 DLL 视图（主文件 §5.6）或调用方自建视图；
                                 视图自带 assoc 时优先，除非本结构显式提供 */
  const am_frame_assoc_t* assoc;   /* B12：可选关联（NULL=无）；结果原样回填 */
  uint32_t flags;              /* AM_INPUT_BORROW_RETAINED：仅异步，显式保留拥有者 */
} am_image_input_t;
typedef struct am_infer_params_s {
  uint32_t struct_size;
  float confidence_threshold;  /* 档案默认外的覆盖；越界 vision.confidenceRange */
  float nms_iou_threshold;     /* NMS IoU 阈值覆盖（P8 四轮）：0=用档案默认（detect/segment/obb
                                  内置 NMS 默认 0.7，ultralytics 默认）；仅带 NMS 档案生效 */
  uint32_t max_det;            /* 最大检出数覆盖（P8 四轮）：0=用档案默认（300）；仅带 NMS 档案生效 */
  uint32_t sync_deadline_ms;   /* 同步截止时间（0=默认 30000）；超时语义见主文件 §3.3（B06） */
  uint64_t slot_key;           /* B11：实时槽身份（如 source_id）；0=全局单槽；适用于图像/张量/
                                  批量全部入口（P6，无入口例外） */
  const am_raw_preprocess_t* raw_preprocess;  /* RAW 模型图像入口必填；PROFILE 模式忽略 */
  uint32_t reserved[2];
} am_infer_params_t;
typedef enum am_dtype_e { AM_DTYPE_F32 = 0, AM_DTYPE_F16, AM_DTYPE_I64, AM_DTYPE_I32, AM_DTYPE_U8 } am_dtype_e;
typedef struct am_tensor_in_s {
  const char* name;       /* 模型输入名，按名匹配 */
  uint32_t dtype;         /* am_dtype_e */
  const int64_t* shape;   /* 维度数组 */
  uint32_t shape_len;
  const void* data;       /* 借用至同步返回/异步接受复制（B06：超时返回后不再引用） */
  uint64_t data_size;     /* 元素数×dtype 宽度必须一致，否则 vision.inputBytesMismatch */
  uint32_t flags;         /* AM_TENSOR_CONTIGUOUS 默认；非连续首版拒绝 */
  const am_frame_assoc_t* assoc;   /* B12：可选关联 */
} am_tensor_in_t;

typedef struct am_batch_item_s {
  uint32_t struct_size;
  const am_image_input_t* image;   /* 图像项（P2 指针化：可演进结构不按值嵌入）；NULL=张量项 */
  uint32_t tensor_count; const am_tensor_in_t* tensors;  /* 张量项（二选一） */
  uint64_t slot_key;               /* P6：逐项槽身份（0=全局槽）——公平/替换覆盖批量入口 */
  const am_raw_preprocess_t* raw_preprocess;  /* P6：逐项前处理（RAW 模型图像项必填；
                                                  PROFILE 忽略；单帧入口用 am_infer_params_t） */
} am_batch_item_t;
typedef struct am_batch_options_s {
  uint32_t struct_size;
  uint32_t batch_mode, wait_deadline_ms;  /* AM_BATCH_NATIVE / AM_BATCH_SPLIT / AM_BATCH_AUTO
                                             （三值语义，P6，主文件 §3.5）；wait 0=不等待满即拒 */
} am_batch_options_t;
typedef struct am_batch_receipt_s {   /* 逐项回执（B11）：接受/拒绝逐项区分，取消逐项独立 */
  uint32_t struct_size, accepted_count;
  uint64_t* request_ids;      /* DLL 分配；被拒项 request_ids[i]=0 */
  const am_status_t* rejects; /* 被拒原因（背压/校验失败逐项区分） */
} am_batch_receipt_t;

typedef enum am_task_e { AM_TASK_NONE = 0, AM_TASK_POSE, AM_TASK_DETECT, AM_TASK_CLASSIFY,
                         AM_TASK_SEGMENT, AM_TASK_OBB, AM_TASK_GENERIC } am_task_e;
typedef struct am_preprocess_meta_s {  /* 实际前处理变换回填（P8 重写）：统一映射
                                          orig = (dst - pad) / scale + crop，供坐标复核 */
  uint32_t struct_size;
  uint32_t src_w, src_h, dst_w, dst_h;
  float scale_x, scale_y;      /* 非等比 STRETCH 时两者不等；LETTERBOX/CENTER_CROP 相等 */
  int32_t crop_x, crop_y;      /* 源图裁切区左上偏移（CENTER_CROP 非零；其余 0） */
  uint32_t pad_left, pad_top;
  uint32_t valid;
} am_preprocess_meta_t;
typedef struct am_tensor_out_s {  /* 原始张量视图：随 result 生命周期；跨 result 持有须
                                    amv_result_retain 保留拥有者（B08，无孤立 tensor retain） */
  const char* name;
  uint32_t dtype;
  const int64_t* shape;
  uint32_t shape_len;
  const void* data;
  uint64_t data_size;
  uint32_t is_contiguous;
} am_tensor_out_t;

/* 坐标约定：框=归一化 xyxy 原图坐标系；关键点=归一化 xy 原图坐标系（非模型 letterbox 坐标） */
typedef struct am_keypoint_s { float x, y, confidence; } am_keypoint_t;
typedef struct am_pose_detection_s {
  float bounds[4];
  float confidence;
  uint32_t kpt_count;
  const am_keypoint_t* keypoints;
} am_pose_detection_t;
typedef struct am_pose_result_s { uint32_t count; const am_pose_detection_t* detections; } am_pose_result_t;

typedef struct am_detect_box_s {
  float bounds[4]; float confidence; int32_t class_id; const char* class_name_utf8;
} am_detect_box_t;
typedef struct am_detect_result_s { uint32_t count; const am_detect_box_t* boxes; } am_detect_result_t;

typedef struct am_classify_entry_s {  /* 档案声明 logits 或已 softmax，两者不混同 */
  int32_t class_id; const char* class_name_utf8; float logit, probability; uint32_t has_probability;
} am_classify_entry_t;
typedef struct am_classify_result_s { uint32_t count; const am_classify_entry_t* entries; } am_classify_result_t;

/* ---- Segment（B02）：掩膜完整可读结构 ---- */
typedef struct am_roi_map_s {   /* 固定布局叶结构（P2：字段冻结、无 struct_size，演进经外层尾部
                                   追加）；掩膜像素→原图坐标：orig = mask_xy * scale + offset */
  float scale_x, scale_y;
  float offset_x, offset_y;
  uint32_t valid;               /* 0=映射未知（不承诺直接叠加原图） */
} am_roi_map_t;
typedef struct am_segment_instance_s {   /* 每实例位掩膜：1bit/px，行对齐到字节；位序定死
                                           LSB-first（P8）：字节内 bit0=行内最左像素（x=0→bit0），
                                           行尾不足一字节高位补 0（无效位），跨行按 stride 对齐 */
  float bounds[4]; float confidence; int32_t class_id;
  uint32_t mask_width, mask_height;   /* 掩膜自身尺寸 */
  uint32_t mask_stride_bytes;         /* 行步长 >= (mask_width+7)/8 */
  const uint8_t* mask_bits;           /* 1=前景点；随 result 生命周期；长期持有 amv_result_retain */
  am_roi_map_t roi_map;               /* 去 letterbox 映射已由 SDK 完成（B02） */
} am_segment_instance_t;
typedef struct am_segment_result_s { uint32_t count; const am_segment_instance_t* instances; } am_segment_result_t;

/* ---- OBB（B02）：旋转框完整可读结构 ---- */
typedef struct am_obb_instance_s {
  float vertices_xy[8];    /* 四顶点原图归一化坐标（像素值按原图宽高归一化）；全局约定定死
                              （P8 三轮公式化）：u=(cosθ,sinθ)、v=(−sinθ,cosθ)、w=沿角度方向
                              边长、h=垂直边长；V0=center+u·(w/2)+v·(h/2)（角度方向边与遍历序
                              下一条边的交点——仅沿宽轴走 w/2 是边中点非顶点），V1/V2/V3 依次
                              翻转 u/v 分量符号；遍历序 V0→V1→V2→V3（y 向下图像系视觉顺时针），
                              成对存 xyxy；数值算例与验算见主文件 §3.6（θ=30°/w=200/h=100/
                              center=(320,240) 非正方形样例） */
  float center_x, center_y;
  float width, height;     /* 沿角度方向与垂直方向的局部宽高（归一化）；w=沿角度方向边长 */
  float angle;             /* 弧度制，范围 [-π,π]；自原图 x 正轴起、图像坐标系（y 向下）视觉
                              顺时针为正（P8 四轮修正：公式 u=(cosθ,sinθ) 本身即数学正角
                              （数学惯例逆时针），坐标值直接在图像坐标系使用——sinθ>0 即向图像
                              y 正方向/向下，正角在 y 向下图像系呈视觉顺时针；全局约定定死） */
  float confidence; int32_t class_id;
  uint32_t angle_normalized;   /* 恒为 1：SDK 已按全局约定规范化输出 */
} am_obb_instance_t;
typedef struct am_obb_result_s { uint32_t count; const am_obb_instance_t* instances; } am_obb_result_t;
/* 旋转 NMS：obb-*-nms 档案内置（e2e 档案无）；结果=NMS 后存留实例；几何独立参考实现对照锁定；
   档案只声明「模型原始输出→全局几何约定」的映射，不各自定义位序/顶点序/起始轴（P8） */

typedef struct amv_result_s {   /* 结构化五类+原始张量双出口；JSON 只用于配置诊断 */
  uint32_t struct_size;
  uint64_t request_id;
  am_task_e task;                  /* 任务判别：仅匹配出口有效，其余 count=0/指针 NULL；
                                     RAW 模型=AM_TASK_GENERIC（仅 raw_outputs） */
  const am_frame_assoc_t* frame;   /* 输入关联（B12）：assoc/像素视图携带；未提供=NULL
                                       （P2 指针化：可演进结构不按值嵌入） */
  am_model_identity_t model;       /* 模型身份/版本（固定布局叶结构）：同帧多模型、换代不串版 */
  am_backend_actual_t execution;   /* 实际后端+设备+子图回退+原因（B14；固定布局子结构，P2：
                                       字段冻结，演进经本结构尾部追加新指针字段） */
  const am_preprocess_meta_t* transform;  /* 图像入口实际变换（P2 指针化；P8 含非等比 scale 与
                                       crop 偏移）；张量入口 NULL */
  am_pose_result_t pose;
  am_detect_result_t detect;
  am_classify_result_t classify;
  am_segment_result_t segment;     /* B02 */
  am_obb_result_t obb;             /* B02 */
  uint32_t raw_output_count;       /* 命名原始张量出口（复核用） */
  const am_tensor_out_t* raw_outputs;
  int64_t infer_start_ms, infer_end_ms;   /* 耗时分段（可观测） */
} amv_result_t;

typedef struct am_realtime_policy_s {
  uint32_t struct_size;
  uint32_t max_inflight;      /* 每槽在途上限 */
  uint32_t replace_oldest;    /* 1=同槽超限替换最旧（旧请求 Replaced 终态可查询） */
  uint32_t slot_count;        /* 槽数上限（默认 1）；槽身份=模型×am_infer_params_t.slot_key */
  uint32_t fair_round_robin;  /* 1（默认）=跨槽公平轮转：单源高频不得独占模型执行 */
} am_realtime_policy_t;

typedef struct am_limits_s {   /* 两库共用框架，非本库字段忽略（=0）；只能调低不能调高 */
  uint32_t struct_size;
  uint32_t max_sources;               /* 相机：每资源域源数 */
  uint32_t max_subscribers_per_source;
  uint32_t max_decoders;              /* 相机：独立解码器数 */
  uint32_t max_capture_continuous;    /* 相机：连续抓图订阅数（每源） */
  uint64_t frame_budget_bytes;        /* 相机：解码+缩放帧总字节（含外部持有） */
  uint32_t external_hold_max_frames;  /* 相机：retain 未释放帧数 */
  uint64_t external_hold_max_bytes;   /* 两库：外部持有字节（ack/TTL 不豁免，B08/B10） */
  uint32_t max_models;                /* 推理：模型句柄数 */
  uint32_t infer_parallelism;         /* 推理：并行执行线程 */
  uint32_t realtime_max_inflight;     /* 推理：每模型×槽实时在途 */
  uint64_t result_budget_bytes;       /* 两库：结果保留（含掩膜）字节 */
  uint32_t result_max_items;          /* 两库：结果保留项数 */
  uint32_t control_max_items;         /* 两库：控制/错误通道容量 */
  uint64_t preprocess_budget_bytes;   /* 推理：预处理缓冲字节（B10 入账） */
  uint32_t shared_pool_max_slots;          /* SDK-DEC-07：每池槽数上限（发布侧逐池 ≤ 此值） */
  uint32_t shared_pool_max_frame_bytes;    /* SDK-DEC-07：单帧容量上限（以 am_limits_t 收紧，配置不得超包络） */
  uint64_t shared_pool_max_total_bytes;    /* SDK-DEC-07：ctx共享像素映射总预算=64MiB，非逐池豁免 */
  uint32_t shared_pool_max_consumers;      /* SDK-DEC-07：每池消费者上限（多模型≠无限广播） */
  uint32_t shared_pool_max_outstanding_borrows; /* SDK-DEC-07：未归还借用上限 */
  uint32_t shared_pool_max_pools;          /* ctx内不同池身份上限=4；仍受共享像素总预算 */
  uint64_t shared_pool_metadata_bytes;     /* ctx控制元数据上限=256KiB，每池不超过64KiB */
  uint32_t reserved[8];
} am_limits_t;

typedef struct am_stats_s {    /* 输出经 DLL 脱敏；scope 语义见主文件 §2.11/§3.8 */
  uint32_t struct_size;
  const char* scope_utf8;
  uint64_t queue_items, queue_bytes;         /* 数据队列聚合 */
  uint64_t external_held_items, external_held_bytes;  /* 外部持有（B10） */
  uint64_t accepted, rejected_backpressure, replaced, expired, dropped;
  uint64_t frames_decoded, frames_dropped;
  int64_t  newest_frame_age_ms;
  uint64_t reconnect_count, resync_count;
  uint64_t infer_executed, infer_failed;
  int64_t  infer_queue_wait_avg_ms, infer_exec_avg_ms;
  double   actual_capture_fps;               /* 连续抓图实际频率（N02） */
  uint64_t native_rss_estimate_bytes;        /* 原生库（FFmpeg/ORT/驱动）内存观测（参考值，
                                               与 SDK 可管理硬限分列，B10；不承诺原生库 RSS 受控） */
  uint32_t threads_active, threads_peak;     /* P4：执行器+原生库线程活跃/峰值（观测口径，
                                               与 total_thread_budget 对照；推理域有效） */
  uint64_t shared_mapped_bytes, shared_metadata_bytes; /* ctx累计共享映射与元数据，含未安全回收旧池 */
  uint32_t shared_pool_count, shared_outstanding_borrows; /* 不同池身份数与在借租约数 */
  uint32_t reserved[6];
} am_stats_t;

typedef struct am_pixel_view_s {
  uint32_t struct_size;
  uint32_t magic;
  void* owner_handle;
  void (*retain_owner)(void* owner);
  void (*release_owner)(void* owner);
  const am_frame_view_t* view;
  const am_frame_assoc_t* assoc;
} am_pixel_view_t;

typedef void (*am_encoded_cb_fn)(amc_encoded_sub_t* sub, am_encoded_packet_t* pkt, void* user_data);
typedef void (*am_frame_cb_fn)(amc_frame_sub_t* sub, amc_frame_t* frame, void* user_data);
typedef void (*am_capture_cb_fn)(amc_capture_sub_t* sub, am_captured_image_t* img, void* user_data);
typedef void (*am_source_event_fn)(amc_source_t* src, am_source_event_e kind,
                                   const am_event_detail_t* detail, void* user_data);
typedef void (*am_result_notify_fn)(amv_model_t* model, uint64_t request_id, uint32_t terminal_state, void* user_data);
#ifdef __cplusplus
}
#endif
#endif
