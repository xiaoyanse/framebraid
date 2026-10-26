"""真实CPU DLL：请求表低预算拒绝、ID与外持关闭边界；不连接设备。"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import struct
import sys
import time

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("record_tensor", Path(__file__).with_name("test-tensors.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
a, base, size, check = t.a, t.base, t.size, t.check


def submit(lib, model):
    tensor = t.Tensor("images", a.AM_DTYPE_F32, [1, 3, 2, 2], struct.pack("<12f", *range(12)))
    rid = c.c_uint64()
    code = lib.amv_infer_tensors_async(model, 1, c.byref(tensor.view), None, c.byref(rid))
    tensor.clear()
    return code, rid.value


def fetch(lib, ctx, rid):
    result = c.POINTER(a.amv_result_t)()
    deadline = time.monotonic() + 5
    while True:
        status = lib.amv_result_fetch(ctx, rid, c.byref(result))
        if status != a.AM_ERR_SDK_TRY_AGAIN:
            check(status)
            return result
        assert time.monotonic() < deadline
        time.sleep(.001)


def replacement_guard(lib, args):
    spec = importlib.util.spec_from_file_location("record_batch", Path(__file__).with_name("test-batch.py"))
    batch = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(batch)
    ctx = base.initialize(lib, args.runtime_dir)
    model = t.load(lib, ctx, args.model)
    gate = batch.Gated()
    try:
        policy = size(a.am_realtime_policy_t)
        policy.max_inflight, policy.slot_count, policy.replace_oldest, policy.fair_round_robin = 2, 1, 1, 1
        check(lib.amv_set_realtime_policy(model, c.byref(policy)))
        first = gate.start(lib, model)
        code, second = submit(lib, model)
        check(code)
        assert second == first + 1
        control = size(a.am_queue_policy_t)
        check(lib.amv_get_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(control)))
        control.max_bytes = 1
        check(lib.amv_set_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(control)))
        code, refused = submit(lib, model)
        check(code, a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
        assert refused == 0
        gate.gate.set()
        for rid in (first, second):
            result = fetch(lib, ctx, rid)
            check(lib.amv_result_ack(ctx, rid))
            check(lib.amv_result_release(result))
        # 旧容量仍在账，不需新增存储时可复用；拒绝没有消耗序号。
        code, third = submit(lib, model)
        check(code)
        assert third == first + 2
        result = fetch(lib, ctx, third)
        check(lib.amv_result_ack(ctx, third))
        check(lib.amv_result_release(result))
        assert gate.counts == [1, 1]
        return dict(growth_rejected_without_replacing_pending=True, no_id_gap=True,
                    retained_capacity_reused=True, input_owners_balanced=True)
    finally:
        gate.gate.set()
        base.close_model(lib, model)
        base.shared.close(lib, ctx, False)


def run(args):
    lib = c.CDLL(str((args.dll_dir / "am_vision.dll").resolve()))
    assert not a.bind(lib, "amv_")
    rows = []
    for budget in (1, 64, 65536):
        ctx = base.initialize(lib, args.runtime_dir)
        model = t.load(lib, ctx, args.model)
        try:
            policy = size(a.am_queue_policy_t)
            check(lib.amv_get_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
            policy.max_bytes = budget
            check(lib.amv_set_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
            code, rid = submit(lib, model)
            rejected = budget < 65536 and not args.baseline
            check(code, a.AM_ERR_SDK_RESOURCE_EXHAUSTED if rejected else 0)
            assert bool(rid) != rejected, (budget, code, rid)
            if rid:
                assert rid & 0xffffffffffff == 1
                result = fetch(lib, ctx, rid)
                assert t.output(result)["pixels"][2] == struct.pack("<12f", *range(12))
                t.release(lib, ctx, result, rid)
                code2, rid2 = submit(lib, model)
                check(code2)
                assert rid2 == rid + 1
                result = fetch(lib, ctx, rid2)
                check(lib.amv_result_ack(ctx, rid2))
                check(lib.amv_result_release(result))
            rows.append(dict(control_bytes=budget, code=code, published_id=rid))
        finally:
            base.close_model(lib, model)
            # 常驻请求表不得伪装成调用方外持缓冲阻止Context关闭。
            state = c.c_uint32()
            check(lib.amv_shutdown(ctx, 1000, c.byref(state)))
            assert state.value == a.AM_LIFECYCLE_CLOSED, state.value
            base.shared.close(lib, ctx, False)
    return dict(baseline=args.baseline, rows=rows, context_closed_with_idle_record_storage=True,
                replacement=None if args.baseline else replacement_guard(lib, args))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ("dll-dir", "runtime-dir", "model"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--baseline", action="store_true", help="仅记录旧版未计费行为，不作为通过条件")
    print(json.dumps(run(parser.parse_args()), indent=2))
