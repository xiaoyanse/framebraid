/* C ABI公开头文件；由scripts/sdk/generate-abi.mjs生成，勿手工修改。 */
#ifndef AM_VISION_H
#define AM_VISION_H
#include "am_sdk_types.h"
#ifdef __cplusplus
extern "C" {
#endif
AM_API am_status_t amv_probe_inference_hardware(amv_context_t* ctx, uint32_t budget_ms);
AM_API am_status_t amv_get_inference_hardware_report(amv_context_t* ctx, char* buffer_utf8,
                                                   uint64_t capacity, uint64_t* required);
AM_API uint32_t amv_get_abi_version(void);
AM_API am_status_t amv_get_version_info(am_version_info_t* out_info);
AM_API am_status_t amv_init(const amv_init_params_t* params, amv_context_t** out_ctx);
AM_API am_status_t amv_shutdown(amv_context_t* ctx, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amv_shutdown_query(amv_context_t* ctx, am_lifecycle_state_t* out_state);
AM_API am_status_t amv_get_init_error(am_init_error_t* out);
AM_API am_status_t amv_query_capability(amv_context_t* ctx, amv_capability_t* out);
AM_API am_status_t amv_query_backends(amv_context_t* ctx, am_backend_matrix_t* out);
AM_API am_status_t amv_backend_retry(const amv_init_params_t* params);
AM_API am_status_t amv_model_load(amv_context_t* ctx, const am_model_desc_t* desc, amv_model_t** out_model);
AM_API am_status_t amv_model_load_memory(amv_context_t* ctx, const am_model_memory_desc_t* desc, amv_model_t** out_model);
AM_API am_status_t amv_model_load_selected(amv_context_t* ctx, const am_model_desc_t* desc,
    const am_backend_selection_t* selection, amv_model_t** out_model, uint64_t* out_attempt_id);
AM_API am_status_t amv_model_load_memory_selected(amv_context_t* ctx, const am_model_memory_desc_t* desc,
    const am_backend_selection_t* selection, uint32_t intra_threads,
    amv_model_t** out_model, uint64_t* out_attempt_id);
AM_API am_status_t amv_model_swap(amv_context_t* ctx,
    const am_model_swap_desc_t* swap_desc, const am_model_desc_t* new_desc,
    amv_model_t** out_model, uint64_t* out_old_model_id);
AM_API am_status_t amv_get_model_selection_report(const amv_model_t* model, char* buffer, uint64_t capacity, uint64_t* required);
AM_API am_status_t amv_get_selection_attempt_report(amv_context_t* ctx, uint64_t attempt_id, char* buffer, uint64_t capacity, uint64_t* required);
AM_API am_status_t amv_model_close(amv_model_t* model, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amv_model_get_info(const amv_model_t* model, am_model_info_t* out_info);
AM_API am_status_t amv_infer_image_sync(amv_model_t* model, const am_image_input_t* input,
                                        const am_infer_params_t* params, amv_result_t** out_result,
                                        uint64_t* out_request_id);
AM_API am_status_t amv_infer_image_async(amv_model_t* model, const am_image_input_t* input,
                                         const am_infer_params_t* params, uint64_t* out_request_id);
AM_API am_status_t amv_infer_tensors_sync(amv_model_t* model, uint32_t input_count,
                                          const am_tensor_in_t* inputs, const am_infer_params_t* params,
                                          amv_result_t** out_result, uint64_t* out_request_id);
AM_API am_status_t amv_infer_tensors_async(amv_model_t* model, uint32_t input_count,
                                           const am_tensor_in_t* inputs, const am_infer_params_t* params,
                                           uint64_t* out_request_id);
AM_API am_status_t amv_infer_batch(amv_model_t* model, uint32_t item_count, const am_batch_item_t* items,
                                   const am_batch_options_t* options, am_batch_receipt_t* out_receipt);
AM_API am_status_t amv_receipt_release(am_batch_receipt_t* receipt);
AM_API am_status_t amv_model_configure_batching(amv_model_t* model, const am_batching_options_t* options);
AM_API am_status_t amv_model_get_batching(amv_model_t* model, am_batching_options_t* out, am_batching_stats_t* stats);
AM_API am_status_t amv_result_notify_callback_set(amv_model_t* model, am_result_notify_fn cb, void* user_data);
AM_API am_status_t amv_result_fetch(amv_context_t* ctx, uint64_t request_id, amv_result_t** out_result);
AM_API am_status_t amv_result_ack(amv_context_t* ctx, uint64_t request_id);
AM_API am_status_t amv_result_retain(amv_result_t* result);
AM_API am_status_t amv_result_release(amv_result_t* result);
AM_API am_status_t amv_request_query(amv_context_t* ctx, uint64_t request_id, am_request_snapshot_t** out_snapshot);
AM_API am_status_t amv_request_snapshot_release(am_request_snapshot_t* snapshot);
AM_API am_status_t amv_request_cancel(amv_context_t* ctx, uint64_t request_id);
AM_API am_status_t amv_set_realtime_policy(amv_model_t* model, const am_realtime_policy_t* policy);
AM_API am_status_t amv_get_limits(amv_context_t* ctx, am_limits_t* out_limits);
AM_API am_status_t amv_set_limits(amv_context_t* ctx, const am_limits_t* limits);
AM_API am_status_t amv_get_stats(amv_context_t* ctx, const char* scope_utf8, am_stats_t** out_stats);
AM_API am_status_t amv_stats_release(am_stats_t* stats);
AM_API am_status_t amv_get_last_error(const void* any_handle, am_error_info_t* out_info);
AM_API const char* amv_status_name(am_status_t status);
AM_API am_status_t amv_object_query_state(const void* any_object_handle, am_lifecycle_state_t* out_state);
AM_API am_status_t amv_object_request_close(const void* any_object_handle);
AM_API am_status_t amv_object_destroy(void* any_object_handle);
AM_API am_status_t amv_get_queue_policy(amv_context_t* ctx, uint32_t queue_class, am_queue_policy_t* out_policy);
AM_API am_status_t amv_set_queue_policy(amv_context_t* ctx, uint32_t queue_class, const am_queue_policy_t* policy);
AM_API am_status_t amv_shared_pool_attach(amv_context_t* ctx, const am_shared_pool_attach_desc_t* attach,
                                          amv_shared_pool_t** out_pool);
AM_API am_status_t amv_shared_pool_borrow(amv_shared_pool_t* pool, uint64_t publication_id,
                                          uint32_t wait_ms, am_shared_frame_view_t* out_view);
AM_API am_status_t amv_shared_frame_as_pixel_view(amv_shared_pool_t* pool,
                                                   const am_shared_frame_view_t* view,
                                                   am_pixel_view_t* out_view);
AM_API void amv_shared_frame_retain_bridge(void* owner);
AM_API void amv_shared_frame_release_bridge(void* owner);
AM_API am_status_t amv_shared_pool_return(amv_shared_pool_t* pool, const am_shared_frame_view_t* view);
AM_API am_status_t amv_shared_pool_detach(amv_shared_pool_t* pool, uint32_t timeout_ms,
                                          am_lifecycle_state_t* out_state);
AM_API am_status_t amv_shared_pool_get_stats(const amv_shared_pool_t* pool,
                                             am_shared_pool_stats_t* out_stats);
#ifdef __cplusplus
}
#endif
#endif
