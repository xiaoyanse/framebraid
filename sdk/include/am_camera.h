/* C ABI公开头文件；由scripts/sdk/generate-abi.mjs生成，勿手工修改。 */
#ifndef AM_CAMERA_H
#define AM_CAMERA_H
#include "am_sdk_types.h"
#ifdef __cplusplus
extern "C" {
#endif
AM_API am_status_t amc_probe_decode_hardware(amc_context_t* ctx,
                                            uint32_t codec, uint32_t budget_ms);
AM_API am_status_t amc_get_decode_hardware_report(void* object, char* buffer_utf8,
                                                 uint64_t capacity, uint64_t* required);
AM_API am_status_t amc_source_set_decode_policy(amc_source_t* source, uint32_t policy);
AM_API uint32_t amc_get_abi_version(void);
AM_API am_status_t amc_get_version_info(am_version_info_t* out_info);
AM_API am_status_t amc_init(const amc_init_params_t* params, amc_context_t** out_ctx);
AM_API am_status_t amc_shutdown(amc_context_t* ctx, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amc_shutdown_query(amc_context_t* ctx, am_lifecycle_state_t* out_state);
AM_API am_status_t amc_get_init_error(am_init_error_t* out);
AM_API am_status_t amc_query_capability(amc_context_t* ctx, amc_capability_t* out);
AM_API am_status_t amc_credentials_set(amc_source_t* src, const am_credential_data_t* creds);
AM_API am_status_t amc_credentials_set_provider(amc_source_t* src, const am_credential_provider_t* provider);
AM_API am_status_t amc_credentials_invalidate(amc_source_t* src, uint32_t known_version);
AM_API am_status_t amc_probe_streams(amc_context_t* ctx, const am_probe_request_t* req, am_probe_result_t** out);
AM_API am_status_t amc_probe_result_release(am_probe_result_t* result);
AM_API am_status_t amc_source_open(amc_context_t* ctx, const am_source_desc_t* desc, amc_source_t** out_src);
AM_API am_status_t amc_source_close(amc_source_t* src, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amc_source_get_status(amc_source_t* src, am_source_status_t* out_status);
AM_API am_status_t amc_source_set_reconnect_policy(amc_source_t* src, const am_reconnect_policy_t* policy);
AM_API am_status_t amc_subscribe_encoded(amc_source_t* src, const am_encoded_sub_desc_t* desc, amc_encoded_sub_t** out_sub);
AM_API am_status_t amc_encoded_unsubscribe(amc_encoded_sub_t* sub, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amc_encoded_set_callback(amc_encoded_sub_t* sub, am_encoded_cb_fn cb, void* user_data);
AM_API am_status_t amc_encoded_pull(amc_encoded_sub_t* sub, uint32_t timeout_ms, am_encoded_packet_t** out_pkt);
AM_API am_status_t amc_encoded_packet_retain(am_encoded_packet_t* pkt);
AM_API am_status_t amc_encoded_packet_release(am_encoded_packet_t* pkt);
AM_API am_status_t amc_subscribe_frames(amc_source_t* src, const am_frame_sub_desc_t* desc, amc_frame_sub_t** out_sub);
AM_API am_status_t amc_frame_unsubscribe(amc_frame_sub_t* sub, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amc_frame_set_callback(amc_frame_sub_t* sub, am_frame_cb_fn cb, void* user_data);
AM_API am_status_t amc_frame_pull(amc_frame_sub_t* sub, uint32_t timeout_ms, amc_frame_t** out_frame);
AM_API am_status_t amc_frame_retain(amc_frame_t* frame);
AM_API am_status_t amc_frame_release(amc_frame_t* frame);
AM_API am_status_t amc_frame_get_view(const amc_frame_t* frame, am_frame_view_t* out_view);
AM_API am_status_t amc_frame_get_meta(const amc_frame_t* frame, am_frame_meta_t* out_meta);
AM_API am_status_t amc_frame_get_scaled(const amc_frame_t* frame, uint32_t target_w, uint32_t target_h,
                                        uint32_t method, amc_frame_t** out_scaled);
AM_API am_status_t amc_frame_as_pixel_view(amc_frame_t* frame, am_pixel_view_t* out_view);
AM_API am_status_t amc_gpu_frame_pool_create(amc_context_t* ctx,
    const void* cfg, amc_gpu_frame_pool_t** out_pool);
AM_API am_status_t amc_gpu_frame_borrow(const amc_gpu_frame_t* frame);
AM_API am_status_t amc_gpu_frame_return(amc_gpu_frame_t* frame);
AM_API am_status_t amc_gpu_frame_release(amc_gpu_frame_t* frame);
AM_API am_status_t amc_gpu_frame_stats(const amc_gpu_frame_t* frame,
    am_gpu_frame_stats_t* out);
AM_API am_status_t amc_gpu_frame_desc(const amc_gpu_frame_t* frame,
    am_gpu_frame_desc_t* out_desc);
AM_API am_status_t amc_gpu_frame_texture(const amc_gpu_frame_t* frame,
    void** out_texture);
AM_API am_status_t amc_capture_sync(amc_source_t* src, const am_capture_request_t* req, am_captured_image_t** out_img);
AM_API am_status_t amc_capture_async(amc_source_t* src, const am_capture_request_t* req, uint64_t* out_request_id);
AM_API am_status_t amc_captured_image_fetch(amc_context_t* ctx, uint64_t request_id, am_captured_image_t** out_img);
AM_API am_status_t amc_captured_image_ack(amc_context_t* ctx, uint64_t request_id);
AM_API am_status_t amc_image_retain(am_captured_image_t* img);
AM_API am_status_t amc_image_release(am_captured_image_t* img);
AM_API am_status_t amc_capture_continuous_start(amc_source_t* src, const am_capture_continuous_desc_t* desc,
                                                amc_capture_sub_t** out_sub);
AM_API am_status_t amc_capture_continuous_stop(amc_capture_sub_t* sub, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amc_capture_sub_set_callback(amc_capture_sub_t* sub, am_capture_cb_fn cb, void* user_data);
AM_API am_status_t amc_capture_sub_pull(amc_capture_sub_t* sub, uint32_t timeout_ms, am_captured_image_t** out_img);
AM_API am_status_t amc_capture_sub_get_stats(amc_capture_sub_t* sub, am_capture_stats_t* out_stats);
AM_API am_status_t amc_decoder_create(amc_context_t* ctx, const am_decoder_config_t* cfg, amc_decoder_t** out_dec);
AM_API am_status_t amc_decoder_submit(amc_decoder_t* dec, const am_decoder_input_t* pkt);
AM_API am_status_t amc_decoder_receive(amc_decoder_t* dec, uint32_t timeout_ms, amc_frame_t** out_frame);
AM_API am_status_t amc_decoder_drain(amc_decoder_t* dec);
AM_API am_status_t amc_decoder_reset(amc_decoder_t* dec, const am_decoder_config_t* new_cfg);
AM_API am_status_t amc_decoder_close(amc_decoder_t* dec, uint32_t timeout_ms, am_lifecycle_state_t* out_state);
AM_API am_status_t amc_decoder_get_status(amc_decoder_t* dec, am_decoder_status_t* out_status);
AM_API am_status_t amc_decoder_get_info(amc_decoder_t* dec, am_decoder_info_t* out_info);
AM_API am_status_t amc_source_set_event_callback(amc_source_t* src, am_source_event_fn cb, void* user_data);
AM_API am_status_t amc_request_query(amc_context_t* ctx, uint64_t request_id, am_request_snapshot_t** out_snapshot);
AM_API am_status_t amc_request_snapshot_release(am_request_snapshot_t* snapshot);
AM_API am_status_t amc_request_cancel(amc_context_t* ctx, uint64_t request_id);
AM_API am_status_t amc_get_limits(amc_context_t* ctx, am_limits_t* out_limits);
AM_API am_status_t amc_set_limits(amc_context_t* ctx, const am_limits_t* limits);
AM_API am_status_t amc_get_stats(amc_context_t* ctx, const char* scope_utf8, am_stats_t** out_stats);
AM_API am_status_t amc_stats_release(am_stats_t* stats);
AM_API am_status_t amc_get_last_error(const void* any_handle, am_error_info_t* out_info);
AM_API const char* amc_status_name(am_status_t status);
AM_API am_status_t amc_object_query_state(const void* any_object_handle, am_lifecycle_state_t* out_state);
AM_API am_status_t amc_object_request_close(const void* any_object_handle);
AM_API am_status_t amc_object_destroy(void* any_object_handle);
AM_API am_status_t amc_get_queue_policy(amc_context_t* ctx, uint32_t queue_class, am_queue_policy_t* out_policy);
AM_API am_status_t amc_set_queue_policy(amc_context_t* ctx, uint32_t queue_class, const am_queue_policy_t* policy);
AM_API void amc_frame_retain_bridge(void* owner);
AM_API void amc_frame_release_bridge(void* owner);
AM_API am_status_t amc_shared_pool_create(amc_context_t* ctx, const am_shared_pool_config_t* cfg,
                                          am_shared_pool_attach_desc_t* out_attach,
                                          amc_shared_pool_t** out_pool);
AM_API am_status_t amc_shared_pool_publish(amc_shared_pool_t* pool, const amc_frame_t* frame,
                                           uint64_t* out_publication_id);
AM_API am_status_t amc_shared_pool_publish_view(amc_shared_pool_t* pool, const am_pixel_view_t* view,
                                                uint64_t* out_publication_id);
AM_API am_status_t amc_shared_pool_get_attach(const amc_shared_pool_t* pool,
                                              am_shared_pool_attach_desc_t* out_attach);
AM_API am_status_t amc_shared_pool_close(amc_shared_pool_t* pool, uint32_t timeout_ms,
                                         am_lifecycle_state_t* out_state);
AM_API am_status_t amc_shared_pool_get_stats(const amc_shared_pool_t* pool,
                                             am_shared_pool_stats_t* out_stats);
AM_API am_status_t amc_set_worker_budget(amc_context_t* ctx, uint32_t max_workers);
AM_API am_status_t amc_get_worker_budget(amc_context_t* ctx, uint32_t* out_max_workers);
#ifdef __cplusplus
}
#endif
#endif
