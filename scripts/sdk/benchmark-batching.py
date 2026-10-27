"""R5 3.1 攒批调度对照：动态轴模型批量负载下攒批开关的吞吐与逐项延迟。仅合成输入。"""
import argparse
import ctypes as c
import importlib.util
import json
import math
import os
from pathlib import Path
import time

sys_dont = True
from pathlib import Path as _P
spec = importlib.util.spec_from_file_location("bl", _P(__file__).with_name("test-batching-live.py"))
bl = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bl)
b, t, base, a, check, size = bl.b, bl.t, bl.base, bl.a, bl.check, bl.size


def run_round(enabled, items_total, per_submit, batch_items, wait_ms):
    model = t.load(b.vision, b.ctx, bl.DIR / "dynamic.onnx")
    try:
        if enabled:
            check(bl.configure(model, items=batch_items, wait_ms=wait_ms, enabled=True))
        receipts, ids_all = [], []
        submitted_at = {}
        rejected_count = 0
        for base_index in range(0, items_total, per_submit):
            items = [b.Item(base_index + i) for i in range(per_submit)]
            receipt, ids, _ = b.submit(b.vision, model, items, mode=a.AM_BATCH_AUTO)
            now = time.perf_counter()
            # 背压/校验拒绝如实计数：额度回收延迟时逐项回执为0，不算丢项。
            rejected_count += sum(1 for rid in ids if not rid)
            for rid, i in zip(ids, range(base_index, base_index + per_submit)):
                if rid:
                    submitted_at[rid] = now
            receipts.append(receipt)
        done_at = {}
        deadline = time.monotonic() + 60
        pending = dict(submitted_at)
        while pending and time.monotonic() < deadline:
            for rid in list(pending):
                snap = c.POINTER(a.am_request_snapshot_t)()
                check(b.vision.amv_request_query(b.ctx, rid, c.byref(snap)))
                state, terminal = snap.contents.state, bool(snap.contents.terminal)
                b.vision.amv_request_snapshot_release(snap)
                if terminal:
                    done_at[rid] = time.perf_counter()
                    del pending[rid]
            if pending:
                time.sleep(0.01)
        assert not pending, f"{len(pending)} 项未到终态"
        for receipt in receipts:
            check(b.vision.amv_receipt_release(c.byref(receipt)))
        for rid in done_at:
            check(b.vision.amv_result_ack(b.ctx, rid))
        latencies = sorted((done_at[rid] - submitted_at[rid]) * 1000 for rid in done_at)
        options, stats = bl.snapshot(model)
        return dict(
            enabled=enabled,
            rejected=rejected_count,
            wall_s=latencies[-1] / 1000 if latencies else 0,
            items=len(latencies),
            p50=latencies[len(latencies) // 2],
            p95=latencies[int(len(latencies) * 0.95) - 1],
            p99=latencies[int(len(latencies) * 0.99) - 1],
            max_ms=latencies[-1],
            formed=stats.formed_batches,
            batched=stats.batched_items,
        )
    finally:
        base.close_model(b.vision, model)


def main(args):
    bl.DIR = args.models.resolve()
    search = os.add_dll_directory(str(args.dll_dir.resolve()))
    b.vision = c.CDLL(str(args.dll_dir.resolve() / "am_vision.dll"))
    a.bind(b.vision, "amv_", require_all=True)
    b.ctx = base.initialize(b.vision, args.runtime.resolve())
    rows = []
    try:
        # 预热各一轮
        run_round(True, args.items, args.per_submit, args.batch_items, args.wait_ms)
        time.sleep(1.0)  # 轮间等待额度/缓存回收，避免上轮残留导致逐项背压拒绝。
        run_round(False, args.items, args.per_submit, 0, 0)
        time.sleep(1.0)
        order = [True, False]
        for round_index in range(args.rounds):
            for enabled in order if round_index % 2 == 0 else reversed(order):
                time.sleep(1.0)
                row = run_round(enabled, args.items, args.per_submit, args.batch_items, args.wait_ms)
                row["round"] = round_index + 1
                rows.append(row)
                print(json.dumps(row), flush=True)
    finally:
        try:
            base.shared.close(b.vision, b.ctx, False)
        except AssertionError:
            pass
        search.close()
    (args.output / "rounds.json").write_text(json.dumps(rows, indent=1) + "\n", encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("dll-dir", "runtime", "models", "output", "target-card"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--items", type=int, default=64)
    parser.add_argument("--per-submit", type=int, default=8)
    parser.add_argument("--batch-items", type=int, default=8)
    parser.add_argument("--wait-ms", type=int, default=100)
    parser.add_argument("--rounds", type=int, default=3)
    main(parser.parse_args())
