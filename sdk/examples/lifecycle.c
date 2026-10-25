#include "am_camera.h"
#include "am_vision.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

int main(void) {
  amc_context_t* camera = NULL;
  amv_context_t* vision = NULL;
  amc_init_params_t cp = {0};
  amv_init_params_t vp = {0};
  am_version_info_t version = {0};
  amc_capability_t cap = {0};
  am_init_error_t failure = {0};
  am_limits_t limits = {0};
  am_queue_policy_t queue = {0};
  uint32_t state = 0;
  version.struct_size = sizeof(version);
  assert(amc_get_abi_version() == AM_ABI_VERSION);
  assert(amv_get_abi_version() == AM_ABI_VERSION);
  assert(amc_get_version_info(&version) == AM_OK);
  assert(version.abi_major == 1);
  assert(strcmp(amc_status_name(AM_ERR_SDK_INTERNAL_ERROR), "sdk.internalError") == 0);
  cp.struct_size = sizeof(cp);
  cp.abi_version = AM_ABI_VERSION;
  vp.struct_size = sizeof(vp);
  vp.abi_version = AM_ABI_VERSION;
  /* 仅验证共享帧/生命周期，不要求安装推理运行库；推理示例须显式提供目录。 */
  vp.feature_flags = AM_INIT_SHARED_FRAMES;
  assert(amc_init(&cp, &camera) == AM_OK);
  assert(amv_init(&vp, &vision) == AM_OK);
  assert(amv_object_query_state(camera, &state) == AM_ERR_SDK_HANDLE_INVALID);
  cap.struct_size = sizeof(cap);
  assert(amc_query_capability(camera, &cap) == AM_OK);
  assert((cap.feature_bits & ~(AMC_CAP_SHARED_FRAME_POOL | AMC_CAP_DECODE_H264 |
      AMC_CAP_DECODE_H265 | AMC_CAP_DECODER_STANDALONE | AMC_CAP_BSF_ANNEXB)) == 0);
  queue.struct_size = sizeof(queue);
  assert(amc_get_queue_policy(camera, AM_Q_ENCODED_STREAM, &queue) == AM_OK);
  assert(queue.max_items == 36);
  queue.max_items = 10;
  assert(amc_set_queue_policy(camera, AM_Q_ENCODED_STREAM, &queue) == AM_OK);
  queue.max_items = 11;
  assert(amc_set_queue_policy(camera, AM_Q_ENCODED_STREAM, &queue) == AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
  assert(amv_get_queue_policy(vision, AM_Q_ENCODED_STREAM, &queue) == AM_ERR_SDK_NOT_SUPPORTED);
  limits.struct_size = sizeof(limits);
  assert(amc_get_limits(camera, &limits) == AM_OK);
  assert(limits.max_sources == 16);
  limits.max_sources = 0;
  assert(amc_set_limits(camera, &limits) == AM_OK);
  limits.max_sources = 1;
  assert(amc_set_limits(camera, &limits) == AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
  assert(amc_shutdown(camera, 0, &state) == AM_OK);
  assert(state == AM_LIFECYCLE_CLOSED);
  assert(amc_shutdown(camera, 0, &state) == AM_OK);
  assert(amc_set_limits(camera, &limits) == AM_ERR_SDK_HANDLE_CLOSED);
  assert(amc_object_destroy(camera) == AM_OK);
  assert(amv_shutdown(vision, 0, &state) == AM_OK);
  assert(amv_object_destroy(vision) == AM_OK);
  cp.abi_version = 0;
  assert(amc_init(&cp, &camera) == AM_ERR_SDK_ABI_UNSUPPORTED);
  assert(camera == NULL);
  failure.struct_size = sizeof(failure);
  assert(amc_get_init_error(&failure) == AM_OK);
  assert(failure.code == AM_ERR_SDK_ABI_UNSUPPORTED);
  assert(strcmp(failure.message, "sdk.abiUnsupported") == 0);
  assert(amv_get_init_error(&failure) == AM_OK);
  assert(failure.code == AM_OK);
  puts("native lifecycle passed: separate DLL domains, limits, close/destroy, init errors");
  return 0;
}
