"""显式本机驱动发现、纯缓存查询、控制额度和关闭；不冒充加速ORT验收。"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import statistics
import sys
import threading
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
import amrt_sdk as a
from safe_sdk import VisionContext


def check(code, expected=0):
    assert code == expected, (code, expected)


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def snapshot(lib, ctx):
    count = c.c_uint64()
    check(lib.amv_get_inference_hardware_report(ctx, None, 0, c.byref(count)))
    assert 0 < count.value <= 65536
    short = c.create_string_buffer(b"x", 1)
    check(lib.amv_get_inference_hardware_report(ctx, short, 1, c.byref(count)), a.AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
    assert short.raw == b"\0"
    data = c.create_string_buffer(65552)
    c.memset(c.addressof(data) + 65536, 0x7A, 16)
    check(lib.amv_get_inference_hardware_report(ctx, data, 65536, c.byref(count)))
    assert data.raw[65536:] == b"z" * 16
    assert data.raw[count.value - 1] == 0
    value = json.loads(data.value)
    assert value["schema"] == "framebraid.inference-hardware/1"
    return value


def main(args):
    output = args.output.resolve()
    assert output.is_relative_to((ROOT / ".local").resolve())
    output.mkdir(parents=True, exist_ok=True)
    lib = c.CDLL(str((args.dll_dir / "am_vision.dll").resolve()))
    a.bind(lib, "amv_", require_all=True)
    params = sized(a.amv_init_params_t)
    params.abi_version, params.infer_threads, params.feature_flags = 0x10000, 2, 1
    ctx = c.POINTER(a.amv_context_t)()
    check(lib.amv_init(c.byref(params), c.byref(ctx)))
    original = sized(a.am_queue_policy_t)
    check(lib.amv_get_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(original)))
    try:
        initial = snapshot(lib, ctx)
        assert initial["state"] == "not_probed" and initial["device_probe"] is None
        assert not initial["runtime_initialized"]
        check(lib.amv_probe_inference_hardware(ctx, 0), a.AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
        check(lib.amv_probe_inference_hardware(ctx, 5001), a.AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
        low = sized(a.am_queue_policy_t)
        c.memmove(c.byref(low), c.byref(original), c.sizeof(low))
        low.max_bytes = 32
        small = c.POINTER(a.amv_context_t)()
        check(lib.amv_init(c.byref(params), c.byref(small)))
        try:
            check(lib.amv_set_queue_policy(small, a.AM_Q_CONTROL, c.byref(low)))
            check(lib.amv_probe_inference_hardware(small, 1500), a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
        finally:
            check(lib.amv_shutdown(small, 1000, c.byref(c.c_uint32())))
            check(lib.amv_object_destroy(small))
        check(lib.amv_probe_inference_hardware(ctx, 1500))
        report = snapshot(lib, ctx)
        assert report["state"] == "probe_complete"
        probe = report["device_probe"]
        assert 1 <= probe["cpu_logical_parallelism"] and len(probe["cuda_devices"]) <= 16
        assert report["observations"]["executed_mask"] == 0
        assert not report["runtime_initialized"], "probe不得隐式初始化ORT"
        times = []
        for _ in range(100):
            start = time.perf_counter_ns()
            again = snapshot(lib, ctx)
            times.append((time.perf_counter_ns() - start) / 1e6)
            assert again == report, "普通查询不能重新探测或更新时间"
        overlap = (c.c_uint64 * 8192)()
        check(lib.amv_get_inference_hardware_report(ctx, c.cast(overlap, c.c_char_p), 65536,
              c.cast(overlap, c.POINTER(c.c_uint64))), a.AM_ERR_SDK_INVALID_ARGUMENT)
        check(lib.amv_get_inference_hardware_report(ctx, None, 1, c.byref(c.c_uint64())), a.AM_ERR_SDK_NULL_ARGUMENT)
        # 新一轮余额不足必须撤销旧成功，不留下看似新鲜的旧结果。
        check(lib.amv_set_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(low)))
        check(lib.amv_probe_inference_hardware(ctx, 1500), a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
        # 队列额度按现有合同只能降低，不能为了测试把额度调大；关闭态查询仍必须可用。
        check(lib.amv_get_inference_hardware_report(ctx, None, 0, c.byref(c.c_uint64())), a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
    finally:
        state = c.c_uint32()
        check(lib.amv_shutdown(ctx, 1000, c.byref(state)))
        assert state.value == a.AM_LIFECYCLE_CLOSED, "内部缓存不能阻塞关闭"
        assert snapshot(lib, ctx)["state"] == "closed"
        check(lib.amv_probe_inference_hardware(ctx, 1500), a.AM_ERR_SDK_HANDLE_CLOSED)
        check(lib.amv_object_destroy(ctx))
    with VisionContext(args.dll_dir / "am_vision.dll", shared_frames=True) as safe:
        assert safe.get_inference_hardware_report()["state"] == "not_probed"
        assert safe.probe_inference_hardware()["state"] == "probe_complete"
    check(lib.amv_init(c.byref(params), c.byref(ctx)))
    check(lib.amv_probe_inference_hardware(ctx, 1500))
    started, errors = threading.Event(), []

    def reader():
        try:
            for _ in range(200):
                snapshot(lib, ctx)
                started.set()
        except BaseException as error:
            errors.append(str(error))
            started.set()

    thread = threading.Thread(target=reader)
    thread.start()
    assert started.wait(2)
    check(lib.amv_shutdown(ctx, 1000, c.byref(c.c_uint32())))
    thread.join(2)
    assert not thread.is_alive() and not errors, errors
    assert snapshot(lib, ctx)["state"] == "closed"
    check(lib.amv_object_destroy(ctx))
    stages = []
    if args.runtime_dir and args.sample_dir:
        spec = importlib.util.spec_from_file_location("tensor_test", Path(__file__).with_name("test-tensors.py"))
        tensor = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(tensor)
        paths = (c.c_char_p * 1)(str(args.runtime_dir.resolve()).encode())
        params.runtime_dirs_utf8, params.runtime_dir_count = paths, 1
        check(lib.amv_init(c.byref(params), c.byref(ctx)))
        model = None
        try:
            stages.append(snapshot(lib, ctx)["observations"])
            model = tensor.load(lib, ctx, args.sample_dir / "image.onnx")
            stages.append(snapshot(lib, ctx)["observations"])
            value = tensor.Tensor("images", a.AM_DTYPE_F32, [1, 3, 2, 2], bytes(48))
            result, request = tensor.invoke(lib, ctx, model, [value])
            tensor.release(lib, ctx, result, request)
            stages.append(snapshot(lib, ctx)["observations"])
            assert stages[0]["loaded_mask"] == stages[0]["executed_mask"] == 0
            assert stages[1]["loaded_mask"] == 1 and stages[1]["executed_mask"] == 0
            assert stages[2]["loaded_mask"] == stages[2]["executed_mask"] == 1
        finally:
            if model:
                tensor.base.close_model(lib, model)
            check(lib.amv_shutdown(ctx, 1000, c.byref(c.c_uint32())))
            check(lib.amv_object_destroy(ctx))
    summary = {"scope": "real driver inventory and optional CPU proof; no accelerated ORT execution", "native": report,
               "query_triple_call_p50_ms": statistics.median(times),
               "query_triple_call_p95_ms": sorted(times)[94],
               "queries": 100, "cpu_proof_stages": stages,
               "buffer_budget_close_python": "passed"}
    (output / "result.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in summary.items() if k != "native"}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path)
    parser.add_argument("--sample-dir", type=Path)
    main(parser.parse_args())

