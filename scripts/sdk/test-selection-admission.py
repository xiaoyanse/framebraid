"""受限线程预算下的选择、严格失败与额度归还；同一CPU ORT、无现场设备。"""
import argparse
import ctypes as c
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("selected", Path(__file__).with_name("test-inference-selection.py"))
s = importlib.util.module_from_spec(spec)
spec.loader.exec_module(s)
a, t, check = s.a, s.t, s.check


def run_failure_and_explicit_rebuild(lib, ctx, failed_path, good_path, output):
    model, _ = s.load(lib, ctx, failed_path, [(0, 0)])
    try:
        old = s.sized(a.am_model_info_t)
        check(lib.amv_model_get_info(model, c.byref(old)))
        old_uid = old.identity.model_uid
        tensor = t.Tensor("x", a.AM_DTYPE_F32, [1], bytes(4))
        array = (a.am_tensor_in_t * 1)(tensor.view)
        result, rid = c.POINTER(a.amv_result_t)(), c.c_uint64()
        check(lib.amv_infer_tensors_sync(model, 1, array, None, c.byref(result), c.byref(rid)),
              a.AM_ERR_VISION_INFERENCE_FAILED)
        assert rid.value and not result
        terminal = []
        for _ in range(2):
            snapshot = c.POINTER(a.am_request_snapshot_t)()
            check(lib.amv_request_query(ctx, rid.value, c.byref(snapshot)))
            try:
                assert snapshot.contents.state == a.AM_REQ_FAILED and snapshot.contents.terminal == 1
                assert snapshot.contents.error.contents.code == a.AM_ERR_VISION_INFERENCE_FAILED
                terminal.append(snapshot.contents.finished_at_ms)
            finally:
                check(lib.amv_request_snapshot_release(snapshot))
            time.sleep(0.02)
        assert terminal[0] == terminal[1]
        failure = s.report(lib, model)["selection"]
        assert failure["native_failures"] == 1 and failure["native_runs"] == 0
        assert failure["last_native_run_succeeded"] is False
        assert failure["selected"]["backend"] == 0 and len(failure["attempts"]) == 1
        check(lib.amv_result_ack(ctx, rid.value))
    finally:
        t.base.close_model(lib, model)
    model, _ = s.load(lib, ctx, good_path, [(0, 0)])
    try:
        info = s.sized(a.am_model_info_t)
        check(lib.amv_model_get_info(model, c.byref(info)))
        assert old_uid != info.identity.model_uid
        result, request = s.infer(lib, ctx, model)
        assert t.output(result) == {"pixels": (a.AM_DTYPE_F32, [1, 3, 2, 2], bytes(48))}
        t.release(lib, ctx, result, request)
        recovery = s.report(lib, model)["selection"]
        assert recovery["native_runs"] == 1 and recovery["native_failures"] == 0
    finally:
        t.base.close_model(lib, model)
    invalid = output / "invalid-model.onnx"
    invalid.write_bytes(b"invalid synthetic ONNX")
    _, rid = s.load(lib, ctx, invalid, [(0, 0), (2, 0)], expected=a.AM_ERR_VISION_MODEL_LOAD_FAILED)
    assert not s.report(lib, ctx, rid)["selection"]["attempts"], "模型错误不能靠切换设备掩盖"
    return {"failure": failure, "recovery": recovery, "unique_terminal": terminal[0],
            "explicit_new_model_uid": True, "invalid_model_stops_before_admission": True}


def main(args):
    root = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    assert output.is_relative_to((root / ".local").resolve())
    output.mkdir(parents=True, exist_ok=True)
    dll = args.dll_dir.resolve() / "am_vision.dll"
    lib = c.CDLL(str(dll))
    a.bind(lib, "amv_", require_all=True)
    p = s.sized(a.amv_init_params_t)
    p.abi_version, p.infer_threads, p.total_thread_budget = a.AM_ABI_VERSION, 2, 3
    directories = (c.c_char_p * 1)(str(args.runtime_dir.resolve()).encode())
    p.runtime_dirs_utf8, p.runtime_dir_count = directories, 1
    ctx = c.POINTER(a.amv_context_t)()
    check(lib.amv_init(c.byref(p), c.byref(ctx)))
    path = args.sample_dir / "image.onnx"
    attempts, recovery = [], None
    try:
        for round_id in range(3):
            resources = None
            at = time.perf_counter_ns()
            model, rid = s.load(lib, ctx, path, [(1, 0), (0, 0)],
                                expected=a.AM_ERR_SDK_RESOURCE_EXHAUSTED if args.baseline else 0)
            duration = (time.perf_counter_ns() - at) / 1e6
            value = s.report(lib, ctx, rid)
            if not args.baseline:
                try:
                    selected = s.report(lib, model)["selection"]
                    assert selected["selected"] == {"backend": 0, "device_id": 0}
                    assert selected["attempts"][0]["stage"] == "resource_admission"
                    assert selected["attempts"][0]["error"] == "sdk.resourceExhausted"
                    assert len(selected["attempts"]) == 2
                    result, request = s.infer(lib, ctx, model)
                    assert t.output(result) == {"pixels": (a.AM_DTYPE_F32, [1, 3, 2, 2], bytes(48))}
                    t.release(lib, ctx, result, request)
                    stats = c.POINTER(a.am_stats_t)()
                    check(lib.amv_get_stats(ctx, None, c.byref(stats)))
                    try:
                        assert stats.contents.threads_active == 3 and stats.contents.threads_peak <= 3
                        resources = {"threads_active": stats.contents.threads_active,
                                     "threads_peak": stats.contents.threads_peak}
                    finally:
                        check(lib.amv_stats_release(stats))
                    # 一个模型占满额度时，下一CPU模型也必须拒绝，不能放松全局总额。
                    _, failed_id = s.load(lib, ctx, path, [(0, 0)], expected=a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
                    assert s.report(lib, ctx, failed_id)["state"] == "failed"
                finally:
                    t.base.close_model(lib, model)
            attempts.append({"round": round_id, "load_ms": duration, "report": value, "resources": resources})
        if not args.baseline:
            for fallback, candidates in [(False, [(1, 0), (0, 0)]), (True, [(1, 0)])]:
                _, rid = s.load(lib, ctx, path, candidates, fallback,
                                expected=a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
                assert len(s.report(lib, ctx, rid)["selection"]["attempts"]) == 1
            model, _ = s.load(lib, ctx, path, [(0, 0)])
            t.base.close_model(lib, model)
            legacy = s.sized(a.am_model_desc_t)
            old_policy = s.sized(a.am_backend_policy_t)
            old_policy.requested, old_policy.strict = a.AM_BACKEND_OPENVINO, 0
            legacy.path_utf8, legacy.load_mode, legacy.intra_threads = str(path.resolve()).encode(), a.AM_LOAD_RAW, 2
            legacy.backend = c.pointer(old_policy)
            old_model = c.POINTER(a.amv_model_t)()
            check(lib.amv_model_load(ctx, c.byref(legacy), c.byref(old_model)), a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
            assert not old_model, "legacy入口不得被重新定义"
            if args.failure_model:
                recovery = run_failure_and_explicit_rebuild(lib, ctx, args.failure_model, path, output)
    finally:
        check(lib.amv_shutdown(ctx, 1000, c.byref(c.c_uint32())))
        check(lib.amv_object_destroy(ctx))
    record = {"baseline_expected_rejection": args.baseline, "budget": {"total": 3, "parallel": 2, "intra": 2},
              "dll_sha256": hashlib.sha256(dll.read_bytes()).hexdigest(), "rounds": attempts, "recovery": recovery}
    (output / "result.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"baseline_expected_rejection": args.baseline, "rounds": len(attempts), "passed": True}))


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--dll-dir", type=Path, required=True)
    p.add_argument("--runtime-dir", type=Path, required=True)
    p.add_argument("--sample-dir", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--baseline", action="store_true")
    p.add_argument("--failure-model", type=Path)
    main(p.parse_args())
