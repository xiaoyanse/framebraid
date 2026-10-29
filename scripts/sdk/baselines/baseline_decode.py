"""解码基线：SDK 抓图解码路径 vs OpenCV 配对（同确定性 JPEG，同输入 sha256 铁律）。

同一确定性 JPEG（numpy 固定种子+q85）喂两臂：SDK 臂走抓图 CPU_FRAME
（本地 Digest 替身请求/响应节流，单次=HTTP+MJPEG 解码+RGBA8 拷贝），
cv2 臂 cv2.imdecode 纯解码；P50/P95 + --baseline 漂移断言。
判读：退出码 0=过；输入 sha256 不一致或 p50 漂移超限=FAIL。
用法：python baseline_decode.py --dll-dir <release> --runtime-dir <ffmpeg bin>
      [--baseline recorded.json] [--output result.json]
"""
import argparse
import ctypes as c
import hashlib
import importlib.util
import json
import platform
import sys
import threading
import time
from http.server import ThreadingHTTPServer
from pathlib import Path
from statistics import median

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "sdk" / "bindings" / "python"))
import amrt_sdk as a  # noqa: E402
import cv2  # noqa: E402
import numpy as np  # noqa: E402

spec = importlib.util.spec_from_file_location("capture_fixture", ROOT / "scripts" / "sdk" / "test-camera-capture.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)

HEIGHTS = (1440, 2160)
WARMUP = 5
SAMPLES = 30
SEED_BASE = 20261010


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


def fixture_jpeg(height):
    width = height * 16 // 9
    yy, xx = np.mgrid[0:height, 0:width]
    pixels = np.stack([(xx * 255 // width), (yy * 255 // height), ((xx + yy) * 255 // (width + height))],
                      axis=2).astype(np.uint8)
    ok, encoded = cv2.imencode(".jpg", pixels, [cv2.IMWRITE_JPEG_QUALITY, 85])
    assert ok
    return encoded.tobytes()


def summary(samples):
    ordered = sorted(samples)
    return {
        "p50_ms": round(median(samples), 3),
        "p95_ms": round(ordered[max(0, int(len(ordered) * 0.95) - 1)], 3),
    }


def sdk_capture_arm(lib, ctx, port, jpeg, height):
    fixture.Fixture.image = jpeg
    source = fixture.open_source(lib, ctx, port)
    try:
        req = sized(a.am_capture_request_t)
        req.output_kind, req.deadline_ms = a.AM_CAP_CPU_FRAME, 10000
        out = c.POINTER(a.am_captured_image_t)()
        samples = []
        for i in range(WARMUP + SAMPLES):
            started = time.perf_counter()
            code = lib.amc_capture_sync(source, c.byref(req), c.byref(out))
            elapsed_ms = (time.perf_counter() - started) * 1000
            checked(lib, code, "capture_sync")
            try:
                view = out.contents.view.contents
                expected = (height * 16 // 9, height, a.AM_PF_RGBA8)
                dims = (view.width, view.height, view.format)
                assert dims == expected, f"cpu_frame 尺寸不符: {dims} != {expected}"
                assert view.plane_size_bytes[0] == expected[0] * expected[1] * 4, view.plane_size_bytes[0]
            finally:
                checked(lib, lib.amc_image_release(out), "image_release")
            if i >= WARMUP:
                samples.append(elapsed_ms)
        return summary(samples)
    finally:
        fixture.close_object(lib, source)


def cv_arm(jpeg):
    array = np.frombuffer(jpeg, dtype=np.uint8)
    samples = []
    for _ in range(WARMUP + SAMPLES):
        started = time.perf_counter()
        image = cv2.imdecode(array, cv2.IMREAD_COLOR)
        elapsed_ms = (time.perf_counter() - started) * 1000
        assert image is not None
        samples.append(elapsed_ms)
    return summary(samples)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, default=None)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    stage = "fixture"
    try:
        jpegs = {height: fixture_jpeg(height) for height in HEIGHTS}
        digests = {h: hashlib.sha256(j).hexdigest() for h, j in jpegs.items()}
        baseline = None
        if args.baseline:
            baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
            for height, digest in digests.items():
                recorded = next(cc for cc in baseline["decode"]["cells"] if cc["height"] == height)
                assert digest == recorded["jpeg_sha256"], f"{height}p 输入 sha256 漂移"
            print("[input] fixture sha256 与 recorded.json 一致", flush=True)

        stage = "init"
        lib, ctx = init_context(args.dll_dir, args.runtime_dir)
        result = {
            "ok": True,
            "schema": "framebraid.baseline-decode/1",
            "arms": "sdk=capture_cpu_frame(http+decode+copy) vs cv=imdecode",
            "os": platform.platform(),
            "warmup": WARMUP,
            "samples": SAMPLES,
            "cells": [],
        }
        limits = sized(a.am_limits_t)
        checked(lib, lib.amc_get_limits(ctx, c.byref(limits)), "get_limits")
        result["frame_budget_bytes"] = limits.frame_budget_bytes
        server = None
        try:
            stage = "server"
            server = ThreadingHTTPServer(("127.0.0.1", 0), fixture.Fixture)
            server.daemon_threads = True
            threading.Thread(target=server.serve_forever, daemon=True).start()
            for height in HEIGHTS:
                jpeg = jpegs[height]
                stage = f"sdk-{height}"
                sdk_stats = sdk_capture_arm(lib, ctx, server.server_address[1], jpeg, height)
                stage = f"cv-{height}"
                cv_stats = cv_arm(jpeg)
                cell = {"height": height, "jpeg_sha256": digests[height],
                        "sdk": sdk_stats, "cv": cv_stats}
                if baseline:
                    recorded = next(cc for cc in baseline["decode"]["cells"] if cc["height"] == height)
                    cell["drift_ms"] = {
                        arm: round((sdk_stats if arm == "sdk" else cv_stats)["p50_ms"] - recorded[f"{arm}_p50_ms"], 3)
                        for arm in ("sdk", "cv")
                    }
                    for arm in ("sdk", "cv"):
                        observed = (sdk_stats if arm == "sdk" else cv_stats)["p50_ms"]
                        tolerance = recorded.get("tolerance_pct", 25.0)
                        floor = recorded.get("abs_floor_ms", 2.0)
                        delta = observed - recorded[f"{arm}_p50_ms"]
                        assert abs(delta) <= max(recorded[f"{arm}_p50_ms"] * tolerance / 100.0, floor), (
                            f"{height}p {arm} p50 漂移超限: 实测 {observed:.2f}ms vs "
                            f"基线 {recorded[f'{arm}_p50_ms']:.2f}ms (容差 max({tolerance}%, {floor}ms))"
                        )
                result["cells"].append(cell)
                print(f"[cell] {height}p sdk_p50={sdk_stats['p50_ms']:.2f}ms sdk_p95={sdk_stats['p95_ms']:.2f}ms", flush=True)
                print(f"[cell] {height}p cv_p50={cv_stats['p50_ms']:.2f}ms cv_p95={cv_stats['p95_ms']:.2f}ms", flush=True)
        finally:
            if server is not None:
                server.shutdown()
                server.server_close()
            checked(lib, lib.amc_shutdown(ctx, 100, c.byref(c.c_uint32())), "shutdown")
            lib.amc_object_destroy(ctx)
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(result, ensure_ascii=False, indent=1), encoding="utf-8")
        print(json.dumps(result, ensure_ascii=False))
    except Exception as error:
        print(json.dumps({"ok": False, "stage": stage, "error": repr(error)}, ensure_ascii=False))
        sys.exit(1)


if __name__ == "__main__":
    main()
