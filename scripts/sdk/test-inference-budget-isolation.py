"""真实 DLL：慢输入归还期间的分类/Context 字节隔离与超额拒绝，不连接设备。"""
import argparse
import ctypes as c
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("batch_test", Path(__file__).with_name("test-batch.py"))
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)
a, base, size, check = b.a, b.base, b.size, b.check


def policy(lib, ctx, cls, maximum):
    value = size(a.am_queue_policy_t)
    check(lib.amv_get_queue_policy(ctx, cls, c.byref(value)))
    value.max_bytes = maximum
    check(lib.amv_set_queue_policy(ctx, cls, c.byref(value)))


def finish(lib, ctx, rid):
    base.wait(lib, ctx, rid)
    result = c.POINTER(a.amv_result_t)()
    check(lib.amv_result_fetch(ctx, rid, c.byref(result)))
    try:
        value = base.outputs(result)
        assert value[0][1] == [1, 3, 2, 2] and len(value[0][2]) == 48
        return value[0][2].hex()
    finally:
        check(lib.amv_result_ack(ctx, rid))
        check(lib.amv_result_release(result))


def counters(lib, ctx):
    pointer = c.POINTER(a.am_stats_t)()
    check(lib.amv_get_stats(ctx, None, c.byref(pointer)))
    try:
        return pointer.contents.accepted, pointer.contents.rejected_backpressure
    finally:
        check(lib.amv_stats_release(pointer))


def image_submit(lib, model, gated, slot=0):
    item = gated.item(retained=False)
    params = size(a.am_infer_params_t)
    params.raw_preprocess, params.confidence_threshold = c.pointer(gated.preprocess), 0.25
    params.slot_key = slot
    rid = c.c_uint64()
    at = time.perf_counter_ns()
    code = lib.amv_infer_image_async(model, c.byref(item.image), c.byref(params), c.byref(rid))
    return code, rid.value, (time.perf_counter_ns() - at) / 1e6


def run_case(lib, runtime, path, case, baseline):
    ctx = base.initialize(lib, runtime)
    second_ctx = base.initialize(lib, runtime) if case == "other_context" else ctx
    models, held_receipt, held_id = [], None, None
    gate = b.Gated()
    # 保留实际足量的平面缓冲；加大合法尾部 padding，精确占住已声明的输入预算。
    gate.data = (c.c_ubyte * 16384)(*range(96))
    gate.frame.planes[0] = c.cast(gate.data, c.POINTER(c.c_ubyte))
    gate.frame.plane_size_bytes[0] = len(gate.data)
    normal = b.Gated()
    expected_code = a.AM_ERR_SDK_QUEUE_FULL if baseline else 0
    try:
        holder = b.t.load(lib, ctx, path)
        models.append(holder)
        candidate = b.t.load(lib, second_ctx, path)
        models.append(candidate)
        if case == "realtime_then_offline":
            policy(lib, ctx, a.AM_Q_OFFLINE_BATCH, 8192)
        else:
            policy(lib, second_ctx, a.AM_Q_REALTIME_INFER, 512)
        if case == "offline_then_realtime":
            held_receipt, held_ids, codes = b.submit(lib, holder, [gate.item()], a.AM_BATCH_SPLIT)
            assert codes == [0] and held_ids[0] and gate.entered.wait(5)
            held_id = held_ids[0]
        else:
            held_id = gate.start(lib, holder)
        before = counters(lib, second_ctx)
        at = time.perf_counter_ns()
        if case == "realtime_then_offline":
            receipt, ids, codes = b.submit(lib, candidate, [normal.item(False)], a.AM_BATCH_SPLIT)
            try:
                code, rid = codes[0], ids[0]
            finally:
                check(lib.amv_receipt_release(c.byref(receipt)))
        else:
            code, rid, _ = image_submit(lib, candidate, normal)
        duration_ms = (time.perf_counter_ns() - at) / 1e6
        assert code == expected_code and bool(rid) == (code == 0), (case, code, rid, expected_code)
        during_code = code
        after = counters(lib, second_ctx)
        delta = [after[i] - before[i] for i in range(2)]
        assert delta == ([0, 1] if baseline else [1, 0]), (case, delta)
        healthy_result = finish(lib, second_ctx, rid) if rid else None
        gate.gate.set()
        held_result = finish(lib, ctx, held_id)
        if healthy_result is not None:
            assert healthy_result == held_result
        assert gate.counts == [1, 1], gate.counts
        # 压力解除后，旧/新候选都必须可正常接收，拒绝不能泄漏额度或保留引用。
        if case == "realtime_then_offline":
            receipt, ids, codes = b.submit(lib, candidate, [normal.item(False)], a.AM_BATCH_SPLIT)
            try:
                assert codes == [0] and finish(lib, second_ctx, ids[0]) == held_result
            finally:
                check(lib.amv_receipt_release(c.byref(receipt)))
        else:
            code, rid, _ = image_submit(lib, candidate, normal)
            check(code)
            assert finish(lib, second_ctx, rid) == held_result
        return {"case": case, "healthy_code_during_hold": during_code,
                "accepted_during_hold": healthy_result is not None, "submit_ms": duration_ms,
                "accepted_rejected_delta": delta,
                "output_sha256": hashlib.sha256(bytes.fromhex(held_result)).hexdigest(),
                "balanced_owner": gate.counts, "recovered_after_release": True}
    finally:
        gate.gate.set()
        if held_receipt is not None:
            check(lib.amv_receipt_release(c.byref(held_receipt)))
        for model in reversed(models):
            base.close_model(lib, model)
        if second_ctx != ctx:
            base.shared.close(lib, second_ctx, False)
        base.shared.close(lib, ctx, False)


def saturation(lib, runtime, path):
    ctx = base.initialize(lib, runtime)
    gate = b.Gated()
    models = []
    try:
        holder, candidate = [b.t.load(lib, ctx, path) for _ in range(2)]
        models.extend([holder, candidate])
        # 208 字节/图；一项持有时，第二项超过同一实时类 300 字节上限。
        policy(lib, ctx, a.AM_Q_REALTIME_INFER, 300)
        held = gate.start(lib, holder)
        normal = b.Gated()
        code, rid, _ = image_submit(lib, candidate, normal)
        check(code, a.AM_ERR_SDK_QUEUE_FULL)
        assert rid == 0
        gate.gate.set()
        original = finish(lib, ctx, held)
        code, rid, _ = image_submit(lib, candidate, normal)
        check(code)
        assert finish(lib, ctx, rid) == original
        return {"same_class_rejects_at_300_bytes": True, "no_rejected_request_id": True,
                "recovered_after_release": True}
    finally:
        gate.gate.set()
        for model in reversed(models):
            base.close_model(lib, model)
        base.shared.close(lib, ctx, False)


def main(args):
    root = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    assert out.is_relative_to((root / ".local").resolve())
    out.mkdir(parents=True, exist_ok=True)
    dll = args.dll_dir.resolve() / "am_vision.dll"
    lib = c.CDLL(str(dll))
    a.bind(lib, "amv_", require_all=True)
    records = [run_case(lib, args.runtime_dir, args.sample_dir / "image.onnx", case, args.baseline)
               for _ in range(3) for case in ["realtime_then_offline", "offline_then_realtime", "other_context"]]
    value = {"schema": "framebraid.r3-budget-isolation/1", "baseline": args.baseline,
             "dll_sha256": hashlib.sha256(dll.read_bytes()).hexdigest(),
             "model_sha256": hashlib.sha256((args.sample_dir / "image.onnx").read_bytes()).hexdigest(),
             "cases": records, "same_class": saturation(lib, args.runtime_dir, args.sample_dir / "image.onnx"),
             "accepted_during_hold": sum(row["accepted_during_hold"] for row in records),
             "scope": "controlled owner release barrier and actual CPU identity inference; no hardware/long-run claim"}
    (out / "results.json").write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(value, ensure_ascii=False))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--sample-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--baseline", action="store_true")
    main(parser.parse_args())
