"""初始化失败/显式恢复、纯共享帧及后端证据分层；每场景独立进程。"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("tensor_test", Path(__file__).with_name("test-tensors.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
a, base, size, check = t.a, t.base, t.size, t.check


def params(directory=None, shared=False):
    p = size(a.amv_init_params_t)
    p.abi_version, p.infer_threads = a.AM_ABI_VERSION, 2
    p.feature_flags = a.AM_INIT_SHARED_FRAMES if shared else 0
    if directory is not None:
        dirs = (c.c_char_p*1)(str(directory.resolve()).encode())
        p.runtime_dirs_utf8, p.runtime_dir_count = dirs, 1
        p._owner = dirs
    return p


def init(vision, p, expected=0):
    ctx = c.POINTER(a.amv_context_t)()
    check(vision.amv_init(c.byref(p), c.byref(ctx)), expected)
    assert bool(ctx) == (expected == 0)
    return ctx


def matrix(vision, ctx):
    value = size(a.am_backend_matrix_t)
    check(vision.amv_query_backends(ctx, c.byref(value)))
    assert value.row_count == 3
    rows = [{name: getattr(row, name) for name in ["backend", "hw_detected", "deps_present", "provider_loadable", "verified"]}
            | {"note": row.note_utf8.decode()} for row in value.rows[:3]]
    assert [r["backend"] for r in rows] == [0, 1, 2]
    return rows


def recover(vision, args, bad):
    error = a.AM_ERR_VISION_RUNTIME_MISSING if args.child == "missing" else a.AM_ERR_VISION_RUNTIME_LOAD_FAILED
    first = params(None if args.child == "missing" else bad)
    init(vision, first, error)
    failure = size(a.am_init_error_t)
    check(vision.amv_get_init_error(c.byref(failure)))
    assert failure.code == error and failure.attempt_seq > 0
    assert failure.failed_at_ms > 0
    init(vision, params(args.runtime_dir), error)
    # 故障未修复时纯共享帧仍独立可用，但不能用它绕过推理恢复闸门。
    shared = init(vision, params(shared=True))
    assert all(not r["deps_present"] and not r["provider_loadable"] for r in matrix(vision, shared))
    check(vision.amv_backend_retry(c.byref(params(args.runtime_dir))), a.AM_ERR_SDK_INVALID_ARGUMENT)
    base.shared.close(vision, shared, False)
    good = params(args.runtime_dir)
    check(vision.amv_backend_retry(c.byref(good)))
    check(vision.amv_get_init_error(c.byref(failure)))
    assert failure.code == 0 and failure.failed_at_ms == -(1 << 63)
    ctx = init(vision, good)
    before = matrix(vision, ctx)
    assert before[0]["deps_present"] == 1 and before[0]["verified"] == 0
    model = t.load(vision, ctx, args.sample_dir/"image.onnx")
    loaded = matrix(vision, ctx)
    assert loaded[0]["provider_loadable"] == 1 and loaded[0]["verified"] == 0
    tensor = t.Tensor("images", a.AM_DTYPE_F32, [1, 3, 2, 2], bytes(48))
    result, rid = t.invoke(vision, ctx, model, [tensor])
    t.release(vision, ctx, result, rid)
    after = matrix(vision, ctx)
    assert after[0]["provider_loadable"] == after[0]["verified"] == 1
    assert all(not r["verified"] for r in after[1:])
    # 配置指向另一路运行库不能污染已工作的ctx，也不能悄悄换库。
    init(vision, params(bad), a.AM_ERR_VISION_BACKEND_CONFLICT)
    check(vision.amv_backend_retry(c.byref(good)), a.AM_ERR_SDK_INVALID_ARGUMENT)
    base.close_model(vision, model)
    state = c.c_uint32()
    check(vision.amv_shutdown(ctx, 0, c.byref(state)))
    assert state.value == a.AM_LIFECYCLE_CLOSED
    check(vision.amv_backend_retry(c.byref(good)), a.AM_ERR_SDK_INVALID_ARGUMENT)
    check(vision.amv_object_destroy(ctx))
    check(vision.amv_backend_retry(c.byref(good)))
    return {"initialFailure": error, "explicitRetryRecovered": True, "sharedOnlyWithoutOrt": True,
            "activeAndClosedUndestroyedContextRejectRetry": True, "cpuProofStages": [before[0], loaded[0], after[0]],
            "accelerationRows": after[1:], "missingVision": a.bind(vision, "amv_")}


def child(args):
    bad = args.output/args.child
    bad.mkdir(parents=True, exist_ok=True)
    (bad/"onnxruntime.dll").write_bytes(b"AM-RT invalid runtime test; not executable")
    vision = c.CDLL(str((args.dll_dir/"am_vision.dll").resolve()))
    a.bind(vision, "amv_")
    if args.child == "external":
        runtime = c.CDLL(str((args.runtime_dir/"onnxruntime.dll").resolve()))
        init(vision, params(bad), a.AM_ERR_VISION_BACKEND_CONFLICT)
        good = params(args.runtime_dir)
        check(vision.amv_backend_retry(c.byref(good)))
        ctx = init(vision, good)
        base.shared.close(vision, ctx, False)
        assert runtime._handle
        return {"externalRuntimeConflictRejected": True, "matchingExplicitRuntimeAccepted": True}
    return recover(vision, args, bad)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--sample-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--child", choices=["missing", "malformed", "external"])
    args = parser.parse_args()
    if args.child:
        print(json.dumps(child(args)))
    else:
        records = []
        for scenario in ["missing", "malformed", "external"]:
            result = subprocess.run([sys.executable, str(Path(__file__).resolve()), *sys.argv[1:], "--child", scenario],
                                    capture_output=True, text=True, timeout=30)
            assert result.returncode == 0, (scenario, result.returncode, result.stderr)
            records.append(json.loads(result.stdout))
        print(json.dumps({"scenarios": records}))
