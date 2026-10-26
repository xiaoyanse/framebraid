"""连续抓图控制存储短预算；仅本机Digest合成服务，不读取现场设备。"""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import sys
import threading

sys.dont_write_bytecode = True
from camera_recovery_fixture import module
from local_paths import local_path

t = module("test-camera-capture.py")
a, check, size = t.a, t.check, t.size


def main(args):
    out = local_path(args.output.resolve(), "evidence", fresh=True)
    out.mkdir(parents=True)
    target = {"metric": "MET-2 M-RECOVER/S5", "control_bytes": 512,
              "queue_items": 3, "expectation": "reject before consumer registration/HTTP",
              "observe_baseline": args.observe_baseline}
    (out / "target-card.json").write_bytes((json.dumps(target, indent=2) + "\n").encode())
    dll = (args.dll_dir / "am_camera.dll").resolve()
    lib = c.CDLL(str(dll))
    a.bind(lib, "amc_")
    params = size(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    ctx = c.POINTER(a.amc_context_t)()
    check(lib.amc_init(c.byref(params), c.byref(ctx)))
    server = t.ThreadingHTTPServer(("127.0.0.1", 0), t.Fixture)
    server.daemon_threads = True
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    source, subscription = None, c.POINTER(a.amc_capture_sub_t)()
    try:
        source = t.open_source(lib, ctx, server.server_address[1])
        policy = size(a.am_queue_policy_t)
        check(lib.amc_get_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
        policy.max_bytes = 512
        check(lib.amc_set_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
        desc = size(a.am_capture_continuous_desc_t)
        desc.output_kind, desc.mode = a.AM_CAP_ENCODED_JPEG, a.AM_SUB_PULL
        desc.target_interval_ms, desc.request_timeout_ms = 60_000, 500
        desc.queue_max_items, desc.queue_max_bytes = 3, 1024 * 1024
        before = t.Fixture.requests
        code = lib.amc_capture_continuous_start(source, c.byref(desc), c.byref(subscription))
        if not args.observe_baseline:
            check(code, a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
            assert not subscription
        result = {"dll_sha256": hashlib.sha256(dll.read_bytes()).hexdigest(),
                  "start_status": code, "subscription_created": bool(subscription),
                  "http_requests_before": before, "http_requests_after_start": t.Fixture.requests}
        if not args.observe_baseline:
            assert result["http_requests_after_start"] == before
        (out / "result.json").write_bytes((json.dumps(result, indent=2) + "\n").encode())
        print(json.dumps(result))
    finally:
        if subscription:
            t.close_object(lib, subscription)
        if source:
            t.close_object(lib, source)
        t.shared.close(lib, ctx, True)
        server.shutdown()
        server.server_close()
        worker.join(2)
        assert not worker.is_alive()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--observe-baseline", action="store_true")
    main(parser.parse_args())
