"""节拍基线：连续抓图目标间隔 vs 实际 GET 间隔分布（P50 偏差断言）。

复用 test-camera-capture.py 的 Digest HTTP 替身并记录授权 GET 时间戳，
P50 偏差/GET 比率/帧比率三判据；--baseline 时判据阈值取自 recorded.json。
判读：退出码 0=过。
用法：python baseline_capture_cadence.py --dll-dir <release> --runtime-dir <ffmpeg bin>
      [--baseline recorded.json] [--output result.json]
"""
import argparse
import ctypes as c
import importlib.util
import json
from http.server import ThreadingHTTPServer
import platform
import sys
import threading
import time
from pathlib import Path
from statistics import median

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "sdk" / "bindings" / "python"))
import amrt_sdk as a  # noqa: E402

spec = importlib.util.spec_from_file_location("capture_fixture", ROOT / "scripts" / "sdk" / "test-camera-capture.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)

DEFAULTS = {
    "target_ms": 100,
    "window_s": 5,
    "warmup_s": 2,
    "p50_deviation_pct_max": 10.0,
    "gets_ratio_min": 0.8,
    "frames_ratio_min": 0.7,
}


class TimedFixture(fixture.Fixture):
    stamps = []

    def do_GET(self):
        if self.headers.get("Authorization", "").startswith("Digest "):
            with fixture.Fixture.lock:
                TimedFixture.stamps.append(time.perf_counter())
        fixture.Fixture.do_GET(self)


def reset_stamps():
    with fixture.Fixture.lock:
        TimedFixture.stamps = []


def checked(lib, code, stage=""):
    if code != 0:
        name = lib.amc_status_name(code) if hasattr(lib, "amc_status_name") else None
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


def run_window(lib, ctx, port, thresholds):
    target = thresholds["target_ms"]
    desc = sized(a.am_capture_continuous_desc_t)
    desc.target_interval_ms = target
    desc.request_timeout_ms = 1000
    desc.max_inflight, desc.queue_max_items, desc.queue_max_bytes = 1, 4, 32 << 20
    desc.mode = a.AM_SUB_PULL
    source = fixture.open_source(lib, ctx, port)
    sub = c.POINTER(a.amc_capture_sub_t)()
    checked(lib, lib.amc_capture_continuous_start(source, c.byref(desc), c.byref(sub)), "continuous_start")
    deadline = time.monotonic() + thresholds["warmup_s"]
    while time.monotonic() < deadline:
        pull_or_skip(lib, sub)
    reset_stamps()
    window = thresholds["window_s"]
    started = time.monotonic()
    frames = 0
    while time.monotonic() - started < window:
        frames += pull_or_skip(lib, sub)
    gets = len(TimedFixture.stamps)
    checked(lib, lib.amc_object_request_close(sub), "sub_request_close")
    wait_closed(lib, sub)
    fixture.close_object(lib, source)
    intervals = [y - x for x, y in zip(TimedFixture.stamps, TimedFixture.stamps[1:])]
    return target, gets, frames, intervals


def pull_or_skip(lib, sub):
    out = c.POINTER(a.am_captured_image_t)()
    code = lib.amc_capture_sub_pull(sub, 20, c.byref(out))
    if code in (a.AM_ERR_SDK_TRY_AGAIN, a.AM_ERR_SDK_DEADLINE_EXCEEDED):
        return 0
    checked(lib, code, "sub_pull")
    checked(lib, lib.amc_image_release(out), "image_release")
    return 1


def wait_closed(lib, obj):
    state = c.c_uint32()
    until = time.monotonic() + 5
    while True:
        checked(lib, lib.amc_object_query_state(obj, c.byref(state)), "query_state")
        if state.value == a.AM_LIFECYCLE_CLOSED:
            checked(lib, lib.amc_object_destroy(obj), "destroy")
            return
        assert time.monotonic() < until, f"close stuck at {state.value}"
        time.sleep(0.005)


def evaluate(target, gets, frames, intervals, thresholds):
    expected = thresholds["window_s"] * 1000.0 / target
    p50 = median(intervals) * 1000
    deviation_pct = abs(p50 - target) / target * 100
    gets_ratio = gets / expected
    frames_ratio = frames / expected
    assert deviation_pct <= thresholds["p50_deviation_pct_max"], \
        f"GET 间隔 p50={p50:.1f}ms 偏差 {deviation_pct:.1f}% 超过 {thresholds['p50_deviation_pct_max']}%"
    assert gets_ratio >= thresholds["gets_ratio_min"], \
        f"GET 比率 {gets_ratio:.2f} 低于 {thresholds['gets_ratio_min']} (gets={gets} expected~{expected:.0f})"
    assert frames_ratio >= thresholds["frames_ratio_min"], \
        f"帧比率 {frames_ratio:.2f} 低于 {thresholds['frames_ratio_min']} (frames={frames})"
    return {
        "target_ms": target,
        "window_s": thresholds["window_s"],
        "gets": gets,
        "frames": frames,
        "expected": round(expected, 1),
        "p50_interval_ms": round(p50, 2),
        "p95_interval_ms": round(sorted(intervals)[max(0, int(len(intervals) * 0.95) - 1)] * 1000, 2),
        "p50_deviation_pct": round(deviation_pct, 2),
        "gets_ratio": round(gets_ratio, 3),
        "frames_ratio": round(frames_ratio, 3),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, default=None)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    stage = "baseline-load"
    try:
        thresholds = dict(DEFAULTS)
        if args.baseline:
            data = json.loads(args.baseline.read_text(encoding="utf-8"))
            thresholds.update(data.get("cadence", {}))
        stage = "init"
        lib, ctx = init_context(args.dll_dir, args.runtime_dir)
        server = None
        result = {"ok": True, "schema": "framebraid.baseline-cadence/1", "os": platform.platform()}
        try:
            stage = "server"
            server = ThreadingHTTPServer(("127.0.0.1", 0), TimedFixture)
            server.daemon_threads = True
            threading.Thread(target=server.serve_forever, daemon=True).start()
            stage = "window"
            target, gets, frames, intervals = run_window(lib, ctx, server.server_address[1], thresholds)
            row = evaluate(target, gets, frames, intervals, thresholds)
            result["cadence"] = row
            print(f"[cadence] target={row['target_ms']}ms p50={row['p50_interval_ms']}ms "
                  f"deviation={row['p50_deviation_pct']}% gets_ratio={row['gets_ratio']} "
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