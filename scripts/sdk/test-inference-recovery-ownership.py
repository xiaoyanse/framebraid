"""R3：真实CPU结果过期/外部持有、并发fetch/release与原生批次取消屏障。"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import ctypes as c
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import sys
import time

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("batch", Path(__file__).with_name("test-batch.py"))
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)
a, t, base, sized, check = b.a, b.t, b.base, b.size, b.check


def expiry(lib, runtime, sample_dir):
    ctx = base.initialize(lib, runtime)
    model, held = None, c.POINTER(a.amv_result_t)()
    payload = struct.pack("<12f", *range(12))
    try:
        model = t.load(lib, ctx, sample_dir / "image.onnx")
        def infer():
            return t.invoke(lib, ctx, model, [t.Tensor("images", a.AM_DTYPE_F32, [1, 3, 2, 2], payload)])
        initial, rid = infer()
        expected = t.output(initial)
        assert expected == {"pixels": (a.AM_DTYPE_F32, [1, 3, 2, 2], payload)}
        check(lib.amv_result_release(initial))
        def fetch(_):
            for _ in range(100):
                result = c.POINTER(a.amv_result_t)()
                check(lib.amv_result_fetch(ctx, rid, c.byref(result)))
                try:
                    assert t.output(result) == expected
                finally:
                    check(lib.amv_result_release(result))
        with ThreadPoolExecutor(max_workers=4) as pool:
            list(pool.map(fetch, range(4)))
        check(lib.amv_result_ack(ctx, rid))
        policy = sized(a.am_queue_policy_t)
        check(lib.amv_get_queue_policy(ctx, a.AM_Q_RESULTS, c.byref(policy)))
        policy.ttl_ms = 40
        check(lib.amv_set_queue_policy(ctx, a.AM_Q_RESULTS, c.byref(policy)))
        held, rid = infer()
        time.sleep(0.15)
        snapshot = c.POINTER(a.am_request_snapshot_t)()
        check(lib.amv_request_query(ctx, rid, c.byref(snapshot)), a.AM_ERR_SDK_RESULT_EXPIRED)
        assert not snapshot and t.output(held) == expected
        state = c.c_uint32()
        check(lib.amv_model_close(model, 1000, c.byref(state)))
        assert state.value == a.AM_LIFECYCLE_CLOSED
        check(lib.amv_object_query_state(model, c.byref(state)))
        assert state.value == a.AM_LIFECYCLE_CLOSED
        tensor = t.Tensor("images", a.AM_DTYPE_F32, [1, 3, 2, 2], payload)
        array = (a.am_tensor_in_t * 1)(tensor.view)
        rejected = c.c_uint64(123)
        check(lib.amv_infer_tensors_async(model, 1, array, None, c.byref(rejected)), a.AM_ERR_SDK_HANDLE_CLOSED)
        assert rejected.value == 0
        check(lib.amv_model_close(model, 1000, c.byref(state)))
        check(lib.amv_object_destroy(model))
        model = None
        assert t.output(held) == expected
        check(lib.amv_shutdown(ctx, 0, c.byref(state)))
        assert state.value == a.AM_LIFECYCLE_CLOSING
        assert t.output(held) == expected
        check(lib.amv_result_release(held))
        held = c.POINTER(a.amv_result_t)()
        base.shared.close(lib, ctx, False)
        ctx = None
        return {"parallel_fetch_release": 400, "result_ttl_ms": 40, "expired_request_reported": True,
                "external_pointer_survives_ttl_model_close_and_context_closing": True,
                "context_drains_after_release": True, "output_sha256": hashlib.sha256(payload).hexdigest()}
    finally:
        if held:
            check(lib.amv_result_release(held))
        if model:
            base.close_model(lib, model)
        if ctx:
            base.shared.close(lib, ctx, False)


def main(args):
    from local_paths import local_path
    out = local_path(args.output.resolve(), "evidence", fresh=True)
    out.mkdir(parents=True)
    dll = args.dll_dir.resolve() / "am_vision.dll"
    lib = c.CDLL(str(dll))
    a.bind(lib, "amv_", require_all=True)
    result = {"expiry": expiry(lib, args.runtime_dir, args.tensor_dir),
              "native_cancel": b.native_cancel(lib, args.runtime_dir, args.batch_dir),
              "identity": {"dll_sha256": hashlib.sha256(dll.read_bytes()).hexdigest(),
                           "image_model_sha256": hashlib.sha256((args.tensor_dir / "image.onnx").read_bytes()).hexdigest(),
                           "batch_model_sha256": hashlib.sha256((args.batch_dir / "image-dynamic.onnx").read_bytes()).hexdigest()},
              "scope": "actual generic CPU Identity/batch and ownership; no profile-family accuracy/hardware/soak claim"}
    spec = importlib.util.spec_from_file_location("lifecycle", Path(__file__).with_name("test-module-lifecycle.py"))
    lifecycle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(lifecycle)
    assert not lifecycle.workers(lifecycle.windows()), "inference worker survived model/context draining"
    result["inference_workers_after_drain"] = 0
    (out / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps(result))


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--dll-dir", type=Path, required=True)
    p.add_argument("--runtime-dir", type=Path, required=True)
    p.add_argument("--tensor-dir", type=Path, required=True)
    p.add_argument("--batch-dir", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    main(p.parse_args())
