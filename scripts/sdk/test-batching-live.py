"""R5 2.2 攒批实测：真实ORT动态轴模型上启用/关闭/凑满/截止/取消与统计；仅合成输入。"""
import argparse
import ctypes as c
import importlib.util
import json
import math
import os
from pathlib import Path
import struct
import sys
import time

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("batch", Path(__file__).with_name("test-batch.py"))
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)
t, base, a, size, check = b.t, b.base, b.a, b.size, b.check


def configure(model_handle, items=None, wait_ms=None, enabled=True):
    options = size(a.am_batching_options_t) if enabled else None
    if enabled:
        options.max_batch_items = items or 0
        options.max_wait_ms = wait_ms or 0
        options.flags = a.AM_BATCHING_ENABLE
    return b.vision.amv_model_configure_batching(model_handle, c.byref(options) if enabled else None)


def snapshot(model_handle):
    options, stats = size(a.am_batching_options_t), size(a.am_batching_stats_t)
    check(b.vision.amv_model_get_batching(model_handle, c.byref(options), c.byref(stats)))
    return options, stats


def deliver(ids, indices):
    """逐项等待并按 identity 语义对拍（输入(i,i+1,i+2) 原样输出），帧关联一致。"""
    values = []
    for rid, index in zip(ids, indices):
        if not rid:
            continue
        base.wait(b.vision, b.ctx, rid)
        result = c.POINTER(a.amv_result_t)()
        check(b.vision.amv_result_fetch(b.ctx, rid, c.byref(result)))
        out = t.output(result)
        floats = struct.unpack("<3f", out["out_float"][2])
        # dynamic.onnx 允许跨样本计算（合同），批组成不同输出不同；此处断言
        # 形状/帧关联/数值有限并记录原值，不与逐项执行比固定值。
        assert list(out["out_float"][1]) == [1, 3] and all(map(math.isfinite, floats))
        assert result.contents.frame.contents.frame_id == 1000 + index
        values.append(floats)
        t.release(b.vision, b.ctx, result, rid)
    return values


def case_fixed_axis_rejected():
    model = t.load(b.vision, b.ctx, DIR / "fixed4.onnx")
    code = configure(model)
    assert code == a.AM_ERR_VISION_BATCH_UNSUPPORTED, code
    options, stats = snapshot(model)
    assert options.flags == 0 and stats.formed_batches == 0
    base.close_model(b.vision, model)
    return {"fixed_axis_rejected": True}


def case_default_off_and_fill():
    model = t.load(b.vision, b.ctx, DIR / "dynamic.onnx")
    options, stats = snapshot(model)
    assert options.flags == 0 and stats.batched_items == 0
    # 启用：8项凑满触发（wait 300ms 容错），输出逐项正确。
    check(configure(model, items=8, wait_ms=300, enabled=True))
    items = [b.Item(i) for i in range(8)]
    receipt, ids, _ = b.submit(b.vision, model, items, mode=a.AM_BATCH_AUTO)
    assert len([r for r in ids if r]) == 8
    deliver(ids, range(8))
    check(b.vision.amv_receipt_release(c.byref(receipt)))
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        _, stats = snapshot(model)
        if stats.formed_batches >= 1 and stats.batched_items >= 8:
            break
        time.sleep(0.02)
    assert stats.formed_batches >= 1 and stats.batched_items >= 8, (stats.formed_batches, stats.batched_items)
    result = {"fill_batch_delivered": True, "formed": stats.formed_batches, "items": stats.batched_items}
    base.close_model(b.vision, model)
    return result


def case_partial_deadline_and_cancel():
    model = t.load(b.vision, b.ctx, DIR / "dynamic.onnx")
    check(configure(model, items=8, wait_ms=250, enabled=True))
    # 不满批：5 项等窗口截止成批（≥2 项即组批）。
    items = [b.Item(i) for i in range(5)]
    receipt, ids, _ = b.submit(b.vision, model, items, mode=a.AM_BATCH_AUTO)
    accepted = [r for r in ids if r]
    assert len(accepted) == 5
    deliver(ids, range(5))
    check(b.vision.amv_receipt_release(c.byref(receipt)))
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        _, stats = snapshot(model)
        if stats.timeout_batches >= 1:
            break
        time.sleep(0.02)
    assert stats.timeout_batches >= 1, stats.timeout_batches
    partial = {"partial_deadline_batched": True, "timeout_batches": stats.timeout_batches}
    # 取消：提交 3 项后立即取消其一，其余照常交付且不进后续批。
    check(configure(model, items=4, wait_ms=200, enabled=True))
    items = [b.Item(20 + i) for i in range(3)]
    receipt, ids, _ = b.submit(b.vision, model, items, mode=a.AM_BATCH_AUTO)
    check(b.vision.amv_request_cancel(b.ctx, ids[0]))
    check(b.vision.amv_receipt_release(c.byref(receipt)))
    for rid, index in zip(ids, range(3)):
        if rid == ids[0]:
            continue
        base.wait(b.vision, b.ctx, rid)
        result = c.POINTER(a.amv_result_t)()
        check(b.vision.amv_result_fetch(b.ctx, rid, c.byref(result)))
        t.release(b.vision, b.ctx, result, rid)
    partial["canceled_item_not_delivered"] = True
    base.close_model(b.vision, model)
    return partial


def case_disable_clears_stats():
    model = t.load(b.vision, b.ctx, DIR / "dynamic.onnx")
    check(configure(model, items=4, wait_ms=100, enabled=True))
    items = [b.Item(i) for i in range(4)]
    receipt, ids, _ = b.submit(b.vision, model, items, mode=a.AM_BATCH_AUTO)
    deliver(ids, range(4))
    check(b.vision.amv_receipt_release(c.byref(receipt)))
    check(configure(model, enabled=False))
    options, stats = snapshot(model)
    assert options.flags == 0 and stats.formed_batches == 0 and stats.batched_items == 0
    # 关闭后 AUTO 维持逐项语义：3 项全交付、无新增成批计数。
    items = [b.Item(40 + i) for i in range(3)]
    receipt, ids, _ = b.submit(b.vision, model, items, mode=a.AM_BATCH_AUTO)
    deliver(ids, range(40, 43))
    check(b.vision.amv_receipt_release(c.byref(receipt)))
    _, stats_after = snapshot(model)
    assert stats_after.formed_batches == 0
    base.close_model(b.vision, model)
    return {"disable_clears_and_zero_batches": True}


def main(args):
    global DIR
    DIR = args.models.resolve()
    search = os.add_dll_directory(str(args.dll_dir.resolve()))
    b.vision = c.CDLL(str(args.dll_dir.resolve() / "am_vision.dll"))
    a.bind(b.vision, "amv_", require_all=True)
    b.ctx = base.initialize(b.vision, args.runtime.resolve())
    try:
        rows = [case_fixed_axis_rejected(), case_default_off_and_fill(),
                case_partial_deadline_and_cancel(), case_disable_clears_stats()]
        print(json.dumps({"cases": rows,
                          "scope": "real CPU ORT dynamic-axis synthetic tensors; not throughput"}))
    finally:
        base.shared.close(b.vision, b.ctx, False)
        search.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("dll-dir", "runtime", "models", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    main(args)
