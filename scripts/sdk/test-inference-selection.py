"""有序选择C ABI、Python封装、真实CPU Run及失败/缓存/拥有权；不连接设备。"""
import argparse
import ctypes as c
import hashlib
import importlib.util
import json
from pathlib import Path
import statistics
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
import amrt_sdk as a
from safe_sdk import VisionContext, SdkError, sized

spec = importlib.util.spec_from_file_location("tensor_test", Path(__file__).with_name("test-tensors.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)


def check(code, expected=0):
    assert code == expected, (code, expected)


def report(lib, owner, attempt=None):
    fn = lib.amv_get_model_selection_report if attempt is None else lib.amv_get_selection_attempt_report
    args = (owner,) if attempt is None else (owner, attempt)
    required = c.c_uint64()
    check(fn(*args, None, 0, c.byref(required)))
    assert 1 < required.value <= 8192
    short = c.create_string_buffer(b"x", 1)
    check(fn(*args, short, 1, c.byref(required)), a.AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
    assert short.raw == b"\0"
    buffer = c.create_string_buffer(8208)
    c.memset(c.addressof(buffer) + 8192, 0x79, 16)
    check(fn(*args, buffer, 8192, c.byref(required)))
    assert buffer.raw[8192:] == b"y" * 16
    value = json.loads(buffer.value)
    assert value["schema"] == "framebraid.inference-selection/1"
    return value


def load(lib, ctx, path, candidates, fallback=True, memory=False, expected=0):
    values = (a.am_backend_candidate_t * len(candidates))(*(a.am_backend_candidate_t(*p) for p in candidates))
    policy = sized(a.am_backend_selection_t)
    policy.candidate_count, policy.candidates = len(values), values
    policy.allow_fallback, policy.budget_ms = int(fallback), 5000
    desc = sized(a.am_model_memory_desc_t if memory else a.am_model_desc_t)
    desc.load_mode = a.AM_LOAD_RAW
    model, attempt = c.POINTER(a.amv_model_t)(), c.c_uint64()
    if memory:
        payload = path.read_bytes()
        storage = (c.c_ubyte * len(payload)).from_buffer_copy(payload)
        desc.bytes, desc.size = storage, len(storage)
        code = lib.amv_model_load_memory_selected(ctx, c.byref(desc), c.byref(policy), 2, c.byref(model), c.byref(attempt))
        c.memset(storage, 0, len(storage))
    else:
        desc.path_utf8, desc.intra_threads = str(path.resolve()).encode(), 2
        code = lib.amv_model_load_selected(ctx, c.byref(desc), c.byref(policy), c.byref(model), c.byref(attempt))
    check(code, expected)
    assert bool(model) == (expected == 0)
    return model, attempt.value


def infer(lib, ctx, model, asynchronous=False):
    value = t.Tensor("images", a.AM_DTYPE_F32, [1, 3, 2, 2], bytes(48))
    return t.invoke(lib, ctx, model, [value], asynchronous)


def main(args):
    output = args.output.resolve()
    assert output.is_relative_to((ROOT / ".local").resolve())
    output.mkdir(parents=True, exist_ok=True)
    dll = args.dll_dir.resolve() / "am_vision.dll"
    path = args.sample_dir.resolve() / "image.onnx"
    lib = c.CDLL(str(dll))
    a.bind(lib, "amv_", require_all=True)
    ctx = t.base.initialize(lib, args.runtime_dir)
    models = []
    comparisons, snapshots = [], []
    try:
        cap = sized(a.amv_capability_t)
        check(lib.amv_query_capability(ctx, c.byref(cap)))
        assert cap.feature_bits & a.AMV_CAP_ORDERED_SELECTION
        # 每轮交错旧入口与新入口；同CPU、输入、ORT和线程，只比较接口开销。
        for round_id in range(3):
            for selected in (False, True):
                start = time.perf_counter_ns()
                if selected:
                    model, attempt = load(lib, ctx, path, [(0, 0)])
                    assert attempt
                else:
                    model = t.load(lib, ctx, path)
                models.append(model)
                load_ms = (time.perf_counter_ns() - start) / 1e6
                latencies = []
                for i in range(23):
                    at = time.perf_counter_ns()
                    result, rid = infer(lib, ctx, model, i % 2 == 1)
                    elapsed = (time.perf_counter_ns() - at) / 1e6
                    actual = t.output(result)
                    assert len(actual) == 1 and next(iter(actual.values())) == (a.AM_DTYPE_F32, [1, 3, 2, 2], bytes(48)), actual
                    assert result.contents.execution.device_utf8 == b"CPU"
                    t.release(lib, ctx, result, rid)
                    if i >= 3:
                        latencies.append(elapsed)
                snapshot = report(lib, model)
                if selected:
                    assert snapshot["selection"]["native_runs"] == 23
                    assert snapshot["selection"]["delivered_runs"] == 23
                    assert snapshot["selection"]["selected"] == {"backend": 0, "device_id": 0}
                    snapshots.append(snapshot)
                else:
                    assert snapshot["state"] == "legacy_model" and snapshot["selection"] is None
                comparisons.append({"round": round_id, "selected": selected, "load_ms": load_ms,
                                    "latency_ms": latencies})
                t.base.close_model(lib, models.pop())
        # CPU版ORT显式缺EP控制；不是CUDA/OpenVINO实机正向验收。
        for backend in (1, 2):
            _, failed = load(lib, ctx, path, [(backend, 0), (0, 0)], False,
                             expected=a.AM_ERR_VISION_BACKEND_STRICT_UNAVAILABLE)
            failure = report(lib, ctx, failed)
            assert failure["state"] == "failed" and len(failure["selection"]["attempts"]) == 1
            model, fallback_id = load(lib, ctx, path, [(backend, 0), (0, 0)], True, memory=True)
            models.append(model)
            fallback = report(lib, model)
            assert len(fallback["selection"]["attempts"]) == 2
            assert fallback["selection"]["selected"]["backend"] == 0
            check(lib.amv_get_selection_attempt_report(ctx, failed, None, 0, c.byref(c.c_uint64())),
                  a.AM_ERR_SDK_RESULT_EXPIRED)
            _, latest = load(lib, ctx, path, [(backend, 0)], True,
                             expected=a.AM_ERR_VISION_BACKEND_STRICT_UNAVAILABLE)
            assert report(lib, model)["attempt_id"] == fallback_id
            assert report(lib, ctx, latest)["selection"]["selected"] is None
            result, rid = infer(lib, ctx, model)
            assert result.contents.execution.fallback_reason_utf8
            check(lib.amv_result_ack(ctx, rid))
            # 结果拥有设备/失败字符串，模型关闭后仍然有效。
            t.base.close_model(lib, models.pop())
            assert result.contents.execution.device_utf8 == b"CPU"
            assert result.contents.execution.fallback_reason_utf8
            check(lib.amv_result_release(result))
        for candidates in ([], [(0, 0), (0, 0)], [(0, 1)], [(3, 0)], [(2, 0x80000000)]):
            _, attempt = load(lib, ctx, path, candidates, expected=a.AM_ERR_SDK_INVALID_ARGUMENT)
            assert attempt == 0
        model, attempt = load(lib, ctx, path, [(0, 0)])
        models.append(model)
        check(lib.amv_probe_inference_hardware(ctx, 1500))
        hw_buffer, hw_required = c.create_string_buffer(65536), c.c_uint64()
        check(lib.amv_get_inference_hardware_report(ctx, hw_buffer, len(hw_buffer), c.byref(hw_required)))
        assert json.loads(hw_buffer.value)["observations"]["executed_mask"] & 1
        timings = []
        for _ in range(100):
            at = time.perf_counter_ns()
            assert report(lib, model)["attempt_id"] == attempt
            timings.append((time.perf_counter_ns() - at) / 1e6)
        overlap = (c.c_uint64 * 1024)()
        check(lib.amv_get_model_selection_report(model, c.cast(overlap, c.c_char_p), 8192, overlap),
              a.AM_ERR_SDK_INVALID_ARGUMENT)
        check(lib.amv_model_close(model, 1000, c.byref(c.c_uint32())))
        assert report(lib, model)["state"] == "closed"
        check(lib.amv_object_destroy(models.pop()))
    finally:
        for model in models:
            t.base.close_model(lib, model)
        check(lib.amv_shutdown(ctx, 1000, c.byref(c.c_uint32())))
        assert report(lib, ctx, 1)["state"] == "closed"
        check(lib.amv_object_destroy(ctx))
    with VisionContext(dll, [args.runtime_dir]) as safe:
        with safe.load_model_selected(path, [(0, 0)]) as model:
            assert model.get_selection_report()["state"] == "ready"
            assert safe.get_selection_attempt_report(model.selection_attempt_id)["state"] == "ready"
        with safe.load_model_memory_selected(path.read_bytes(), [(2, 0), (0, 0)]) as model:
            assert model.get_selection_report()["selection"]["selected"]["backend"] == 0
        try:
            safe.load_model_selected(path, [(2, 0)], allow_fallback=False)
            raise AssertionError("strict load unexpectedly succeeded")
        except SdkError as error:
            assert error.attempt_id and safe.get_selection_attempt_report(error.attempt_id)["state"] == "failed"
    small = t.base.initialize(lib, args.runtime_dir)
    try:
        low = sized(a.am_queue_policy_t)
        check(lib.amv_get_queue_policy(small, a.AM_Q_CONTROL, c.byref(low)))
        low.max_bytes = 32
        check(lib.amv_set_queue_policy(small, a.AM_Q_CONTROL, c.byref(low)))
        _, attempt = load(lib, small, path, [(0, 0)], expected=a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
        assert attempt == 0
    finally:
        check(lib.amv_shutdown(small, 1000, c.byref(c.c_uint32())))
        check(lib.amv_object_destroy(small))
    summary = {"dll_sha256": hashlib.sha256(dll.read_bytes()).hexdigest(),
               "model_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
               "comparisons": comparisons, "snapshots": snapshots,
               "query_three_calls_p50_ms": statistics.median(timings),
               "query_three_calls_p95_ms": sorted(timings)[94],
               "functional": "passed", "accelerated_positive_execution": "not tested"}
    if args.pose_model and args.pose_image:
        with VisionContext(dll, [args.runtime_dir]) as safe:
            with safe.load_model_selected(args.pose_model, [(0, 0)],
                                          profile="pose-yolo26-e2e-coco17-v1", budget_ms=5000) as model:
                responses, concurrent_reads = [], []
                image = args.pose_image.read_bytes()
                for _ in range(6):
                    at = time.perf_counter_ns()
                    rid = model.infer_encoded(image, png=True, asynchronous=True)
                    for _ in range(25):
                        start = time.perf_counter_ns()
                        report(lib, model.handle)
                        concurrent_reads.append((time.perf_counter_ns() - start) / 1e6)
                    safe.wait(rid)
                    with safe.fetch(rid) as result:
                        safe.ack(rid)
                        assert result.handle.contents.execution.actual == a.AM_BACKEND_CPU
                    responses.append((time.perf_counter_ns() - at) / 1e6)
                pose = model.get_selection_report()
                assert pose["selection"]["native_runs"] == pose["selection"]["delivered_runs"] == 6
                summary["pose"] = {"model_sha256": hashlib.sha256(args.pose_model.read_bytes()).hexdigest(),
                                   "responses_ms": responses, "report": pose,
                                   "query_during_run_p95_ms": sorted(concurrent_reads)[142]}
    (output / "result.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in summary.items() if k not in ("comparisons", "snapshots")}))


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--dll-dir", type=Path, required=True)
    p.add_argument("--runtime-dir", type=Path, required=True)
    p.add_argument("--sample-dir", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--pose-model", type=Path)
    p.add_argument("--pose-image", type=Path)
    main(p.parse_args())
