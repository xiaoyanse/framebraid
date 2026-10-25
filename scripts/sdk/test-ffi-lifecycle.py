"""验证开发阶段实际DLL子集及并发生命周期；不是最终121入口整包验收。"""
import argparse
import ctypes as c
import json
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdk" if (ROOT / "sdk/bindings").is_dir() else ROOT
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    args = parser.parse_args()
    camera = c.CDLL(str((args.dll_dir / "am_camera.dll").resolve()))
    vision = c.CDLL(str((args.dll_dir / "am_vision.dll").resolve()))
    missing_camera = a.bind(camera, "amc_")
    missing_vision = a.bind(vision, "amv_")
    layouts = json.loads((SDK / "abi/layout-win-x64.json").read_text())
    for name, layout in layouts["structures"].items():
        assert c.sizeof(getattr(a, name)) == layout["size"], name
        assert c.alignment(getattr(a, name)) == layout["align"], name

    def cycle(_):
        for _ in range(100):
            params = a.amc_init_params_t()
            params.struct_size = c.sizeof(params)
            params.abi_version = a.AM_ABI_VERSION
            handle = c.POINTER(a.amc_context_t)()
            assert camera.amc_init(c.byref(params), c.byref(handle)) == a.AM_OK
            state = c.c_uint32()
            assert vision.amv_object_query_state(handle, c.byref(state)) == a.AM_ERR_SDK_HANDLE_INVALID
            assert camera.amc_shutdown(handle, 0, c.byref(state)) == a.AM_OK
            assert state.value == a.AM_LIFECYCLE_CLOSED
            assert camera.amc_shutdown(handle, 0, c.byref(state)) == a.AM_OK
            assert camera.amc_object_destroy(handle) == a.AM_OK

    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(cycle, range(4)))
    return {"layouts": len(layouts["structures"]), "parallelLifecycleCycles": 400,
            "actualCameraExports": sum(n.startswith("amc_") for n in a.FUNCTIONS) - len(missing_camera),
            "actualVisionExports": sum(n.startswith("amv_") for n in a.FUNCTIONS) - len(missing_vision),
            "missingCamera": missing_camera, "missingVision": missing_vision,
            "scope": "development-lifecycle-subset; final G0-G3 pending"}


if __name__ == "__main__":
    print(json.dumps(main(), ensure_ascii=False))
