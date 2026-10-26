"""ISAPI变频订阅回收：活消费者不退出时，历史采样频率不能积累缓存。"""
import argparse
import ctypes as c
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("capture", Path(__file__).with_name("test-camera-capture.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
a, sized, check = t.a, t.size, t.check
from local_paths import local_path


def subscribe(lib, source, interval):
    desc = sized(a.am_capture_continuous_desc_t)
    desc.output_kind, desc.mode, desc.target_interval_ms = a.AM_CAP_ENCODED_JPEG, a.AM_SUB_PULL, interval
    desc.queue_max_items, desc.queue_max_bytes, desc.request_timeout_ms = 1, 1024 * 1024, 500
    sub = c.POINTER(a.amc_capture_sub_t)()
    check(lib.amc_capture_continuous_start(source, c.byref(desc), c.byref(sub)))
    return sub


def first(lib, sub):
    until = time.perf_counter() + 1
    while True:
        image = c.POINTER(a.am_captured_image_t)()
        code = lib.amc_capture_sub_pull(sub, 50, c.byref(image))
        if code == 0:
            try:
                assert c.string_at(image.contents.encoded, image.contents.encoded_size) == t.Fixture.image
            finally:
                check(lib.amc_image_release(image))
            return 0
        if code not in (a.AM_ERR_SDK_TRY_AGAIN, a.AM_ERR_SDK_DEADLINE_EXCEEDED):
            assert not image
            return code
        value = sized(a.am_capture_stats_t)
        check(lib.amc_capture_sub_get_stats(sub, c.byref(value)))
        if value.consecutive_failures or time.perf_counter() >= until:
            return code


def main(args):
    out = local_path(args.output.resolve(), "evidence", fresh=True)
    out.mkdir(parents=True)
    target = {"source": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
              "metric": "MET-2 M-RECOVER S5", "rounds": 20, "healthy_slow_interval_ms": 60000,
              "changing_intervals_ms": [900 - 40 * i for i in range(20)],
              "cached_items_after_each_stop_max": 1, "all_rounds_expected": args.require_complete,
              "frame_budget_bytes": 32 * 1024 * 1024,
              "scope": "local Digest HTTP synthetic JPEG; logical cache/items and admission, not RSS or throughput"}
    (out / "target-card.json").write_text(json.dumps(target, indent=2), encoding="utf-8")
    lib = c.CDLL(str(args.dll_dir.resolve() / "am_camera.dll"))
    a.bind(lib, "amc_", require_all=True)
    init = sized(a.amc_init_params_t)
    init.abi_version = a.AM_ABI_VERSION
    ctx, source, permanent = c.POINTER(a.amc_context_t)(), None, None
    check(lib.amc_init(c.byref(init), c.byref(ctx)))
    limits = sized(a.am_limits_t)
    check(lib.amc_get_limits(ctx, c.byref(limits)))
    limits.frame_budget_bytes = target["frame_budget_bytes"]
    check(lib.amc_set_limits(ctx, c.byref(limits)))
    server = t.ThreadingHTTPServer(("127.0.0.1", 0), t.Fixture)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    result = {"rounds": [], "scope": target["scope"]}
    try:
        source = t.open_source(lib, ctx, server.server_address[1])
        permanent = subscribe(lib, source, 60000)
        check(first(lib, permanent))
        result["initial"] = t.stats(lib, ctx)
        for interval in target["changing_intervals_ms"]:
            sub = subscribe(lib, source, interval)
            try:
                code = first(lib, sub)
            finally:
                t.close_object(lib, sub)
            result["rounds"].append({"interval_ms": interval, "first_code": code, "after_stop_stats": t.stats(lib, ctx)})
            (out / "progress.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
            if code:
                break
        image = c.POINTER(a.am_captured_image_t)()
        req = sized(a.am_capture_request_t)
        req.output_kind, req.deadline_ms = a.AM_CAP_ENCODED_JPEG, 500
        result["healthy_single_code"] = lib.amc_capture_sync(source, c.byref(req), c.byref(image))
        if image:
            check(lib.amc_image_release(image))
        t.close_object(lib, permanent)
        permanent = None
        result["after_all_subscriptions_stopped"] = t.stats(lib, ctx)
        assert result["after_all_subscriptions_stopped"] == (0, 0, 0)
    finally:
        if permanent:
            t.close_object(lib, permanent)
        if source:
            t.close_object(lib, source)
        t.shared.close(lib, ctx, True)
        server.shutdown()
        server.server_close()
        worker.join(2)
        assert not worker.is_alive()
    result["identity"] = {"dll_sha256": hashlib.sha256((args.dll_dir / "am_camera.dll").read_bytes()).hexdigest(),
                          "jpeg_sha256": hashlib.sha256(t.Fixture.image).hexdigest(), "jpeg_bytes": len(t.Fixture.image)}
    result["successful_rounds"] = sum(r["first_code"] == 0 for r in result["rounds"])
    result["cache_items_peak_after_stop"] = max(r["after_stop_stats"][0] for r in result["rounds"])
    result["http_requests"] = t.Fixture.requests
    result["observer_finished"] = True
    (out / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps({k:result[k] for k in ["successful_rounds", "cache_items_peak_after_stop", "healthy_single_code", "identity"]}))
    if args.require_complete:
        assert result["successful_rounds"] == 20
        assert result["cache_items_peak_after_stop"] <= 1
        assert result["healthy_single_code"] == 0


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--dll-dir", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--require-complete", action="store_true")
    main(p.parse_args())
