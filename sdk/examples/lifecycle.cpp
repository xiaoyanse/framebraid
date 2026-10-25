#include "am_camera.h"
#include <cassert>
#include <iostream>
#include <memory>

struct CameraDeleter {
    void operator()(amc_context_t* context) const noexcept {
        uint32_t state = AM_LIFECYCLE_CLOSING;
        const auto closed = amc_shutdown(context, 5000, &state);
        if (closed == AM_OK && state == AM_LIFECYCLE_CLOSED) {
            const auto destroyed = amc_object_destroy(context);
            assert(destroyed == AM_OK);
        }
    }
};

int main() {
    amc_init_params_t parameters{};
    parameters.struct_size = sizeof(parameters);
    parameters.abi_version = AM_ABI_VERSION;
    amc_context_t* raw = nullptr;
    assert(amc_init(&parameters, &raw) == AM_OK);
    const std::unique_ptr<amc_context_t, CameraDeleter> camera(raw);
    am_limits_t limits{};
    limits.struct_size = sizeof(limits);
    assert(amc_get_limits(camera.get(), &limits) == AM_OK);
    assert(limits.shared_pool_max_total_bytes == 64 * 1024 * 1024);
    std::cout << "C++ lifecycle passed: RAII closes before destroy\n";
}
