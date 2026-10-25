"""真实DLL调度回归：公平开关、忙模型隔离、并发关闭及原生批次持有。"""
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
spec = importlib.util.spec_from_file_location("batch_test", Path(__file__).with_name("test-batch.py"))
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)
t, a, base, size, check = b.t, b.a, b.base, b.size, b.check


def submit(lib, model, slot):
    tensor = t.Tensor("images", a.AM_DTYPE_F32, [1,3,2,2], struct.pack("<12f", *range(12)))
    params = size(a.am_infer_params_t)
    params.slot_key, params.confidence_threshold = slot, .25
    rid = c.c_uint64()
    check(lib.amv_infer_tensors_async(model, 1, c.byref(tensor.view), c.byref(params), c.byref(rid)))
    tensor.clear()
    return rid.value


def slot_fairness(lib, runtime, model_path, fair):
    ctx = base.initialize(lib, runtime)
    model = t.load(lib, ctx, model_path)
    gate = b.Gated()
    trace, errors = [], []
    complete = threading.Event()
    @a.am_result_notify_fn
    def notify(handle, rid, terminal, user):
        if terminal != a.AM_REQ_DONE:
            errors.append((rid, terminal))
        trace.append(rid)
        if len(trace) == 7:
            complete.set()
    try:
        policy = size(a.am_realtime_policy_t)
        policy.max_inflight, policy.slot_count, policy.fair_round_robin = 8, 3, int(fair)
        check(lib.amv_set_realtime_policy(model, c.byref(policy)))
        check(lib.amv_result_notify_callback_set(model, notify, None))
        first = gate.start(lib, model)
        slots = [1,1,1,2,2,2]
        ids = [submit(lib, model, slot) for slot in slots]
        gate.gate.set()
        assert complete.wait(5) and not errors
        check(lib.amv_result_notify_callback_set(model, a.am_result_notify_fn(), None))
        assert trace[0] == first
        actual = [dict(zip(ids, slots))[rid] for rid in trace[1:]]
        assert actual == ([1,2,1,2,1,2] if fair else slots), actual
        for rid in [first, *ids]:
            base.wait(lib, ctx, rid)
            out = c.POINTER(a.amv_result_t)()
            check(lib.amv_result_fetch(ctx, rid, c.byref(out)))
            try:
                if rid != first:
                    assert t.output(out)["pixels"][2] == struct.pack("<12f", *range(12))
            finally:
                check(lib.amv_result_ack(ctx, rid))
                check(lib.amv_result_release(out))
        assert gate.counts == [1,1]
        return dict(enabled=fair, completion_slots=actual, owners=gate.counts)
    finally:
        gate.gate.set()
        base.close_model(lib, model)
        base.shared.close(lib, ctx, False)


def busy_close(lib, runtime, model_path):
    ctx = base.initialize(lib, runtime)
    first = t.load(lib, ctx, model_path)
    second = t.load(lib, ctx, model_path)
    gate = b.Gated()
    try:
        policy = size(a.am_realtime_policy_t)
        policy.max_inflight, policy.slot_count, policy.fair_round_robin = 8, 2, 1
        check(lib.amv_set_realtime_policy(first, c.byref(policy)))
        gate.start(lib, first)
        pending = [submit(lib, first, 1) for _ in range(6)]
        for rid in pending:
            check(lib.amv_request_cancel(ctx, rid))
        lifecycle = c.c_uint32()
        check(lib.amv_model_close(first, 0, c.byref(lifecycle)))
        assert lifecycle.value == a.AM_LIFECYCLE_CLOSING
        start = time.perf_counter_ns()
        rid = submit(lib, second, 1)
        base.wait(lib, ctx, rid)
        elapsed = time.perf_counter_ns() - start
        assert elapsed < 100_000_000
        out = c.POINTER(a.amv_result_t)()
        check(lib.amv_result_fetch(ctx, rid, c.byref(out)))
        assert t.output(out)["pixels"][2] == struct.pack("<12f", *range(12))
        t.release(lib, ctx, out, rid)
        check(lib.amv_model_close(first, 0, c.byref(lifecycle)))
        assert lifecycle.value == a.AM_LIFECYCLE_CLOSING, "active input released before callback returned"
        gate.gate.set()
        base.close_model(lib, first)
        first = None
        assert gate.counts == [1,1]
        return dict(other_model_elapsed_ms=elapsed/1e6, active_owner_blocks_close=True)
    finally:
        gate.gate.set()
        for model in (first, second):
            if model:
                base.close_model(lib, model)
        base.shared.close(lib, ctx, False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dll-dir", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--sample-dir", required=True, type=Path)
    parser.add_argument("--image-model", required=True, type=Path)
    args = parser.parse_args()
    lib = c.CDLL(str((args.dll_dir/"am_vision.dll").resolve()))
    a.bind(lib, "amv_", require_all=True)
    results = dict(fair=[slot_fairness(lib,args.runtime_dir,args.image_model,fair) for fair in (False,True)],
                   close=busy_close(lib,args.runtime_dir,args.image_model))
    ctx = base.initialize(lib,args.runtime_dir)
    try:
        results.update(batch_modes=b.basic(lib,ctx,args.sample_dir),
                       partial=b.partial_and_modes(lib,ctx,args.sample_dir),
                       native64=b.sixty_four(lib,ctx,args.sample_dir))
    finally:
        base.shared.close(lib,ctx,False)
    for name, function in (("native_cancel",b.native_cancel),("waiting",b.waiting),
                           ("receipt_close",b.receipt_close),("admission_deadline",b.admission_deadline),
                           ("controls",b.control_and_errors)):
        results[name] = function(lib,args.runtime_dir,args.sample_dir)
    print(json.dumps(results))


if __name__ == "__main__":
    main()
