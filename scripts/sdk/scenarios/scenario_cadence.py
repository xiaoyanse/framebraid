"""R2 场景：连续抓图节拍≈目标（GET 计数比率断言，复用 beat 模式）。

窗口内物理 GET 计数与送达帧数对目标节拍的比率双判据
（GET ≥0.8x、帧 ≥0.7x），语义与 test-capture-beat.py 一致并输出 JSON。
判读：退出码 0=过。
用法：python scenario_cadence.py --dll-dir <release> --runtime-dir <ffmpeg bin>
      [--target-ms 100] [--window-s 3] [--output result.json]
"""
import argparse
import ctypes as c
import importlib.util
import json
import platform
import sys
import threading
import time
from http.server import ThreadingHTTPServer
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "sdk" / "bindings" / "python"))
import amrt_sdk as a  # noqa: E402

spec = importlib.util.spec_from_file_location("capture_fixture", ROOT / "scripts" / "sdk" / "test-camera-capture.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)

DEFAULTS = {"target_ms": 100, "window_s": 3.0, "gets_ratio_min": 0.8, "frames_ratio_min": 0.7}


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


def pull_or_skip(lib, sub):
    out = c.POINTER(a.am_captured_image_t)()
    code = lib.amc_capture_sub_pull(sub, 20, c.byref(out))
    if code in (a.AM_ERR_SDK_TRY_AGAIN, a.AM_ERR_SDK_DEADLINE_EXCEEDED):
        return 0
    checked(lib, code, "sub_pull")
    checked(lib, lib.amc_image_release(out), "image_release")
    return 1


def run(lib, ctx, port, thresholds):
    target = thresholds["target_ms"]
    desc = sized(a.am_capture_continuous_desc_t)
    desc.target_interval_ms = target
    desc.request_timeout_ms = 1000
    desc.max_inflight, desc.queue_max_items, desc.queue_max_bytes = 1, 4, 32 << 20
    desc.mode = a.AM_SUB_PULL
    source = fixture.open_source(lib, ctx, port)
    sub = c.POINTER(a.amc_capture_sub_t)()
    try:
        checked(lib, lib.amc_capture_continuous_start(source, c.byref(desc), c.byref(sub)), "continuous_start")
        time.sleep(0.3)
        before = fixture.Fixture.requests
        started = time.monotonic()
        frames = 0
        while time.monotonic() - started < thresholds["window_s"]:
            frames += pull_or_skip(lib, sub)
        gets = fixture.Fixture.requests - before
    finally:
        checked(lib, lib.amc_object_request_close(sub), "sub_request_close")
        state = c.c_uint32()
        until = time.monotonic() + 5
        while True:
            checked(lib, lib.amc_object_query_state(sub, c.byref(state)), "query_state")
            if state.value == a.AM_LIFECYCLE_CLOSED:
                break
            assert time.monotonic() < until, state.value
            time.sleep(0.005)
        checked(lib, lib.amc_object_destroy(sub), "destroy")
        fixture.close_object(lib, source)
    expected = thresholds["window_s"] * 1000.0 / target
    gets_ratio = gets / expected
    frames_ratio = frames / expected
    assert gets_ratio >= thresholds["gets_ratio_min"], \
        f"GET 比率 {gets_ratio:.2f} 低于 {thresholds['gets_ratio_min']} (gets={gets}, expected~{expected:.0f})"
    assert frames_ratio >= thresholds["frames_ratio_min"], \
        f"帧比率 {frames_ratio:.2f} 低于 {thresholds['frames_ratio_min']} (frames={frames})"
    return {
        "target_ms": target,
        "window_s": thresholds["window_s"],
        "gets": gets,
        "frames": frames,
        "expected": round(expected, 1),
        "gets_ratio": round(gets_ratio, 3),
        "frames_ratio": round(frames_ratio, 3),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--target-ms", type=int, default=None)
    parser.add_argument("--window-s", type=float, default=None)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    stage = "init"
    try:
        thresholds = dict(DEFAULTS)
        if args.target_ms:
            thresholds["target_ms"] = args.target_ms
        if args.window_s:
            thresholds["window_s"] = args.window_s
        lib, ctx = init_context(args.dll_dir, args.runtime_dir)
        server = None
        result = {"ok": True, "schema": "framebraid.scenario-cadence/1", "os": platform.platform()}
        try:
            stage = "server"
            server = ThreadingHTTPServer(("127.0.0.1", 0), fixture.Fixture)
            server.daemon_threads = True
            threading.Thread(target=server.serve_forever, daemon=True).start()
            stage = "window"
            row = run(lib, ctx, server.server_address[1], thresholds)
            result["cadence"] = row
            print(f"[cadence] target={row['target_ms']}ms gets_ratio={row['gets_ratio']} "
                  f"frames_ratio={row['frames_ratio']}", flush=True)
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
