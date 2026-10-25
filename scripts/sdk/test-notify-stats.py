"""真实推理通知/注销关闭屏障及统计拥有者验证；不启动主服务。"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import struct
import sys
import threading
import time

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("tensor_test", Path(__file__).with_name("test-tensors.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
a, base, size, check = t.a, t.base, t.size, t.check


def submit(vision, model):
    tensor = t.Tensor("images", a.AM_DTYPE_F32, [1, 3, 2, 2], struct.pack("<12f", *range(12)))
    array = (a.am_tensor_in_t*1)(tensor.view)
    rid = c.c_uint64()
    code = vision.amv_infer_tensors_async(model, 1, array, None, c.byref(rid))
    tensor.clear()
    return code, rid.value


def stats(vision, ctx):
    out = c.POINTER(a.am_stats_t)()
    check(vision.amv_get_stats(ctx, None, c.byref(out)))
    assert out.contents.scope_utf8 == b"context"
    value = {key: getattr(out.contents, key) for key in ["accepted", "rejected_backpressure", "dropped",
        "infer_executed", "infer_failed", "threads_active", "threads_peak", "external_held_items"]}
    check(vision.amv_stats_release(out))
    return value


def basic(vision, ctx, model):
    events, errors = [], []
    ready = threading.Event()

    @a.am_result_notify_fn
    def callback(handle, rid, terminal, user):
        try:
            assert terminal == a.AM_REQ_DONE and user == 7
            state = c.c_uint32()
            check(vision.amv_model_close(handle, 0, c.byref(state)), a.AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION)
            check(vision.amv_shutdown(ctx, 0, c.byref(state)), a.AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION)
            check(vision.amv_result_notify_callback_set(handle, a.am_result_notify_fn(), None), a.AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION)
            result = c.POINTER(a.amv_result_t)()
            check(vision.amv_result_fetch(ctx, rid, c.byref(result)))
            assert t.output(result)["pixels"][2] == struct.pack("<12f", *range(12))
            check(vision.amv_result_ack(ctx, rid))
            check(vision.amv_result_release(result))
            events.append(rid)
        except Exception as e:
            errors.append(repr(e))
        finally:
            ready.set()
    check(vision.amv_result_notify_callback_set(model, callback, 7))
    for _ in range(4):
        ready.clear()
        code, rid = submit(vision, model)
        check(code)
        assert ready.wait(5), "notification missing"
        assert not errors, errors
        assert events[-1] == rid
    check(vision.amv_result_notify_callback_set(model, a.am_result_notify_fn(), None))
    snapshot = stats(vision, ctx)
    assert snapshot["accepted"] == 4 and snapshot["infer_executed"] == 4
    assert snapshot["infer_failed"] == 0 and snapshot["threads_active"] >= 3
    assert snapshot["threads_peak"] >= snapshot["threads_active"]
    code, rid = submit(vision, model)
    check(code)
    base.wait(vision, ctx, rid)
    result = c.POINTER(a.amv_result_t)()
    check(vision.amv_result_fetch(ctx, rid, c.byref(result)))
    assert stats(vision, ctx)["external_held_items"] == 1
    t.release(vision, ctx, result, rid)
    assert len(events) == 4
    return {"notified": 4, "callbackFetchAckRelease": True, "unsafeReentryRejected": True,
            "afterUnregisterNoCallback": True, "stats": snapshot}


def barriers(vision, ctx, model):
    gate, entered, unregistered = threading.Event(), threading.Event(), threading.Event()
    errors, notified = [], []

    @a.am_result_notify_fn
    def callback(handle, rid, state, user):
        notified.append(rid)
        entered.set()
        if not gate.wait(5):
            errors.append("callback release gate expired")

    policy = size(a.am_queue_policy_t)
    check(vision.amv_get_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
    policy.max_items = 2
    check(vision.amv_set_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
    check(vision.amv_result_notify_callback_set(model, callback, None))
    code, first = submit(vision, model)
    check(code)
    assert entered.wait(5)
    ids = [first]
    # 慢回调不会占用推理执行线程；后续结果仍可按ID拉取。
    for _ in range(8):
        code, rid = submit(vision, model)
        check(code)
        ids.append(rid)
    deadline = time.monotonic()+5
    remaining = set(ids)
    while remaining:
        for rid in list(remaining):
            result = c.POINTER(a.amv_result_t)()
            code = vision.amv_result_fetch(ctx, rid, c.byref(result))
            if code == a.AM_ERR_SDK_TRY_AGAIN:
                continue
            check(code)
            t.release(vision, ctx, result, rid)
            remaining.remove(rid)
        assert time.monotonic() < deadline

    def unregister():
        code = vision.amv_result_notify_callback_set(model, a.am_result_notify_fn(), None)
        if code != 0:
            errors.append(code)
        unregistered.set()
    thread = threading.Thread(target=unregister)
    thread.start()
    time.sleep(.02)
    assert not unregistered.is_set(), "barrier returned while callback still active"
    gate.set()
    thread.join(5)
    assert unregistered.is_set() and not errors, errors
    assert notified == [first]
    assert stats(vision, ctx)["dropped"] > 0

    gate.clear()
    entered.clear()
    check(vision.amv_result_notify_callback_set(model, callback, None))
    check(submit(vision, model)[0])
    assert entered.wait(5)
    state = c.c_uint32()
    check(vision.amv_model_close(model, 0, c.byref(state)))
    assert state.value == a.AM_LIFECYCLE_CLOSING
    gate.set()
    base.close_model(vision, model)
    assert stats(vision, ctx)["threads_active"] == 0
    return {"slowCallbackDoesNotBlockInference": True, "notificationOverflowQueryable": True,
            "unregisterWaitsForActiveCallback": True, "closeWaitsForCallback": True,
            "notificationThreadReclaimed": True}


def ownership(vision, ctx):
    pointer = c.POINTER(a.am_stats_t)()
    check(vision.amv_get_stats(ctx, b"invalid", c.byref(pointer)), a.AM_ERR_SDK_INVALID_ARGUMENT)
    assert not pointer
    check(vision.amv_get_stats(ctx, None, c.byref(pointer)))
    check(vision.amv_result_release(c.cast(pointer, c.POINTER(a.amv_result_t))), a.AM_ERR_SDK_HANDLE_INVALID)
    state = c.c_uint32()
    check(vision.amv_shutdown(ctx, 0, c.byref(state)))
    assert state.value == a.AM_LIFECYCLE_CLOSING
    assert pointer.contents.scope_utf8 == b"context"
    check(vision.amv_stats_release(pointer))
    check(vision.amv_stats_release(pointer), a.AM_ERR_SDK_HANDLE_INVALID)
    base.shared.close(vision, ctx, False)
    return {"statsKeepsContextClosing": True, "wrongReleaseAndDoubleReleaseRejected": True}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--sample-dir", type=Path, required=True)
    args = parser.parse_args()
    vision = c.CDLL(str((args.dll_dir/"am_vision.dll").resolve()))
    missing = a.bind(vision, "amv_")
    ctx = base.initialize(vision, args.runtime_dir)
    model = t.load(vision, ctx, args.sample_dir/"image.onnx")
    result = {"notification": basic(vision, ctx, model)}
    result["barriers"] = barriers(vision, ctx, model)
    result["ownership"] = ownership(vision, ctx)
    result["missingVision"] = missing
    print(json.dumps(result))
