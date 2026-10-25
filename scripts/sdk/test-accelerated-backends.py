"""显式目录、独立进程验证CPU与单加速后端顺序/回退；不复制运行库。"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("backend_test", Path(__file__).with_name("test-backends.py"))
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)
t, a, base, size, check = b.t, b.a, b.base, b.size, b.check


def load(vision, ctx, path, backend, strict=True, expected=0):
    policy = size(a.am_backend_policy_t)
    policy.requested, policy.strict = backend, int(strict)
    desc = size(a.am_model_desc_t)
    desc.path_utf8, desc.load_mode, desc.intra_threads = str(path.resolve()).encode(), a.AM_LOAD_RAW, 1
    desc.backend = c.pointer(policy)
    model = c.POINTER(a.amv_model_t)()
    check(vision.amv_model_load(ctx, c.byref(desc), c.byref(model)), expected)
    assert bool(model) == (expected == 0)
    return model


def infer(vision, ctx, model, actual):
    info = size(a.am_model_info_t)
    check(vision.amv_model_get_info(model, c.byref(info)))
    assert info.execution.actual == actual
    assert info.execution.subgraph_cpu_nodes == (0 if actual == 0 else -1)
    values = struct.pack("<f", 1.0)*(1536*1536)
    tensor = t.Tensor("matrix", a.AM_DTYPE_F32, [1536, 1536], values)
    result, rid = t.invoke(vision, ctx, model, [tensor])
    raw = t.output(result)["product"][2]
    assert all(value == 1536.0 for value in memoryview(raw).cast("f"))
    assert result.contents.execution.actual == actual
    fallback = result.contents.execution.fallback_reason_utf8
    t.release(vision, ctx, result, rid)
    return {"actual": actual, "subgraphCpuNodes": info.execution.subgraph_cpu_nodes,
            "all2359296ValuesExact": True, "fallback": None if not fallback else fallback.decode()}


def child(args):
    vision = c.CDLL(str((args.dll_dir/"am_vision.dll").resolve()))
    a.bind(vision, "amv_")
    ctx = b.init(vision, b.params(args.runtime_dir))
    path = args.sample_dir/"matmul.onnx"
    if args.order == "missing":
        load(vision, ctx, path, args.backend, True, a.AM_ERR_VISION_BACKEND_STRICT_UNAVAILABLE)
        model = load(vision, ctx, path, args.backend, False)
        result = infer(vision, ctx, model, 0)
        assert result["fallback"]
        base.close_model(vision, model)
        base.shared.close(vision, ctx, False)
        return {"order": args.order, "strictRejected": True, "fallback": result}
    order = [0, args.backend] if args.order == "cpu-first" else [args.backend, 0]
    models, results = [], []
    for backend in order:
        model = load(vision, ctx, path, backend)
        models.append(model)
        results.append(infer(vision, ctx, model, backend))
    rows = b.matrix(vision, ctx)
    assert rows[0]["verified"] == rows[args.backend]["verified"] == 1
    other = 3-args.backend
    load(vision, ctx, path, other, False, a.AM_ERR_VISION_BACKEND_CONFLICT)
    for model in models:
        base.close_model(vision, model)
    load(vision, ctx, path, other, True, a.AM_ERR_VISION_BACKEND_CONFLICT)
    base.shared.close(vision, ctx, False)
    return {"order": args.order, "results": results, "rows": rows, "crossAccelerationConflictRejected": True}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--sample-dir", type=Path, required=True)
    parser.add_argument("--backend", type=int, choices=[1, 2], required=True)
    parser.add_argument("--missing-only", action="store_true")
    parser.add_argument("--order", choices=["cpu-first", "accelerator-first", "missing"])
    args = parser.parse_args()
    if args.order:
        print(json.dumps(child(args)))
    else:
        records = []
        for order in (["missing"] if args.missing_only else ["cpu-first", "accelerator-first"]):
            result = subprocess.run([sys.executable, str(Path(__file__).resolve()), *sys.argv[1:], "--order", order],
                                    capture_output=True, text=True, timeout=90)
            assert result.returncode == 0, (order, result.returncode, result.stderr[-4000:])
            records.append(json.loads(result.stdout))
        print(json.dumps({"backend": args.backend, "orders": records}))
