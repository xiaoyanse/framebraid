"""R1 场景：4K 档 cpu_frame 获取与默认帧预算（2560x1440 与 3840x2160）。

正向判据（硬）：代表性相机帧（确定性梯度 JPEG）cpu_frame 成功、尺寸/步长
正确、释放后队列归零——默认 96MiB 预算不拒真实 4K 帧。
预算执行负例（软）：超尺寸编码（确定性噪声 JPEG）被干净拒绝
（RESOURCE_EXHAUSTED），峰值计费 ≈ (4+7)B/px + 2xencoded，encoded 余量有限。
判读：退出码 0=过。
用法：python scenario_budget_4k.py --dll-dir <release> --runtime-dir <ffmpeg bin>
      [--output result.json]
"""
import argparse
import ctypes as c
import importlib.util
import json
import platform
import sys
import threading
from http.server import ThreadingHTTPServer
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "sdk" / "bindings" / "python"))
import amrt_sdk as a  # noqa: E402
import cv2  # noqa: E402
import numpy as np  # gradient fixture uses cv2

spec = importlib.util.spec_from_file_location("capture_fixture", ROOT / "scripts" / "sdk" / "test-camera-capture.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)

HEIGHTS = (1440, 2160)
SEED_BASE = 20261010


def gradient_jpeg(height):
    width = height * 16 // 9
    yy, xx = np.mgrid[0:height, 0:width]
    image = np.stack([(xx * 255 // width), (yy * 255 // height), ((xx + yy) * 255 // (width + height))],
                     axis=2).astype(np.uint8)
    ok, encoded = cv2.imencode(".jpg", image, [cv2.IMWRITE_JPEG_QUALITY, 85])
    assert ok
    return encoded.tobytes()


def noise_jpeg(height):
    rng = np.random.default_rng(SEED_BASE + height)
    pixels = rng.integers(0, 256, (height, height * 16 // 9, 3), dtype=np.uint8)
    ok, encoded = cv2.imencode(".jpg", pixels, [cv2.IMWRITE_JPEG_QUALITY, 85])
    assert ok
    return encoded.tobytes()


def checked(lib, code, stage=""):
    if code != 0:
        name = lib.amc_status_name(code)
        message = name.decode() if name else ""
        raise AssertionError(f"{stage} code={code} {message}")


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def init_context(dll_dir, runtime_dir):
    lib = c.CDLL(str((dll_dir / "am_camera.dll").resolve()))
    a.bind(lib, "amc_")
    dirs = (c.c_char_p * 1)(str(runtime_dir.resolve()).encode())
    params = sized(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    params.runtime_dirs_utf8, params.runtime_dir_count = dirs, 1
    ctx = c.POINTER(a.amc_context_t)()
    checked(lib, lib.amc_init(c.byref(params), c.byref(ctx)), "amc_init")
    return lib, ctx


def run_cell(lib, ctx, port, height):
    fixture.Fixture.image = gradient_jpeg(height)
    source = fixture.open_source(lib, ctx, port)
    try:
        image = fixture.captured(lib, source, a.AM_CAP_CPU_FRAME)
        try:
            view = image.contents.view.contents
            dims = (view.width, view.height, view.format)
            expected = (height * 16 // 9, height, a.AM_PF_RGBA8)
            assert dims == expected, f"cpu_frame 尺寸不符: {dims} != {expected}"
            assert view.plane_size_bytes[0] == expected[0] * expected[1] * 4, view.plane_size_bytes[0]
        finally:
            checked(lib, lib.amc_image_release(image), "image_release")
        assert fixture.stats(lib, ctx) == (0, 0, 0), "释放后队列未归零"
        return {"height": height, "width": height * 16 // 9, "format": "RGBA8",
                "encoded_bytes": len(fixture.Fixture.image), "within_budget": True}
    finally:
        fixture.close_object(lib, source)


def budget_enforcement_probe(lib, ctx, port):
    """超尺寸编码负例：被干净拒绝=预算执行正常；被接纳=峰值计费口径已变，记录供复核。"""
    fixture.Fixture.image = noise_jpeg(2160)
    source = fixture.open_source(lib, ctx, port)
    try:
        req = sized(a.am_capture_request_t)
        req.output_kind, req.deadline_ms = a.AM_CAP_CPU_FRAME, 5000
        out = c.POINTER(a.am_captured_image_t)()
        code = lib.amc_capture_sync(source, c.byref(req), c.byref(out))
        if code == 0:
            checked(lib, lib.amc_image_release(out), "image_release")
            return {"encoded_bytes": len(fixture.Fixture.image), "outcome": "admitted",
                    "note": "峰值计费口径变化，复核 (4+7)B/px+2xencoded 公式"}
        assert code == a.AM_ERR_SDK_RESOURCE_EXHAUSTED, \
            f"超尺寸帧应干净拒绝 RESOURCE_EXHAUSTED，实际 code={code}"
        return {"encoded_bytes": len(fixture.Fixture.image), "outcome": "rejected_cleanly",
                "code_name": "sdk.resourceExhausted"}
    finally:
        fixture.close_object(lib, source)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    stage = "init"
    try:
        lib, ctx = init_context(args.dll_dir, args.runtime_dir)
        limits = sized(a.am_limits_t)
        checked(lib, lib.amc_get_limits(ctx, c.byref(limits)), "get_limits")
        server = None
        result = {"ok": True, "schema": "framebraid.scenario-budget-4k/1",
                  "os": platform.platform(), "frame_budget_bytes": limits.frame_budget_bytes,
                  "cells": [], "budget_enforcement": None}
        try:
            stage = "server"
            server = ThreadingHTTPServer(("127.0.0.1", 0), fixture.Fixture)
            server.daemon_threads = True
            threading.Thread(target=server.serve_forever, daemon=True).start()
            port = server.server_address[1]
            for height in HEIGHTS:
                stage = f"capture-{height}"
                row = run_cell(lib, ctx, port, height)
                result["cells"].append(row)
                print(f"[budget] {height}p cpu_frame ok encoded={row['encoded_bytes']}B", flush=True)
            stage = "budget-enforcement"
            probe = budget_enforcement_probe(lib, ctx, port)
            result["budget_enforcement"] = probe
            print(f"[budget] 2160p noise {probe['encoded_bytes']}B outcome={probe['outcome']}", flush=True)
            if args.output:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps(result, ensure_ascii=False, indent=1), encoding="utf-8")
            print(json.dumps(result, ensure_ascii=False))
        finally:
            if server is not None:
                server.shutdown()
                server.server_close()
            fixture.shared.close(lib, ctx, True)
    except Exception as error:
        print(json.dumps({"ok": False, "stage": stage, "error": repr(error)}, ensure_ascii=False))
        sys.exit(1)


if __name__ == "__main__":
    main()
