"""真实CPU推理的未取结果、慢通知、共享槽持有隔离；仅合成输入。"""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
from camera_recovery_fixture import module
from local_paths import local_path

t = module("test-tensors.py")
a, base, shared, sized, check = t.a, t.base, t.base.shared, t.size, t.check
PAYLOAD = struct.pack("<12f", *range(12))
EXPECTED = {"pixels": (a.AM_DTYPE_F32, [1, 3, 2, 2], PAYLOAD)}


def terminal(lib, ctx, rid):
    until = time.perf_counter() + 5
    while True:
        p = c.POINTER(a.am_request_snapshot_t)()
        check(lib.amv_request_query(ctx, rid, c.byref(p)))
        try:
            state, code = p.contents.state, p.contents.error.contents.code
        finally:
            check(lib.amv_request_snapshot_release(p))
        if state >= a.AM_REQ_DONE:
            return state, code
        assert time.perf_counter() < until
        time.sleep(0.002)


def submit(lib, model):
    tensor = t.Tensor("images", a.AM_DTYPE_F32, [1, 3, 2, 2], PAYLOAD)
    array, rid = (a.am_tensor_in_t * 1)(tensor.view), c.c_uint64()
    check(lib.amv_infer_tensors_async(model, 1, array, None, c.byref(rid)))
    assert rid.value
    tensor.clear()
    return rid.value


def snapshot(lib, ctx):
    p = c.POINTER(a.am_stats_t)()
    check(lib.amv_get_stats(ctx, None, c.byref(p)))
    try:
        return {n: getattr(p.contents, n) for n in ["queue_items", "queue_bytes", "external_held_items", "external_held_bytes", "accepted", "rejected_backpressure", "infer_executed", "infer_failed", "dropped", "threads_active", "threads_peak"]}
    finally:
        check(lib.amv_stats_release(p))


def healthy(lib, ctx, model, count):
    elapsed = []
    for _ in range(count):
        start = time.perf_counter()
        output, rid = t.invoke(lib, ctx, model, [t.Tensor("images", a.AM_DTYPE_F32, [1, 3, 2, 2], PAYLOAD)])
        try:
            assert t.output(output) == EXPECTED
        finally:
            check(lib.amv_result_ack(ctx, rid))
            check(lib.amv_result_release(output))
        elapsed.append((time.perf_counter() - start) * 1000)
    assert max(elapsed) <= 2000, max(elapsed)
    return {"count": count, "max_ms": max(elapsed), "samples_ms": elapsed, "byte_exact": True}


def results_pressure(lib, runtime, directory):
    slow = fast = None
    models, held, ids = [], c.POINTER(a.amv_result_t)(), []
    try:
        slow, fast = base.initialize(lib, runtime), None
        fast = base.initialize(lib, runtime)
        for ctx in [slow, fast]:
            models.append(t.load(lib, ctx, directory / "image.onnx"))
        policy = sized(a.am_queue_policy_t)
        check(lib.amv_get_queue_policy(slow, a.AM_Q_RESULTS, c.byref(policy)))
        policy.max_items = 2
        check(lib.amv_set_queue_policy(slow, a.AM_Q_RESULTS, c.byref(policy)))
        for _ in range(2):
            rid = submit(lib, models[0])
            ids.append(rid)
            assert terminal(lib, slow, rid) == (a.AM_REQ_DONE, 0)
        saturated = snapshot(lib, slow)
        rid = submit(lib, models[0])
        assert terminal(lib, slow, rid) == (a.AM_REQ_FAILED, a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
        check(lib.amv_result_ack(slow, rid))
        other = healthy(lib, fast, models[1], 50)
        assert snapshot(lib, slow)["queue_items"] == saturated["queue_items"] == 2
        check(lib.amv_result_fetch(slow, ids[0], c.byref(held)))
        for rid in ids:
            check(lib.amv_result_ack(slow, rid))
        ids.clear()
        policy.max_items = 1
        check(lib.amv_set_queue_policy(slow, a.AM_Q_RESULTS, c.byref(policy)))
        held_stats = snapshot(lib, slow)
        assert held_stats["external_held_items"] == 1 and held_stats["external_held_bytes"] > 0
        assert t.output(held) == EXPECTED
        rid = submit(lib, models[0])
        assert terminal(lib, slow, rid) == (a.AM_REQ_FAILED, a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
        check(lib.amv_result_ack(slow, rid))
        other_held = healthy(lib, fast, models[1], 50)
        check(lib.amv_result_release(held))
        held = c.POINTER(a.amv_result_t)()
        resumed = healthy(lib, slow, models[0], 10)
        drained = snapshot(lib, slow)
        assert all(drained[n] == 0 for n in ["queue_items", "queue_bytes", "external_held_items", "external_held_bytes"]), drained
        assert drained["infer_failed"] == 2 and drained["accepted"] == drained["infer_executed"] == 14, drained
        return {"saturated": saturated, "healthy_while_unfetched": other,
                "externally_held": held_stats, "healthy_while_held": other_held,
                "after_release": resumed, "drained": drained,
                "bounded_failed_requests": 2, "scope": "result quota is shared within one Context; healthy other Context has separate result budget"}
    finally:
        if held:
            check(lib.amv_result_release(held))
        for rid in ids:
            check(lib.amv_result_ack(slow, rid))
        for model in models:
            base.close_model(lib, model)
        for ctx in [slow, fast]:
            if ctx:
                shared.close(lib, ctx, False)


def slow_notifications(lib, runtime, directory):
    ctx = base.initialize(lib, runtime)
    models, errors = [], []
    gate, entered = threading.Event(), threading.Event()
    calls = []
    @a.am_result_notify_fn
    def callback(model, rid, state, user):
        try:
            calls.append(rid)
            if not entered.is_set():
                entered.set()
                assert gate.wait(20), "owned result callback gate deadline"
        except Exception as error:
            errors.append(str(error))
    try:
        for _ in range(2):
            models.append(t.load(lib, ctx, directory / "image.onnx"))
        policy = sized(a.am_queue_policy_t)
        check(lib.amv_get_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
        policy.max_items = 16
        check(lib.amv_set_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
        before = snapshot(lib, ctx)
        check(lib.amv_result_notify_callback_set(models[0], callback, None))
        completed = healthy(lib, ctx, models[0], 1)
        assert entered.wait(5)
        more = healthy(lib, ctx, models[0], 50)
        other = healthy(lib, ctx, models[1], 50)
        assert len(calls) == 1 and not gate.is_set()
        gate.set()
        check(lib.amv_result_notify_callback_set(models[0], a.am_result_notify_fn(), None))
        after = snapshot(lib, ctx)
        assert after["dropped"] > before["dropped"], (before, after)
        count = len(calls)
        healthy(lib, ctx, models[0], 5)
        assert len(calls) == count and not errors, errors
        return {"first": completed, "blocked_model_still_executes": more, "other_model_same_context": other,
                "before": before, "after": after, "callback_count_after_unregister": count,
                "notification_overflow_reported": True}
    finally:
        gate.set()
        for model in models:
            base.close_model(lib, model)
        shared.close(lib, ctx, False)


def shared_pressure(camera, lib, runtime, directory):
    cctx, vctx = shared.init(camera), base.initialize(lib, runtime)
    pools, consumers, views, model = [], [], [], None
    data, frame, assoc, pixel = shared.sample()
    try:
        for _ in range(2):
            pool, desc = shared.create(camera, cctx)
            pools.append(pool)
            consumers.append(shared.attach(lib, vctx, desc))
        first = shared.publish(camera, pools[0], pixel)
        view = shared.borrow(lib, consumers[0], first)
        views.append(view)
        frozen = c.string_at(view.plane_data[0], frame.plane_size_bytes[0])
        bridge = sized(a.am_pixel_view_t)
        check(lib.amv_shared_frame_as_pixel_view(consumers[0], c.byref(view), c.byref(bridge)))
        image, pre, params = sized(a.am_image_input_t), sized(a.am_raw_preprocess_t), sized(a.am_infer_params_t)
        image.kind, image.flags, image.view = a.AM_IMG_PIXEL_VIEW, a.AM_INPUT_BORROW_RETAINED, c.pointer(bridge)
        pre.target_w, pre.target_h, pre.resize = 2, 2, 1
        params.confidence_threshold, params.raw_preprocess = 0.25, c.pointer(pre)
        model = t.load(lib, vctx, directory / "image.onnx")
        copied = sized(a.am_image_input_t)
        copied.kind, copied.view = a.AM_IMG_PIXEL_VIEW, c.pointer(pixel)
        reference, ref_id = c.POINTER(a.amv_result_t)(), c.c_uint64()
        check(lib.amv_infer_image_sync(model, c.byref(copied), c.byref(params), c.byref(reference), c.byref(ref_id)))
        try:
            outputs = base.outputs(reference)
        finally:
            check(lib.amv_result_ack(vctx, ref_id.value))
            check(lib.amv_result_release(reference))
        started = time.perf_counter()
        elapsed = []
        for i in range(30):
            shared.publish(camera, pools[0], pixel, a.AM_ERR_SDK_QUEUE_FULL)
            publication = shared.publish(camera, pools[1], pixel)
            healthy_view = shared.borrow(lib, consumers[1], publication)
            try:
                assert c.string_at(healthy_view.plane_data[0], frame.plane_size_bytes[0]) == frozen
            finally:
                check(lib.amv_shared_pool_return(consumers[1], c.byref(healthy_view)))
            output, rid = c.POINTER(a.amv_result_t)(), c.c_uint64()
            infer_started = time.perf_counter()
            check(lib.amv_infer_image_async(model, c.byref(image), c.byref(params), c.byref(rid)))
            base.wait(lib, vctx, rid.value)
            check(lib.amv_result_fetch(vctx, rid.value, c.byref(output)))
            try:
                assert output.contents.frame.contents.frame_id == assoc.frame_id
                assert output.contents.frame.contents.source_generation == assoc.source_generation
                assert output.contents.frame.contents.config_generation == assoc.config_generation
                current = base.outputs(output)
                assert current == outputs
            finally:
                check(lib.amv_result_ack(vctx, rid.value))
                check(lib.amv_result_release(output))
            elapsed.append((time.perf_counter() - infer_started) * 1000)
            assert elapsed[-1] <= 2000, elapsed[-1]
            time.sleep(5 / 30)
        assert c.string_at(view.plane_data[0], frame.plane_size_bytes[0]) == frozen
        held = shared.stats(camera, pools[0])
        assert held.outstanding_borrows == 1
        check(lib.amv_shared_pool_return(consumers[0], c.byref(view)))
        views.clear()
        shared.publish(camera, pools[0], pixel)
        return {"held_slot_preserved": True, "held_publish_rejections": 30, "other_pool_publications": 30,
                "shared_inputs_real_inference": 30, "held_seconds": time.perf_counter() - started,
                "infer_calls_ms": elapsed,
                "resumed_after_return": True, "output_sha256": hashlib.sha256(outputs[0][2]).hexdigest()}
    finally:
        for view in views:
            check(lib.amv_shared_pool_return(consumers[0], c.byref(view)))
        if model:
            base.close_model(lib, model)
        for consumer in consumers:
            shared.close(lib, consumer, False, True)
        for pool in pools:
            shared.close(camera, pool, True, True)
        shared.close(lib, vctx, False)
        shared.close(camera, cctx, True)


def main(args):
    out = local_path(args.output.resolve(), "evidence", fresh=True)
    out.mkdir(parents=True)
    target = {"source": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
              "metric": "MET-2 M-RECOVER S4/S5", "healthy_each_call_deadline_ms": 2000,
              "unfetched_results": 2, "control_items": 16, "shared_pool_slots_each": 1,
              "shared_live_hold_min_seconds": 5,
              "scope": "actual CPU Identity; bounded input; not five profiles, acceleration or soak"}
    (out / "target-card.json").write_text(json.dumps(target, indent=2), encoding="utf-8")
    camera = c.CDLL(str(args.dll_dir.resolve() / "am_camera.dll"))
    lib = c.CDLL(str(args.dll_dir.resolve() / "am_vision.dll"))
    a.bind(camera, "amc_", require_all=True)
    a.bind(lib, "amv_", require_all=True)
    result = {}
    try:
        for name, invoke in [("results", lambda: results_pressure(lib, args.runtime_dir, args.tensor_dir)),
                             ("notifications", lambda: slow_notifications(lib, args.runtime_dir, args.tensor_dir)),
                             ("shared", lambda: shared_pressure(camera, lib, args.runtime_dir, args.tensor_dir))]:
            result[name] = invoke()
            (out / (name + ".json")).write_text(json.dumps(result[name], indent=2), encoding="utf-8")
        lifecycle = module("test-module-lifecycle.py")
        assert not lifecycle.workers(lifecycle.windows())
    except BaseException as error:
        (out / "failure.json").write_text(json.dumps({"type": type(error).__name__, "message": str(error)}, indent=2), encoding="utf-8")
        raise
    result["identity"] = {name: hashlib.sha256((args.dll_dir / name).read_bytes()).hexdigest() for name in ["am_camera.dll", "am_vision.dll"]}
    result["identity"]["model_sha256"] = hashlib.sha256((args.tensor_dir / "image.onnx").read_bytes()).hexdigest()
    result["identity"]["runtime_files"] = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in args.runtime_dir.glob("*.dll")}
    result["passed"] = True
    (out / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps({"passed": True, "output": str(out)}))


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    for name in ["dll-dir", "runtime-dir", "tensor-dir", "output"]:
        p.add_argument("--" + name, type=Path, required=True)
    main(p.parse_args())
