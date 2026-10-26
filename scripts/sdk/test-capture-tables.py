"""真实Media DLL索引预算/合并失败恢复；仅本机Digest合成图服务。"""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import sys
import threading
import time

sys.dont_write_bytecode = True
from camera_recovery_fixture import module
from local_paths import local_path

t = module("test-camera-capture.py")
a, size, check = t.a, t.size, t.check


def context(lib):
    params = size(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    ctx = c.POINTER(a.amc_context_t)()
    check(lib.amc_init(c.byref(params), c.byref(ctx)))
    return ctx


def control(lib, ctx, maximum):
    policy = size(a.am_queue_policy_t)
    check(lib.amc_get_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
    policy.max_bytes = maximum
    check(lib.amc_set_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))


def fetch(lib, ctx, rid, expected=0):
    until = time.monotonic() + 3
    while True:
        image = c.POINTER(a.am_captured_image_t)()
        code = lib.amc_captured_image_fetch(ctx, rid, c.byref(image))
        if code != a.AM_ERR_SDK_TRY_AGAIN:
            check(code, expected)
            if expected == 0:
                assert c.string_at(image.contents.encoded, image.contents.encoded_size) == t.Fixture.image
                check(lib.amc_captured_image_ack(ctx, rid))
                assert c.string_at(image.contents.encoded, image.contents.encoded_size) == t.Fixture.image
                check(lib.amc_image_release(image))
            else:
                assert not image
                check(lib.amc_captured_image_ack(ctx, rid))
            return
        assert time.monotonic() < until
        time.sleep(.001)


def request(lib, source):
    req = size(a.am_capture_request_t)
    req.deadline_ms = 1000
    rid = c.c_uint64()
    return lib.amc_capture_async(source, c.byref(req), c.byref(rid)), rid.value


def request_budgets(lib, port, baseline):
    rows = []
    for budget in (72, 80, 65536):
        ctx = context(lib)
        source = None
        try:
            source = t.open_source(lib, ctx, port)
            if budget == 65536:
                limits = size(a.am_limits_t)
                check(lib.amc_get_limits(ctx,c.byref(limits)))
                limits.max_sources = 0
                check(lib.amc_set_limits(ctx,c.byref(limits)))
            control(lib, ctx, budget)
            before = t.Fixture.requests
            code, rid = request(lib, source)
            rejected = budget == 72 and not baseline
            check(code, a.AM_ERR_SDK_RESOURCE_EXHAUSTED if rejected else 0)
            assert bool(rid) != rejected
            if rid:
                assert rid & 0xffffffffffff == 1
                if baseline and budget < 65536:
                    # 极小额度无法另分配图片输出包装或query快照，仅观察准入/HTTP和终态。
                    until = time.monotonic()+3
                    while True:
                        terminal = lib.amc_captured_image_ack(ctx,rid)
                        if terminal != a.AM_ERR_SDK_TRY_AGAIN:
                            check(terminal)
                            break
                        assert time.monotonic()<until
                        time.sleep(.001)
                else:
                    fetch(lib, ctx, rid, a.AM_ERR_SDK_RESOURCE_EXHAUSTED if budget == 80 else 0)
            after = t.Fixture.requests
            if budget < 65536 and not baseline:
                assert after == before, (budget, before, after)
            if budget == 65536:
                code, second = request(lib, source)
                check(code)
                assert second == rid + 1
                fetch(lib, ctx, second)
            rows.append(dict(control_bytes=budget, submit_status=code, request_id=rid,
                             physical_http=after-before,
                             terminal_status=(None if baseline else (8 if budget==80 else 0)) if rid else None,
                             existing_source_survives_new_source_limit=(budget==65536)))
        finally:
            if source:
                t.close_object(lib, source)
            t.shared.close(lib, ctx, True)
    return rows


def subscription(lib, source, interval=60000):
    desc = size(a.am_capture_continuous_desc_t)
    desc.output_kind, desc.mode = a.AM_CAP_ENCODED_JPEG, a.AM_SUB_PULL
    desc.target_interval_ms, desc.request_timeout_ms = interval, 500
    desc.queue_max_items, desc.queue_max_bytes = 1, 1024*1024
    sub = c.POINTER(a.amc_capture_sub_t)()
    check(lib.amc_capture_continuous_start(source, c.byref(desc), c.byref(sub)))
    return sub


def cache_failure(lib, port, maximum):
    ctx, source, sub = context(lib), None, None
    try:
        source = t.open_source(lib, ctx, port)
        control(lib, ctx, maximum)
        before = t.Fixture.requests
        sub = subscription(lib, source, 30)
        until = time.monotonic() + 3
        while True:
            value = size(a.am_capture_stats_t)
            check(lib.amc_capture_sub_get_stats(sub, c.byref(value)))
            if value.consecutive_failures >= 2 and t.Fixture.requests >= before+2:
                break
            assert time.monotonic() < until, (value.consecutive_failures, t.Fixture.requests-before)
            time.sleep(.01)
        attempts, failures = t.Fixture.requests-before, value.consecutive_failures
        image = c.POINTER(a.am_captured_image_t)()
        code = lib.amc_capture_sub_pull(sub, 0, c.byref(image))
        assert code in (a.AM_ERR_SDK_TRY_AGAIN, a.AM_ERR_SDK_DEADLINE_EXCEEDED) and not image
        t.close_object(lib, sub)
        sub = None
        image = t.captured(lib, source)
        assert c.string_at(image.contents.encoded, image.contents.encoded_size) == t.Fixture.image
        check(lib.amc_image_release(image))
        return dict(cache_failures=failures, actual_http_attempts=attempts,
                    no_stuck_flight=True, healthy_single_capture_after_stop=True)
    finally:
        if sub:
            t.close_object(lib, sub)
        if source:
            t.close_object(lib, source)
        t.shared.close(lib, ctx, True)


def closed_handles(lib, port):
    ctx, source, handles = context(lib), None, []
    try:
        source = t.open_source(lib, ctx, port)
        for _ in range(20):
            sub = subscription(lib, source)
            handles.append(sub)
            state = c.c_uint32()
            check(lib.amc_capture_continuous_stop(sub, 1000, c.byref(state)))
            assert state.value == a.AM_LIFECYCLE_CLOSED
        return dict(closed_handles_kept=20, registration_reused=True)
    finally:
        for sub in handles:
            t.close_object(lib, sub)
        if source:
            t.close_object(lib, source)
        t.shared.close(lib, ctx, True)


def main(args):
    out = local_path(args.output.resolve(), "evidence", fresh=True)
    out.mkdir(parents=True)
    target = dict(metric="MET-2 M-RECOVER/S5", control_bytes=[72,80,65536],
                  cache_control_bytes=args.cache_control_bytes,
                  scope="Windows x64 measured private layouts; encoded JPEG, no FFmpeg runtime; not public ABI size constants",
                  baseline=args.observe_baseline)
    (out/"target-card.json").write_bytes((json.dumps(target,indent=2)+"\n").encode())
    dll = args.dll_dir.resolve()/"am_camera.dll"
    lib = c.CDLL(str(dll))
    a.bind(lib,"amc_",require_all=True)
    server = t.ThreadingHTTPServer(("127.0.0.1",0),t.Fixture)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever,daemon=True)
    thread.start()
    try:
        result = dict(dll_sha256=hashlib.sha256(dll.read_bytes()).hexdigest(),
                      budgets=request_budgets(lib,server.server_address[1],args.observe_baseline))
        if not args.observe_baseline:
            result["cache"] = cache_failure(lib,server.server_address[1],args.cache_control_bytes)
            result["closed_handles"] = closed_handles(lib,server.server_address[1])
        (out/"result.json").write_bytes((json.dumps(result,indent=2)+"\n").encode())
        print(json.dumps(result))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(2)
        assert not thread.is_alive()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ("dll-dir","output"):
        parser.add_argument("--"+name,type=Path,required=True)
    parser.add_argument("--observe-baseline",action="store_true")
    parser.add_argument("--cache-control-bytes",type=int,default=1360)
    main(parser.parse_args())
