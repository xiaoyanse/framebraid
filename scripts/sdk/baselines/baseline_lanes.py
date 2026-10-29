"""多路线性度基线：1/2/4 路并发抓图吞吐，线性度下限断言。

N 路各自对独立本地 HTTP 替身连续抓图（独立 GET 流），每路独立消费线程
拉帧计数；断言 2 路 ≥1.5x、4 路 ≥3.0x 于单路吞吐（比率判据，非绝对 fps）。
判读：退出码 0=过。
用法：python baseline_lanes.py --dll-dir <release> --runtime-dir <ffmpeg bin>
      [--baseline recorded.json] [--output result.json]
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

LANES = (1, 2, 4)
WARMUP_S = 1.0
WINDOW_S = 4.0
DEFAULTS = {
    "lanes": list(LANES),
    "window_s": WINDOW_S,
    "ratio2_min": 1.5,
    "ratio4_min": 3.0,
}


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


def start_sub(lib, source):
    desc = sized(a.am_capture_continuous_desc_t)
    desc.target_interval_ms = 20
    desc.request_timeout_ms = 1000
    desc.max_inflight, desc.queue_max_items, desc.queue_max_bytes = 1, 4, 32 << 20
    desc.mode = a.AM_SUB_PULL
    sub = c.POINTER(a.amc_capture_sub_t)()
    checked(lib, lib.amc_capture_continuous_start(source, c.byref(desc), c.byref(sub)), "continuous_start")
    return sub


def pull_worker(lib, sub, stop, counts, lane):
    while not stop.is_set():
        out = c.POINTER(a.am_captured_image_t)()
        code = lib.amc_capture_sub_pull(sub, 20, c.byref(out))
        if code in (a.AM_ERR_SDK_TRY_AGAIN, a.AM_ERR_SDK_DEADLINE_EXCEEDED):
            continue
        checked(lib, code, "sub_pull")
        checked(lib, lib.amc_image_release(out), "image_release")
        counts[lane] += 1


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


def run_cell(lib, sources, lanes, thresholds):
    subs = [start_sub(lib, sources[i]) for i in range(lanes)]
    stop = threading.Event()
    counts = [0] * lanes
    threads = [threading.Thread(target=pull_worker, args=(lib, sub, stop, counts, i), daemon=True)
               for i, sub in enumerate(subs)]
    for thread in threads:
        thread.start()
    time.sleep(WARMUP_S)
    counts[:] = [0] * lanes
    started = time.monotonic()
    deadline = started + thresholds["window_s"]
    while time.monotonic() < deadline:
        time.sleep(0.05)
    stop.set()
    for thread in threads:
        thread.join(2)
    elapsed = time.monotonic() - started
    for sub in subs:
        checked(lib, lib.amc_object_request_close(sub), "sub_request_close")
    for sub in subs:
        wait_closed(lib, sub)
    return counts, elapsed


def evaluate(results, thresholds):
    totals = {}
    per_lane = {}
    for lanes, (counts, elapsed) in results.items():
        totals[lanes] = sum(counts) / elapsed
        per_lane[lanes] = [round(n / elapsed, 1) for n in counts]
    ratio2 = totals[2] / totals[1]
    ratio4 = totals[4] / totals[1]
    assert ratio2 >= thresholds["ratio2_min"], \
        f"2 路吞吐比率 {ratio2:.2f} 低于下限 {thresholds['ratio2_min']}"
    assert ratio4 >= thresholds["ratio4_min"], \
        f"4 路吞吐比率 {ratio4:.2f} 低于下限 {thresholds['ratio4_min']}"
    return {
        "window_s": thresholds["window_s"],
        "total_fps": {str(k): round(v, 1) for k, v in totals.items()},
        "per_lane_fps": {str(k): v for k, v in per_lane.items()},
        "ratio2": round(ratio2, 2),
        "ratio4": round(ratio4, 2),
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
            thresholds.update(data.get("lanes", {}))
        stage = "init"
        lib, ctx = init_context(args.dll_dir, args.runtime_dir)
        servers = []
        try:
            for _ in range(max(LANES)):
                server = ThreadingHTTPServer(("127.0.0.1", 0), fixture.Fixture)
                server.daemon_threads = True
                threading.Thread(target=server.serve_forever, daemon=True).start()
                servers.append(server)
            stage = "sources"
            sources = [fixture.open_source(lib, ctx, s.server_address[1]) for s in servers]
            try:
                results = {}
                for lanes in LANES:
                    stage = f"lanes-{lanes}"
                    results[lanes] = run_cell(lib, sources, lanes, thresholds)
                    counts, elapsed = results[lanes]
                    print(f"[lanes] {lanes}路 total={sum(counts) / elapsed:.1f}fps "
                          f"per_lane={[round(n / elapsed, 1) for n in counts]}", flush=True)
                row = evaluate(results, thresholds)
                result = {"ok": True, "schema": "framebraid.baseline-lanes/1",
                          "os": platform.platform(), "lanes": row}
                if args.output:
                    args.output.parent.mkdir(parents=True, exist_ok=True)
                    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=1), encoding="utf-8")
                print(json.dumps(result, ensure_ascii=False))
            finally:
                for source in sources:
                    fixture.close_object(lib, source)
        finally:
            for server in servers:
                server.shutdown()
                server.server_close()
            fixture.shared.close(lib, ctx, True)
    except Exception as error:
        print(json.dumps({"ok": False, "stage": stage, "error": repr(error)}, ensure_ascii=False))
        sys.exit(1)


if __name__ == "__main__":
    main()