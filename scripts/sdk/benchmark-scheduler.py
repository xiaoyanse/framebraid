"""真实DLL忙模型取消/替换积压对照，使用本机合成模型，不访问设备。"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import sys
import time
from perf_common import dll_build_identity, file_sha256, process_resources

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("tensor_test", Path(__file__).with_name("test-tensors.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
a, base, size, check = t.a, t.base, t.size, t.check
ROOT = Path(__file__).resolve().parents[2]


def generate(path):
    import numpy as np
    import onnx
    from onnx import helper as h, TensorProto as T, numpy_helper
    nodes = [h.make_node("MatMul", ["matrix" if i == 0 else f"v{i-1}", "weights"],
                         ["product" if i == 63 else f"v{i}"]) for i in range(64)]
    graph = h.make_graph(nodes, "scheduler_busy_reference",
        [h.make_tensor_value_info("matrix", T.FLOAT, [512, 512])],
        [h.make_tensor_value_info("product", T.FLOAT, [512, 512])],
        [numpy_helper.from_array(np.eye(512, dtype=np.float32), "weights")])
    model = h.make_model(graph, producer_name="FrameBraid scheduler fixture", opset_imports=[h.make_opsetid("", 17)])
    model.ir_version = 10
    onnx.checker.check_model(model)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(model.SerializeToString())


def state(lib, ctx, rid):
    pointer = c.POINTER(a.am_request_snapshot_t)()
    check(lib.amv_request_query(ctx, rid, c.byref(pointer)))
    try:
        return pointer.contents.state
    finally:
        check(lib.amv_request_snapshot_release(pointer))


def submit(lib, model, slow):
    data = struct.pack("<f", 1.0) * (512*512 if slow else 12)
    tensor = t.Tensor("matrix" if slow else "images", a.AM_DTYPE_F32,
                      [512, 512] if slow else [1, 3, 2, 2], data)
    params = size(a.am_infer_params_t)
    params.slot_key, params.confidence_threshold = 1, .25
    rid = c.c_uint64()
    code = lib.amv_infer_tensors_async(model, 1, c.byref(tensor.view), c.byref(params), c.byref(rid))
    tensor.clear()
    return code, rid.value


def trial(lib, ctx, busy, healthy, replacement):
    policy = size(a.am_realtime_policy_t)
    policy.max_inflight = 2 if replacement else 8
    policy.slot_count, policy.replace_oldest, policy.fair_round_robin = 1, int(replacement), 1
    check(lib.amv_set_realtime_policy(busy, c.byref(policy)))
    code, first = submit(lib, busy, True)
    check(code)
    until = time.monotonic() + 5
    while state(lib, ctx, first) == a.AM_REQ_PENDING:
        if time.monotonic() > until:
            raise TimeoutError("busy request did not start")
        time.sleep(.0001)
    assert state(lib, ctx, first) == a.AM_REQ_RUNNING
    pending = []
    for _ in range(7):
        code, rid = submit(lib, busy, True)
        check(code)
        pending.append(rid)
    assert state(lib, ctx, first) == a.AM_REQ_RUNNING, "fixture too fast; no busy-model contention reproduced"
    start = time.perf_counter_ns()
    for rid in pending:
        check(lib.amv_request_cancel(ctx, rid))
    rejected = 0
    while True:
        code, rid = submit(lib, healthy, False)
        if code == a.AM_ERR_SDK_QUEUE_FULL:
            rejected += 1
            assert not rid
            if time.monotonic() > until:
                raise TimeoutError("healthy admission remained blocked")
            time.sleep(.0005)
            continue
        check(code)
        admitted_ns = time.perf_counter_ns() - start
        busy_at_admission = state(lib, ctx, first) == a.AM_REQ_RUNNING
        healthy_id = rid
        break
    retired = set(pending)
    while retired:
        retired = {rid for rid in retired if state(lib, ctx, rid) < a.AM_REQ_DONE}
        if time.monotonic() > until:
            raise TimeoutError("retired queue did not drain")
        if retired:
            time.sleep(.0005)
    terminal_ns = time.perf_counter_ns() - start
    for rid in pending:
        assert state(lib, ctx, rid) in (a.AM_REQ_CANCELED, a.AM_REQ_REPLACED)
        check(lib.amv_result_ack(ctx, rid))
    for rid, name, shape, values in ((first, "product", [512,512], 512*512),
                                   (healthy_id, "pixels", [1,3,2,2], 12)):
        base.wait(lib, ctx, rid)
        result = c.POINTER(a.amv_result_t)()
        check(lib.amv_result_fetch(ctx, rid, c.byref(result)))
        try:
            assert t.output(result)[name] == (a.AM_DTYPE_F32, shape, struct.pack("<f",1.0)*values)
        finally:
            t.release(lib, ctx, result, rid)
    return dict(mode="replacement" if replacement else "cancel", healthy_admission_ns=admitted_ns,
                retired_terminal_ns=terminal_ns, busy_still_running_at_healthy_admission=busy_at_admission,
                admission_rejections=rejected, canceled_or_replaced=7)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dll-dir", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--busy-model", required=True, type=Path)
    parser.add_argument("--healthy-model", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--require-early-cleanup", action="store_true")
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to((ROOT / ".local/evidence").resolve()):
        raise SystemExit("use a new output directory below .local/evidence")
    if not args.busy_model.exists():
        if not args.busy_model.resolve().is_relative_to((ROOT / ".local/tests").resolve()):
            raise SystemExit("generated model must stay below .local/tests")
        generate(args.busy_model)
    lib = c.CDLL(str((args.dll_dir/"am_vision.dll").resolve()))
    a.bind(lib, "amv_", require_all=True)
    params = size(a.amv_init_params_t)
    params.abi_version, params.infer_threads, params.domain_infer_slots, params.total_thread_budget = a.AM_ABI_VERSION, 2, 8, 16
    dirs = (c.c_char_p*1)(str(args.runtime_dir.resolve()).encode())
    params.runtime_dirs_utf8, params.runtime_dir_count = dirs, 1
    ctx = c.POINTER(a.amv_context_t)()
    check(lib.amv_init(c.byref(params), c.byref(ctx)))
    models = []
    try:
        models.append(t.load(lib, ctx, args.busy_model))
        models.append(t.load(lib, ctx, args.healthy_model))
        for model in models:
            info = size(a.am_model_info_t)
            check(lib.amv_model_get_info(model, c.byref(info)))
            assert info.execution.actual == a.AM_BACKEND_CPU
        version = size(a.am_version_info_t)
        check(lib.amv_get_version_info(c.byref(version)))
        build = dll_build_identity(args.dll_dir/"am_vision.dll", version.package_version_utf8.decode(),
            version.build_target_utf8.decode(), version.runtime_notes_utf8.decode(),
            subprocess.check_output(["git","rev-parse","HEAD"], cwd=ROOT, text=True).strip(),
            "dirty" if subprocess.check_output(["git","status","--porcelain"], cwd=ROOT) else "clean")
        before = process_resources()
        trials = [trial(lib, ctx, *models, replacement=kind) for _ in range(3) for kind in (False, True)]
        pointer = c.POINTER(a.am_stats_t)()
        check(lib.amv_get_stats(ctx, None, c.byref(pointer)))
        try:
            stats = {key:getattr(pointer.contents,key) for key in (
                "threads_active","threads_peak","queue_items","queue_bytes","external_held_items")}
            assert stats["threads_peak"] <= 16 and stats["external_held_items"] == 0
            assert stats["queue_items"] == 0 and stats["queue_bytes"] == 0
        finally:
            check(lib.amv_stats_release(pointer))
        report = dict(build=build, busy_model_sha256=file_sha256(args.busy_model),
            healthy_model_sha256=file_sha256(args.healthy_model), runtime_sha256=file_sha256(args.runtime_dir/"onnxruntime.dll"),
            budgets=dict(workers=2,slots=8,total_threads=16,retired_terminal_ms=100,healthy_admission_ms=100,observer_timeout_ms=5000),
            trials=trials, stats_after=stats, process_resources=dict(before=before,after=process_resources()),
            scope="actual CPU DLL, synthetic busy model and Identity; measures admission not model throughput")
        output.mkdir(parents=True)
        (output/"scheduler.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
        print(json.dumps(report,indent=2))
        if args.require_early_cleanup:
            assert all(r["busy_still_running_at_healthy_admission"] and r["healthy_admission_ns"] < 100_000_000
                       and r["retired_terminal_ns"] < 100_000_000 for r in trials), "early cleanup contract failed"
    finally:
        for model in reversed(models):
            base.close_model(lib, model)
        base.shared.close(lib,ctx,False)


if __name__ == "__main__":
    main()
