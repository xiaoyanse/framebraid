#include "am_camera.h"
#include "am_vision.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

int main(void) {
  amc_context_t* camera = NULL;
  amv_context_t* vision = NULL;
  amc_shared_pool_t* publisher = NULL;
  amv_shared_pool_t* consumer = NULL;
  amc_init_params_t cp = {0};
  amv_init_params_t vp = {0};
  am_shared_pool_config_t cfg = {0};
  am_shared_pool_attach_desc_t attach = {0};
  am_shared_frame_view_t borrowed = {0};
  am_pixel_view_t pixel = {0}, bridge = {0};
  am_frame_view_t frame = {0};
  am_frame_assoc_t assoc = {0};
  unsigned char bytes[48];
  uint64_t publication = 0;
  uint32_t state = 0;
  memset(bytes, 17, sizeof(bytes));
  cp.struct_size = sizeof(cp); cp.abi_version = AM_ABI_VERSION; cp.feature_flags = AM_INIT_SHARED_FRAMES;
  vp.struct_size = sizeof(vp); vp.abi_version = AM_ABI_VERSION; vp.feature_flags = AM_INIT_SHARED_FRAMES;
  assert(amc_init(&cp, &camera) == AM_OK);
  assert(amv_init(&vp, &vision) == AM_OK);
  cfg.struct_size = sizeof(cfg); cfg.slot_count = 1; cfg.max_frame_bytes = 64;
  cfg.total_bytes = 64; cfg.max_consumers = 1; cfg.max_outstanding_borrows = 2;
  attach.struct_size = sizeof(attach);
  assert(amc_shared_pool_create(camera, &cfg, &attach, &publisher) == AM_OK);
  assert(amv_shared_pool_attach(vision, &attach, &consumer) == AM_OK);
  frame.struct_size = sizeof(frame); frame.width = 4; frame.height = 4;
  frame.format = AM_PF_RGB8; frame.plane_count = 1; frame.planes[0] = bytes;
  frame.strides[0] = 12; frame.plane_size_bytes[0] = sizeof(bytes);
  assoc.struct_size = sizeof(assoc); assoc.source_id = 70; assoc.frame_id = 99;
  assoc.source_generation = 2; assoc.valid = AM_ASSOC_SOURCE | AM_ASSOC_FRAME | AM_ASSOC_GENERATION;
  assoc.pts = INT64_MIN; assoc.captured_at_ms = INT64_MIN; assoc.received_at_ms = INT64_MIN;
  pixel.struct_size = sizeof(pixel); pixel.magic = AM_PIXEL_VIEW_MAGIC;
  pixel.view = &frame; pixel.assoc = &assoc;
  assert(amc_shared_pool_publish_view(publisher, &pixel, &publication) == AM_OK);
  borrowed.struct_size = sizeof(borrowed);
  assert(amv_shared_pool_borrow(consumer, publication, 0, &borrowed) == AM_OK);
  assert(memcmp(bytes, borrowed.plane_data[0], sizeof(bytes)) == 0);
  bridge.struct_size = sizeof(bridge);
  assert(amv_shared_frame_as_pixel_view(consumer, &borrowed, &bridge) == AM_OK);
  assert(bridge.assoc->frame_id == 99);
  bridge.retain_owner(bridge.owner_handle);
  assert(amv_shared_pool_return(consumer, &borrowed) == AM_OK);
  assert(amc_shared_pool_publish_view(publisher, &pixel, &publication) == AM_ERR_SDK_QUEUE_FULL);
  assert(publication == 0);
  bridge.release_owner(bridge.owner_handle);
  assert(amc_shared_pool_publish_view(publisher, &pixel, &publication) == AM_OK);
  assert(amv_shared_pool_detach(consumer, 100, &state) == AM_OK && state == AM_LIFECYCLE_CLOSED);
  assert(amv_object_destroy(consumer) == AM_OK);
  assert(amc_shared_pool_close(publisher, 100, &state) == AM_OK && state == AM_LIFECYCLE_CLOSED);
  assert(amc_object_destroy(publisher) == AM_OK);
  assert(amv_shutdown(vision, 100, &state) == AM_OK); assert(amv_object_destroy(vision) == AM_OK);
  assert(amc_shutdown(camera, 100, &state) == AM_OK); assert(amc_object_destroy(camera) == AM_OK);
  puts("native shared frame passed: copy, identity, lease bridge, close/destroy");
  return 0;
}
