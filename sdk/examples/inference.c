#include "am_vision.h"
#include <stdio.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
#include <stdlib.h>
#endif

/* 使用固定 1x3x2x2 FLOAT Identity 样本；错误路径也归还拥有的结果。 */
static int run_utf8(int argc, char** argv) {
    amv_context_t* ctx = NULL;
    amv_model_t* model = NULL;
    amv_result_t* result = NULL;
    amv_init_params_t init = {0};
    am_model_desc_t desc = {0};
    am_raw_preprocess_t prep = {0};
    am_frame_view_t frame = {0};
    am_pixel_view_t pixel = {0};
    am_image_input_t input = {0};
    am_infer_params_t params = {0};
    unsigned char rgb[12];
    uint64_t request = 0;
    uint32_t state = 0;
    int code = 1, close_code = 0;
    if (argc != 3) { fputs("Usage: inference.exe ORT_DIRECTORY IDENTITY_IMAGE_MODEL\n", stderr); return 2; }
    init.struct_size = sizeof(init); init.abi_version = AM_ABI_VERSION;
    init.runtime_dirs_utf8 = (const char* const*)&argv[1]; init.runtime_dir_count = 1; init.infer_threads = 2;
    code = amv_init(&init, &ctx); if (code != AM_OK) goto cleanup;
    desc.struct_size = sizeof(desc); desc.path_utf8 = argv[2]; desc.load_mode = AM_LOAD_RAW;
    code = amv_model_load(ctx, &desc, &model); if (code != AM_OK) goto cleanup;
    memset(rgb, 17, sizeof(rgb));
    frame.struct_size = sizeof(frame); frame.width = 2; frame.height = 2;
    frame.format = AM_PF_RGB8; frame.plane_count = 1; frame.planes[0] = rgb;
    frame.strides[0] = 6; frame.plane_size_bytes[0] = sizeof(rgb);
    pixel.struct_size = sizeof(pixel); pixel.magic = AM_PIXEL_VIEW_MAGIC; pixel.view = &frame;
    input.struct_size = sizeof(input); input.kind = AM_IMG_PIXEL_VIEW; input.view = &pixel;
    prep.struct_size = sizeof(prep); prep.resize = AM_RESIZE_STRETCH; prep.target_w = 2; prep.target_h = 2;
    params.struct_size = sizeof(params); params.raw_preprocess = &prep; params.confidence_threshold = 0.25f;
    code = amv_infer_image_sync(model, &input, &params, &result, &request);
    if (code != AM_OK) goto cleanup;
    if (result->raw_output_count != 1 || result->raw_outputs[0].data_size != 48) { code = 1; goto cleanup; }
    {
        float values[12]; size_t i;
        memcpy(values, result->raw_outputs[0].data, sizeof(values));
        for (i = 0; i < 12; ++i) if (values[i] != 17.0f) { code = 1; goto cleanup; }
    }
    code = amv_result_ack(ctx, request); request = 0;
cleanup:
    if (result != NULL) { close_code = amv_result_release(result); if (close_code != AM_OK) code = close_code; }
    if (request != 0 && ctx != NULL) {
        /* 同步截止后仍可能在运行；取消后由模型关闭完成屏障，不丢弃未知 ID。 */
        close_code = amv_request_cancel(ctx, request);
        if (code == AM_OK && close_code != AM_OK) code = close_code;
    }
    if (model != NULL) {
        close_code = amv_model_close(model, 5000, &state);
        if (close_code != AM_OK || state != AM_LIFECYCLE_CLOSED) { fputs("Model remains closing; do not destroy or unload\n", stderr); return 3; }
        close_code = amv_object_destroy(model); if (close_code != AM_OK) return 3;
    }
    if (ctx != NULL) {
        close_code = amv_shutdown(ctx, 5000, &state);
        if (close_code != AM_OK || state != AM_LIFECYCLE_CLOSED) { fputs("Context remains closing; do not destroy or unload\n", stderr); return 3; }
        close_code = amv_object_destroy(ctx); if (close_code != AM_OK) return 3;
    }
    if (code != AM_OK) { fprintf(stderr, "SDK operation failed: %d\n", code); return 1; }
    puts("C inference passed: CPU Identity, raw output, ack/release, close/destroy"); return 0;
}

#ifdef _WIN32
/* SDK只收UTF-8；宽命令行不能通过当前ANSI代码页中转。 */
int wmain(int argc, wchar_t** wide) {
    char* arguments[3] = {NULL, NULL, NULL};
    int i, code = 2;
    if (argc != 3) { fputs("Usage: inference.exe ORT_DIRECTORY IDENTITY_IMAGE_MODEL\n", stderr); return 2; }
    for (i = 0; i < 3; ++i) {
        int bytes = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide[i], -1, NULL, 0, NULL, NULL);
        if (bytes == 0) goto done;
        arguments[i] = (char*)malloc((size_t)bytes);
        if (arguments[i] == NULL || WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide[i], -1,
                                                       arguments[i], bytes, NULL, NULL) == 0) goto done;
    }
    code = run_utf8(argc, arguments);
done:
    for (i = 0; i < 3; ++i) free(arguments[i]);
    return code;
}
#else
int main(int argc, char** argv) { return run_utf8(argc, argv); }
#endif
