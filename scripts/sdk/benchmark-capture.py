"""本机HTTP替身与真实Media DLL的采集基准，不连接现场相机。"""
import argparse
import ctypes as c
import hashlib
from http.server import ThreadingHTTPServer
import importlib.util
import json
from pathlib import Path
import platform
import subprocess
import sys
import threading
import time
from perf_common import SCHEMA, deadline_observation, dll_build_identity, process_resources, summarize, validate, workload_budget

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("capture_fixture", Path(__file__).with_name("test-camera-capture.py"))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
a, check, size = fixture.a, fixture.check, fixture.size
ROOT = Path(__file__).resolve().parents[2]


def measure(lib, ctx, source, width, bursts, slow_ms):
    samples, frames = [], set()
    rejected = 0
    resources_before = process_resources()
    began = time.perf_counter_ns()
    for _ in range(bursts):
        pending = {}
        for _ in range(width):
            req = size(a.am_capture_request_t)
            req.deadline_ms = 1000
            rid = c.c_uint64()
            start = time.perf_counter_ns()
            code = lib.amc_capture_async(source, c.byref(req), c.byref(rid))
            if code in (a.AM_ERR_SDK_QUEUE_FULL, a.AM_ERR_SDK_RESOURCE_EXHAUSTED,
                        a.AM_ERR_CAMERA_CAPTURE_CAPACITY):
                assert not rid.value
                rejected += 1
            else:
                check(code)
                assert rid.value and rid.value not in pending
                pending[rid.value] = start
        if slow_ms:
            time.sleep(slow_ms / 1000)
        until = time.monotonic() + 5
        while pending:
            progressed = False
            for rid, start in list(pending.items()):
                out = c.POINTER(a.am_captured_image_t)()
                code = lib.amc_captured_image_fetch(ctx, rid, c.byref(out))
                if code == a.AM_ERR_SDK_TRY_AGAIN:
                    continue
                check(code)
                end = time.perf_counter_ns()
                try:
                    assert out.contents.request_id == rid
                    assert c.string_at(out.contents.encoded, out.contents.encoded_size) == fixture.Fixture.image
                    meta = out.contents.meta.contents
                    assert meta.clock_mask == a.AM_CLK_RECEIVED and meta.received_at_ms > 0
                    frames.add(meta.frame_id)
                finally:
                    check(lib.amc_captured_image_ack(ctx, rid))
                    check(lib.amc_image_release(out))
                samples.append(end - start)
                del pending[rid]
                progressed = True
            if not progressed:
                time.sleep(.0005)
            if time.monotonic() > until:
                raise TimeoutError("accepted capture did not complete")
    elapsed = time.perf_counter_ns() - began
    assert fixture.stats(lib, ctx) == (0, 0, 0)
    return dict(samples_ns=samples, elapsed_ns=elapsed,
                counts=dict(attempted=width*bursts, delivered=len(samples), rejected=rejected, dropped=0),
                unique_physical_frames=len(frames),
                process_resources=dict(before=resources_before, after=process_resources()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--compatibility-baseline", action="store_true", help="旧r9无工作线程预算导出，仅用于同负载对照")
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to((ROOT / ".local/evidence").resolve()):
        raise SystemExit("use a new output directory below .local/evidence")
    lib = c.CDLL(str((args.dll_dir / "am_camera.dll").resolve()))
    missing = a.bind(lib, "amc_", require_all=not args.compatibility_baseline)
    if args.compatibility_baseline:
        assert set(missing) == {"amc_get_worker_budget", "amc_set_worker_budget"}, missing
    params = size(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    ctx = c.POINTER(a.amc_context_t)()
    check(lib.amc_init(c.byref(params), c.byref(ctx)))
    server = ThreadingHTTPServer(("127.0.0.1", 0), fixture.Fixture)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    source = None
    try:
        source = fixture.open_source(lib, ctx, server.server_address[1])
        version = size(a.am_version_info_t)
        check(lib.amc_get_version_info(c.byref(version)))
        build = dll_build_identity(args.dll_dir / "am_camera.dll", version.package_version_utf8.decode(),
            version.build_target_utf8.decode(), version.runtime_notes_utf8.decode(),
            subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "dirty" if subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT) else "clean")
        limits = size(a.am_limits_t)
        check(lib.amc_get_limits(ctx, c.byref(limits)))
        budget = workload_budget("capture")
        budget["sdk_limits"] = {key: getattr(limits, key) for key in (
            "frame_budget_bytes", "external_hold_max_frames", "external_hold_max_bytes",
            "result_budget_bytes", "result_max_items", "control_max_items")}
        output.mkdir(parents=True)
        measure(lib, ctx, source, 1, 8, 0)
        summaries = {}
        for index in range(1, 4):
            for name, width, delay, slow in (("single", 1, 0, 0), ("within-budget", 2, 0, 0),
                                           ("over-budget-slow", 16, .01, 30)):
                fixture.Fixture.delay = delay
                raw = measure(lib, ctx, source, width, 16, slow)
                report = dict(schema=SCHEMA, build=build,
                    identity=dict(workload=dict(name=name, width=width, bursts=16, http_delay_ms=delay*1000, consumer_delay_ms=slow),
                        input_sha256=hashlib.sha256(fixture.Fixture.image).hexdigest(),
                        model_sha256="not-applicable:capture", runtime_sha256="not-applicable:encoded-http-no-ffmpeg",
                        hardware=dict(os=platform.platform(), cpu=platform.processor()), backend="isapi-loopback-digest-fixture",
                        threads=4, budgets=budget,
                        warmup=8, timing_boundary="Python async capture submit through fetch; includes HTTP, polling and consumer delay; excludes validation/ack/release"),
                    unknown=dict(decode_latency=None, capture_queue_wait_per_request=None), **raw)
                report["deadline_observation"] = deadline_observation(raw["samples_ns"], report["identity"]["budgets"]["result_age_budget_ms"])
                validate(report)
                (output/f"{name}-{index}.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
                summaries[f"{name}-{index}"] = dict(latency=summarize(raw["samples_ns"]), counts=raw["counts"])
        print(json.dumps(dict(output=str(output), summaries=summaries,
                             physical_http_peak=fixture.Fixture.peak, scope="local HTTP fixture, real Media DLL; not a camera/network benchmark"), indent=2))
    finally:
        fixture.Fixture.delay = 0
        if source:
            fixture.close_object(lib, source)
        fixture.shared.close(lib, ctx, True)
        server.shutdown()
        server.server_close()
        thread.join(2)


if __name__ == "__main__":
    main()
