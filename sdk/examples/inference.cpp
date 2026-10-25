#include "am_camera.h"
#include "am_vision.h"
#include <array>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <string>
#ifdef _WIN32
#define NOMINMAX
#include <windows.h>
#endif

namespace {
void check(am_status_t status) {
    if (status != AM_OK) throw std::runtime_error("SDK status " + std::to_string(status));
}
template <typename T, am_status_t (*Close)(T*, uint32_t, uint32_t*), am_status_t (*Destroy)(void*)>
struct ObjectDeleter {
    void operator()(T* object) const noexcept {
        uint32_t state = AM_LIFECYCLE_CLOSING;
        if (Close(object, 5000, &state) != AM_OK || state != AM_LIFECYCLE_CLOSED) {
            std::cerr << "SDK object remains closing; retained without forced destroy\n";
            return;
        }
        if (Destroy(object) != AM_OK) std::cerr << "SDK destroy failed\n";
    }
};
using Camera = std::unique_ptr<amc_context_t, ObjectDeleter<amc_context_t, amc_shutdown, amc_object_destroy>>;
using Vision = std::unique_ptr<amv_context_t, ObjectDeleter<amv_context_t, amv_shutdown, amv_object_destroy>>;
using Model = std::unique_ptr<amv_model_t, ObjectDeleter<amv_model_t, amv_model_close, amv_object_destroy>>;
using Publisher = std::unique_ptr<amc_shared_pool_t, ObjectDeleter<amc_shared_pool_t, amc_shared_pool_close, amc_object_destroy>>;
using Consumer = std::unique_ptr<amv_shared_pool_t, ObjectDeleter<amv_shared_pool_t, amv_shared_pool_detach, amv_object_destroy>>;
struct ResultDeleter { void operator()(amv_result_t* result) const noexcept { if (amv_result_release(result) != AM_OK) std::cerr << "Result release failed\n"; } };
using Result = std::unique_ptr<amv_result_t, ResultDeleter>;

class Borrow {
public:
    explicit Borrow(amv_shared_pool_t* pool, uint64_t publication) : pool_(pool) {
        view_.struct_size = sizeof(view_);
        check(amv_shared_pool_borrow(pool_, publication, 0, &view_));
    }
    Borrow(const Borrow&) = delete;
    Borrow& operator=(const Borrow&) = delete;
    ~Borrow() { if (amv_shared_pool_return(pool_, &view_) != AM_OK) std::cerr << "Shared frame return failed\n"; }
    am_pixel_view_t pixel() const {
        am_pixel_view_t result{}; result.struct_size = sizeof(result);
        check(amv_shared_frame_as_pixel_view(pool_, &view_, &result)); return result;
    }
private:
    amv_shared_pool_t* pool_;
    am_shared_frame_view_t view_{};
};
}

static int run_utf8(int argc, const char* const* argv) {
    if (argc != 3) { std::cerr << "Usage: inference_cpp.exe ORT_DIRECTORY IDENTITY_IMAGE_MODEL\n"; return 2; }
    try {
        amc_init_params_t cp{}; cp.struct_size = sizeof(cp); cp.abi_version = AM_ABI_VERSION; cp.feature_flags = AM_INIT_SHARED_FRAMES;
        amc_context_t* cr = nullptr; check(amc_init(&cp, &cr)); Camera camera(cr);
        amv_init_params_t vp{}; vp.struct_size = sizeof(vp); vp.abi_version = AM_ABI_VERSION; vp.feature_flags = AM_INIT_SHARED_FRAMES;
        const std::array<const char*, 1> directories{argv[1]}; vp.runtime_dirs_utf8 = directories.data(); vp.runtime_dir_count = 1; vp.infer_threads = 2;
        amv_context_t* vr = nullptr; check(amv_init(&vp, &vr)); Vision vision(vr);
        am_model_desc_t md{}; md.struct_size = sizeof(md); md.path_utf8 = argv[2]; md.load_mode = AM_LOAD_RAW;
        amv_model_t* mr = nullptr; check(amv_model_load(vision.get(), &md, &mr)); Model model(mr);
        am_shared_pool_config_t config{}; config.struct_size = sizeof(config); config.slot_count = 1;
        config.max_frame_bytes = 64; config.total_bytes = 64; config.max_consumers = 1; config.max_outstanding_borrows = 1;
        am_shared_pool_attach_desc_t attach{}; attach.struct_size = sizeof(attach);
        amc_shared_pool_t* pr = nullptr; check(amc_shared_pool_create(camera.get(), &config, &attach, &pr)); Publisher publisher(pr);
        amv_shared_pool_t* sr = nullptr; check(amv_shared_pool_attach(vision.get(), &attach, &sr)); Consumer consumer(sr);
        std::array<uint8_t, 12> pixels{}; pixels.fill(17);
        am_frame_view_t frame{}; frame.struct_size = sizeof(frame); frame.width = 2; frame.height = 2;
        frame.format = AM_PF_RGB8; frame.plane_count = 1; frame.planes[0] = pixels.data(); frame.strides[0] = 6; frame.plane_size_bytes[0] = pixels.size();
        am_pixel_view_t pixel{}; pixel.struct_size = sizeof(pixel); pixel.magic = AM_PIXEL_VIEW_MAGIC; pixel.view = &frame;
        am_frame_assoc_t association{}; association.struct_size = sizeof(association);
        association.source_id = 1; association.frame_id = 1; association.source_generation = 1;
        association.valid = AM_ASSOC_SOURCE | AM_ASSOC_FRAME | AM_ASSOC_GENERATION;
        association.pts = association.captured_at_ms = association.received_at_ms = INT64_MIN;
        pixel.assoc = &association;
        uint64_t publication = 0; check(amc_shared_pool_publish_view(publisher.get(), &pixel, &publication));
        {
            Borrow borrowed(consumer.get(), publication); auto shared = borrowed.pixel();
            am_image_input_t image{}; image.struct_size = sizeof(image); image.kind = AM_IMG_PIXEL_VIEW; image.view = &shared;
            am_raw_preprocess_t prep{}; prep.struct_size = sizeof(prep); prep.resize = AM_RESIZE_STRETCH; prep.target_w = 2; prep.target_h = 2;
            am_infer_params_t params{}; params.struct_size = sizeof(params); params.raw_preprocess = &prep; params.confidence_threshold = 0.25f;
            uint64_t request = 0; amv_result_t* rr = nullptr;
            check(amv_infer_image_sync(model.get(), &image, &params, &rr, &request)); Result result(rr);
            check(amv_result_ack(vision.get(), request));
            if (result->raw_output_count != 1 || result->raw_outputs[0].data_size != 48) throw std::runtime_error("Unexpected Identity output");
            std::array<float, 12> values{}; std::memcpy(values.data(), result->raw_outputs[0].data, sizeof(values));
            for (float value : values) if (value != 17.0f) throw std::runtime_error("Identity mismatch");
        }
        std::cout << "C++ inference passed: CPU shared frame, result RAII, reverse dependency cleanup\n";
        return 0;
    } catch (const std::exception& error) { std::cerr << error.what() << '\n'; return 1; }
}

#ifdef _WIN32
int wmain(int argc, wchar_t** wide) {
    if (argc != 3) { std::cerr << "Usage: inference_cpp.exe ORT_DIRECTORY IDENTITY_IMAGE_MODEL\n"; return 2; }
    try {
        std::array<std::string, 3> utf8;
        std::array<const char*, 3> arguments{};
        for (std::size_t i = 0; i < utf8.size(); ++i) {
            const int bytes = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide[i], -1, nullptr, 0, nullptr, nullptr);
            if (bytes == 0) throw std::runtime_error("Invalid command line encoding");
            utf8[i].resize(static_cast<std::size_t>(bytes));
            if (WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide[i], -1, utf8[i].data(), bytes, nullptr, nullptr) == 0)
                throw std::runtime_error("UTF-8 conversion failed");
            arguments[i] = utf8[i].c_str();
        }
        return run_utf8(argc, arguments.data());
    } catch (const std::exception& error) { std::cerr << error.what() << '\n'; return 2; }
}
#else
int main(int argc, char** argv) { return run_utf8(argc, argv); }
#endif
