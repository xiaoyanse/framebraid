"""MET-2 S3真实Pose模型CPU资源/异步响应；既有strict与回退在独立进程验证。"""
import argparse
import ctypes as c
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time

from perf_common import dll_build_identity, file_sha256, summarize
from perf_resources import ResourceSampler, available_parallelism, cpu_delta
from perf_met2 import spread

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
import amrt_sdk as a


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def checked(code):
    if code != a.AM_OK:
        raise RuntimeError(f"SDK error {code}")


def close(lib, obj, function):
    state = c.c_uint32()
    checked(function(obj, 2000, c.byref(state)))
    if state.value != a.AM_LIFECYCLE_CLOSED:
        raise RuntimeError("inference lifecycle failed to drain")
    checked(lib.amv_object_destroy(obj))


def cached_report(lib, ctx):
    size = c.c_uint64()
    checked(lib.amv_get_inference_hardware_report(ctx, None, 0, c.byref(size)))
    if not 1 <= size.value <= 65536:
        raise RuntimeError("report size exceeds contract")
    data = c.create_string_buffer(size.value)
    checked(lib.amv_get_inference_hardware_report(ctx, data, size.value, c.byref(size)))
    return json.loads(data.value)


def infer_once(lib, ctx, model, image, frame_id, model_uid):
    assoc = sized(a.am_frame_assoc_t)
    assoc.valid, assoc.source_id, assoc.frame_id = a.AM_ASSOC_SOURCE | a.AM_ASSOC_FRAME, 51, frame_id
    image.assoc = c.pointer(assoc)
    params = sized(a.am_infer_params_t)
    params.confidence_threshold, params.slot_key = .25, 1
    rid, result = c.c_uint64(), c.POINTER(a.amv_result_t)()
    started = time.perf_counter_ns()
    checked(lib.amv_infer_image_async(model, c.byref(image), c.byref(params), c.byref(rid)))
    if not rid.value:
        raise RuntimeError("accepted request lacks identity")
    while True:
        code = lib.amv_result_fetch(ctx, rid.value, c.byref(result))
        if code != a.AM_ERR_SDK_TRY_AGAIN:
            checked(code)
            break
        if time.perf_counter_ns() - started > 10_000_000_000:
            raise TimeoutError("accepted request did not terminate")
        time.sleep(.0005)
    latency = time.perf_counter_ns() - started
    try:
        value = result.contents
        if value.request_id != rid.value or value.frame.contents.frame_id != frame_id or value.frame.contents.source_id != 51:
            raise RuntimeError("result identity mismatch")
        if value.model.model_uid != model_uid:
            raise RuntimeError("result model identity mismatch")
        if value.execution.actual != a.AM_BACKEND_CPU or value.task != a.AM_TASK_POSE:
            raise RuntimeError("unexpected actual backend or task")
        if value.raw_output_count != 1:
            raise RuntimeError("pose raw output count mismatch")
        tensor = value.raw_outputs[0]
        if tensor.shape_len != 3 or not tensor.shape or not tensor.data:
            raise RuntimeError("pose raw output storage missing")
        shape = list(tensor.shape[:tensor.shape_len])
        if tensor.dtype != a.AM_DTYPE_F32 or shape != [1, 300, 57] or tensor.data_size != 300 * 57 * 4:
            raise RuntimeError("pose raw output contract mismatch")
        raw = c.string_at(tensor.data, tensor.data_size)
        floats = c.cast(tensor.data, c.POINTER(c.c_float))
        if not all(math.isfinite(floats[i]) for i in range(300 * 57)):
            raise RuntimeError("non-finite inference output")
        return dict(latency_ns=latency, request_id=rid.value, frame_id=frame_id,
                    output_sha256=hashlib.sha256(raw).hexdigest(),
                    sdk_infer_interval_ms=value.infer_end_ms-value.infer_start_ms,
                    pose_count=value.pose.count)
    finally:
        try:
            checked(lib.amv_result_ack(ctx, rid.value))
        finally:
            checked(lib.amv_result_release(result))


def run_round(lib, ctx, model, image, samples, model_uid, observed=True):
    sampler = ResourceSampler(enabled=observed).start()
    n, method = available_parallelism()
    before = dict(wall_ns=time.perf_counter_ns(), cpu_ns=time.process_time_ns())
    rows, lateness = [], []
    try:
        for index in range(samples):
            due = before["wall_ns"] + index * 200_000_000
            delay = due-time.perf_counter_ns()
            if delay > 0:
                time.sleep(delay / 1e9)
            lateness.append(max(0, time.perf_counter_ns()-due))
            rows.append(infer_once(lib, ctx, model, image, index+1, model_uid))
        remaining = before["wall_ns"] + samples * 200_000_000 - time.perf_counter_ns()
        if remaining > 0:
            time.sleep(remaining / 1e9)
        after = dict(wall_ns=time.perf_counter_ns(), cpu_ns=time.process_time_ns())
    finally:
        observation = sampler.stop()
    if len({row["request_id"] for row in rows}) != len(rows):
        raise RuntimeError("duplicate request identity")
    if len({row["output_sha256"] for row in rows}) != 1:
        raise RuntimeError("same-model/CPU/input outputs changed; correctness review required")
    durations = [row["latency_ns"] for row in rows]
    resources = cpu_delta(before, after, n)
    return dict(rows=rows, latency=summarize(durations), cpu=dict(resources, parallelism_method=method),
        observation=observation, counts=dict(submitted=len(rows), delivered=len(rows),
            useful=sum(value <= 500_000_000 for value in durations), inflight_at_end=0,
            rejected=0, canceled=0, failed=0), schedule_lateness=summarize(lateness),
        useful_fps=sum(value <= 500_000_000 for value in durations) / resources["wall_s"])


def child(args):
    directory = os.add_dll_directory(str(args.dll_dir.resolve()))
    lib = c.CDLL(str(args.dll_dir.resolve() / "am_vision.dll"))
    a.bind(lib, "amv_", require_all=True)
    params = sized(a.amv_init_params_t)
    params.abi_version, params.infer_threads = a.AM_ABI_VERSION, 2
    params.total_thread_budget, params.domain_infer_slots = 16, 2
    dirs = (c.c_char_p * 1)(str(args.runtime_dir.resolve()).encode())
    params.runtime_dirs_utf8, params.runtime_dir_count = dirs, 1
    ctx, model = c.POINTER(a.amv_context_t)(), c.POINTER(a.amv_model_t)()
    init_start = time.perf_counter_ns()
    checked(lib.amv_init(c.byref(params), c.byref(ctx)))
    init_ns = time.perf_counter_ns()-init_start
    try:
        version = sized(a.am_version_info_t)
        checked(lib.amv_get_version_info(c.byref(version)))
        build = dll_build_identity(args.dll_dir / "am_vision.dll", version.package_version_utf8.decode(),
            version.build_target_utf8.decode(), version.runtime_notes_utf8.decode(),
            subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),
            "dirty" if subprocess.check_output(["git","status","--porcelain"],cwd=ROOT) else "clean")
        policy, desc = sized(a.am_backend_policy_t), sized(a.am_model_desc_t)
        policy.requested, policy.strict = args.backend, int(args.strict)
        desc.path_utf8, desc.profile_id = str(args.model.resolve()).encode(), b"pose-yolo26-e2e-coco17-v1"
        desc.load_mode, desc.backend, desc.intra_threads = a.AM_LOAD_PROFILE, c.pointer(policy), 2
        start = time.perf_counter_ns()
        code = lib.amv_model_load(ctx, c.byref(desc), c.byref(model))
        record = dict(requested_backend=args.backend, strict=args.strict, model_status=code,
                      init_ns=init_ns, model_load_ns=time.perf_counter_ns()-start, build=build)
        if args.backend != a.AM_BACKEND_CPU and args.strict:
            if code != a.AM_ERR_VISION_BACKEND_STRICT_UNAVAILABLE or model:
                raise RuntimeError(f"CPU-runtime negative control unexpectedly returned {code}")
            return record
        checked(code)
        info = sized(a.am_model_info_t)
        checked(lib.amv_model_get_info(model, c.byref(info)))
        if info.execution.actual != a.AM_BACKEND_CPU:
            raise RuntimeError("CPU runtime did not produce CPU model")
        record.update(actual_backend=info.execution.actual,
                      fallback_reason=None if not info.execution.fallback_reason_utf8 else info.execution.fallback_reason_utf8.decode())
        if args.backend != a.AM_BACKEND_CPU and not record["fallback_reason"]:
            raise RuntimeError("fallback reason missing")
        data = args.input.read_bytes()
        storage = (c.c_ubyte * len(data)).from_buffer_copy(data)
        image = sized(a.am_image_input_t)
        image.kind, image.encoded, image.encoded_size = a.AM_IMG_PNG, storage, len(data)
        model_uid=info.identity.model_uid
        warmup = [infer_once(lib, ctx, model, image, index+10000, model_uid) for index in range(3)]
        record["warmup_samples"] = warmup
        record["hardware_after_warmup"] = cached_report(lib, ctx)
        # 加速负控制只核回退后真实执行，不把它们混入CPU重复性。
        if args.backend == a.AM_BACKEND_CPU:
            record["rounds"] = [run_round(lib, ctx, model, image, args.samples, model_uid) for _ in range(3)]
            record["observation_off"] = run_round(lib, ctx, model, image, args.samples, model_uid, False)
        all_rows=warmup + [row for group in record.get("rounds",[]) for row in group["rows"]]
        all_rows += record.get("observation_off",{}).get("rows",[])
        if len({row["output_sha256"] for row in all_rows})!=1:
            raise RuntimeError("output changed between warmup/rounds/control")
        if len({row["request_id"] for row in all_rows})!=len(all_rows):
            raise RuntimeError("duplicate terminal request across windows")
        record["hardware_after"] = cached_report(lib, ctx)
        pointer = c.POINTER(a.am_stats_t)()
        checked(lib.amv_get_stats(ctx, None, c.byref(pointer)))
        try:
            stats = pointer.contents
            record["stats_after"] = {name:getattr(stats,name) for name in (
                "accepted", "infer_executed", "infer_failed", "threads_active", "threads_peak", "external_held_items")}
            if (stats.external_held_items or stats.threads_peak > 16 or stats.infer_failed
                    or stats.accepted != len(all_rows) or stats.infer_executed != len(all_rows)):
                raise RuntimeError("resource/terminal-state guard failed")
        finally:
            checked(lib.amv_stats_release(pointer))
        return record
    finally:
        try:
            if model:
                close(lib, model, lib.amv_model_close)
        finally:
            try:
                close(lib, ctx, lib.amv_shutdown)
            finally:
                directory.close()


def main(args):
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to((ROOT / ".local/evidence").resolve()):
        raise ValueError("use new output directory under .local/evidence")
    if not 1 <= args.samples <= 20:
        raise ValueError("sample/time budget exceeded")
    profile = json.loads((ROOT / "crates/vision/resources/profiles.json").read_text(encoding="utf-8"))["profiles"][0]
    if file_sha256(args.model) != profile["sampleSha256"]:
        raise ValueError("frozen model fixture required")
    output.mkdir(parents=True)
    files = {name:file_sha256(Path(__file__).with_name(name)) for name in (
        "benchmark-inference-resources.py", "perf_resources.py", "perf_common.py", "perf_met2.py")}
    identity = dict(schema="framebraid.performance.met2-inference/1", model_sha256=file_sha256(args.model),
        input_sha256=file_sha256(args.input), target_card_sha256=file_sha256(args.target_card),
        dll_sha256=file_sha256(args.dll_dir / "am_vision.dll"),
        runtime_files={p.name:file_sha256(p) for p in sorted(args.runtime_dir.glob("*.dll"))},
        observer_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        observer_files=files, samples=args.samples, request_rate=5,
        scope="CPU runtime real Pose; unavailable accelerator negative controls, no accelerated Run proof")
    (output / "identity.json").write_text(json.dumps(identity,indent=2)+"\n",encoding="utf-8")
    sources = output / "observer-sources"
    sources.mkdir()
    for name in files:
        (sources/name).write_bytes(Path(__file__).with_name(name).read_bytes())
    records=[]
    deadline=time.monotonic()+180
    for backend, strict in ((0,True), (1,True), (1,False), (2,True), (2,False)):
        command=[sys.executable,"-B",str(Path(__file__).resolve()),"--child", "--backend",str(backend),
            "--dll-dir",str(args.dll_dir.resolve()),"--runtime-dir",str(args.runtime_dir.resolve()),
            "--model",str(args.model.resolve()),"--input",str(args.input.resolve()),"--samples",str(args.samples)]
        if strict:
            command.append("--strict")
        try:
            run=subprocess.run(command,capture_output=True,text=True,timeout=min(90,max(1,deadline-time.monotonic())))
        except subprocess.TimeoutExpired as error:
            (output/"failure.json").write_text(json.dumps(dict(error="owned child timeout",backend=backend,strict=strict)),encoding="utf-8")
            raise TimeoutError("experiment stopped within process time budget") from error
        name=f"backend-{backend}-strict-{int(strict)}"
        (output/(name+".stderr.txt")).write_text(run.stderr,encoding="utf-8")
        (output/(name+".json")).write_text(run.stdout,encoding="utf-8")
        if run.returncode:
            raise RuntimeError(f"{name} failed; inspect preserved output")
        records.append(json.loads(run.stdout))
    cpu=records[0]["rounds"]
    review=dict(cpu_s=spread([r["cpu"]["cpu_s"] for r in cpu]),
                latency_p95_ms=spread([r["latency"]["p95_ms"] for r in cpu]),
                counts=[r["counts"] for r in cpu],
                verdict="CPU functional/resource baseline only; no speedup or accelerator claim")
    (output/"review.json").write_text(json.dumps(review,indent=2)+"\n",encoding="utf-8")
    print(json.dumps(dict(output=str(output), review=review),indent=2))


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ("dll-dir","runtime-dir","model","input"):
        parser.add_argument("--"+name,type=Path,required=True)
    parser.add_argument("--output",type=Path)
    parser.add_argument("--target-card",type=Path)
    parser.add_argument("--samples",type=int,default=20)
    parser.add_argument("--backend",type=int,choices=(0,1,2),default=0)
    parser.add_argument("--strict",action="store_true")
    parser.add_argument("--child",action="store_true")
    args=parser.parse_args()
    if args.child:
        record=child(args)
        print(json.dumps(record,indent=2))
    elif args.output and args.target_card and args.target_card.is_file():
        main(args)
    else:
        parser.error("parent requires output and an existing target card")
