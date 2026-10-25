"""真实推理DLL的有界合成负载；测C ABI往返，不冒充YOLO业务性能。"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import struct
import sys
import time
from perf_common import SCHEMA, file_sha256, summarize, validate

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("tensor_test", Path(__file__).with_name("test-tensors.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
a, base, size, check = t.a, t.base, t.size, t.check
ROOT = Path(__file__).resolve().parents[2]


def run_case(lib, ctx, model, bursts, width, slow_ms, observed=True):
    durations, execution_ms = [], []
    rejected = 0
    before = snapshot(lib, ctx)
    sequence = 0
    started = time.perf_counter_ns()
    for _ in range(bursts):
        pending = {}
        for _ in range(width):
            sequence += 1
            values = struct.pack("<12f", *([float(sequence)] * 12))
            tensor = t.Tensor("images", a.AM_DTYPE_F32, [1, 3, 2, 2], values)
            assoc = size(a.am_frame_assoc_t)
            assoc.valid, assoc.frame_id, assoc.source_id = a.AM_ASSOC_FRAME | a.AM_ASSOC_SOURCE, sequence, 42
            tensor.view.assoc = c.pointer(assoc)
            params = size(a.am_infer_params_t)
            params.slot_key, params.confidence_threshold = 1, .25
            rid = c.c_uint64()
            start = time.perf_counter_ns()
            code = lib.amv_infer_tensors_async(model, 1, c.byref(tensor.view), c.byref(params), c.byref(rid))
            tensor.clear()
            if code == a.AM_ERR_SDK_QUEUE_FULL:
                assert rid.value == 0
                rejected += 1
            else:
                check(code)
                assert rid.value and rid.value not in pending
                pending[rid.value] = (start, sequence, values)
        if slow_ms:
            time.sleep(slow_ms / 1000)
        deadline = time.monotonic() + 10
        while pending:
            progressed = False
            for rid, (start, frame_id, values) in list(pending.items()):
                result = c.POINTER(a.amv_result_t)()
                code = lib.amv_result_fetch(ctx, rid, c.byref(result))
                if code == a.AM_ERR_SDK_TRY_AGAIN:
                    continue
                check(code)
                end = time.perf_counter_ns()
                assert result.contents.frame.contents.frame_id == frame_id
                assert result.contents.frame.contents.source_id == 42
                assert t.output(result)["pixels"] == (a.AM_DTYPE_F32, [1, 3, 2, 2], values)
                durations.append(end - start)
                execution_ms.append(result.contents.infer_end_ms - result.contents.infer_start_ms)
                t.release(lib, ctx, result, rid)
                del pending[rid]
                progressed = True
            if observed:
                snapshot(lib, ctx)
            if not progressed:
                time.sleep(.0005)
            if time.monotonic() > deadline:
                raise TimeoutError("accepted inference did not terminate")
    elapsed = time.perf_counter_ns() - started
    after = snapshot(lib, ctx)
    assert after["accepted"] - before["accepted"] == len(durations)
    assert after["rejected_backpressure"] - before["rejected_backpressure"] == rejected
    assert after["external_held_items"] == 0
    assert after["threads_peak"] <= 16
    return dict(samples_ns=durations, elapsed_ns=elapsed,
                counts=dict(attempted=width * bursts, delivered=len(durations), rejected=rejected, dropped=0),
                execution_samples_ms=execution_ms, stats_after=after)


def snapshot(lib, ctx):
    pointer = c.POINTER(a.am_stats_t)()
    check(lib.amv_get_stats(ctx, None, c.byref(pointer)))
    try:
        return {key: getattr(pointer.contents, key) for key in (
            "accepted", "rejected_backpressure", "infer_executed", "infer_failed",
            "threads_active", "threads_peak", "external_held_items", "queue_items", "queue_bytes")}
    finally:
        check(lib.amv_stats_release(pointer))


def main():
    import platform
    import subprocess
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to((ROOT / ".local/evidence").resolve()):
        raise SystemExit("use a new output directory below .local/evidence")
    lib = c.CDLL(str((args.dll_dir / "am_vision.dll").resolve()))
    a.bind(lib, "amv_", require_all=True)
    params = size(a.amv_init_params_t)
    params.abi_version, params.infer_threads = a.AM_ABI_VERSION, 2
    params.domain_infer_slots, params.total_thread_budget = 8, 16
    dirs = (c.c_char_p * 1)(str(args.runtime_dir.resolve()).encode())
    params.runtime_dirs_utf8, params.runtime_dir_count = dirs, 1
    ctx = c.POINTER(a.amv_context_t)()
    check(lib.amv_init(c.byref(params), c.byref(ctx)))
    model = None
    try:
        model = t.load(lib, ctx, args.model)
        info = size(a.am_model_info_t)
        check(lib.amv_model_get_info(model, c.byref(info)))
        assert info.execution.actual == a.AM_BACKEND_CPU
        policy = size(a.am_realtime_policy_t)
        policy.max_inflight, policy.replace_oldest = 8, 0
        policy.slot_count, policy.fair_round_robin = 1, 1
        check(lib.amv_set_realtime_policy(model, c.byref(policy)))
        build = dict(source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                     source_state="dirty" if subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT) else "clean",
                     artifact_sha256=file_sha256(args.dll_dir / "am_vision.dll"), build_profile="release")
        version = size(a.am_version_info_t)
        check(lib.amv_get_version_info(c.byref(version)))
        build["loaded_sdk"] = dict(package=version.package_version_utf8.decode(),
                                   target=version.build_target_utf8.decode(),
                                   notes=version.runtime_notes_utf8.decode())
        runtime_hash = {p.name: file_sha256(p) for p in sorted(args.runtime_dir.glob("*.dll"))}
        import hashlib
        runtime_digest = hashlib.sha256(json.dumps(runtime_hash, sort_keys=True).encode()).hexdigest()
        output.mkdir(parents=True)
        run_case(lib, ctx, model, 20, 1, 0)
        summaries = {}
        for round_id in range(1, 4):
            for name, width, slow, observed in (("single", 1, 0, True), ("within-budget", 4, 0, True),
                    ("over-budget-slow", 32, 20, True), ("observation-off", 4, 0, False)):
                raw = run_case(lib, ctx, model, 32, width, slow, observed)
                report = dict(schema=SCHEMA, build=build,
                    identity=dict(workload=dict(name=name, bursts=32, width=width, slow_consumer_ms=slow, observed=observed),
                        input_sha256=hashlib.sha256(b"sequence 1..width*32 repeated f32[1,3,2,2]").hexdigest(),
                        model_sha256=file_sha256(args.model), runtime_sha256=runtime_digest,
                        hardware=dict(os=platform.platform(), cpu=platform.processor()), backend="onnxruntime-cpu",
                        threads=2, budgets=dict(slots=8, total_threads=16, request_timeout_ms=10_000), warmup=20,
                        timing_boundary="Python C ABI async submit through result fetch; includes polling/consumer delay, excludes validation/release"),
                    runtime_files=runtime_hash, unknown=dict(decode_latency=None, preprocess_latency=None, queue_wait_per_request=None),
                    **raw)
                validate(report)
                (output / f"{name}-{round_id}.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
                summaries[f"{name}-{round_id}"] = dict(latency=summarize(raw["samples_ns"]), counts=raw["counts"])
        print(json.dumps(dict(output=str(output), summaries=summaries,
                             scope="actual CPU DLL; synthetic identity model; not YOLO performance"), indent=2))
    finally:
        if model:
            base.close_model(lib, model)
        base.shared.close(lib, ctx, False)


if __name__ == "__main__":
    main()
