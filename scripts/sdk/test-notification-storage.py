"""真实CPU模型回调注册预算与重复注销；合成输入，不启动现场服务。"""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import sys
import threading

sys.dont_write_bytecode = True
from camera_recovery_fixture import module
from local_paths import local_path

t = module("test-inference-pressure.py")
a, base, check, sized = t.a, t.base, t.check, t.sized


def main(args):
    out = local_path(args.output.resolve(), "evidence", fresh=True)
    out.mkdir(parents=True)
    target = {"metric": "MET-2 M-RECOVER S5", "cycles": 100,
              "one_control_item_registration": "resourceExhausted; no new thread permit",
              "repeated_registration": "one callback per result; constant thread permits",
              "after_last_model_close": "zero SDK inference thread permits",
              "scope": "CPU Identity and SDK ownership/queue contract, not GPU or soak"}
    (out / "target-card.json").write_text(json.dumps(target, indent=2), encoding="utf-8")
    dll = args.dll_dir.resolve() / "am_vision.dll"
    lib = c.CDLL(str(dll))
    a.bind(lib, "amv_")
    result = {"dll_sha256": hashlib.sha256(dll.read_bytes()).hexdigest(), "cycles": []}
    entered, calls = threading.Event(), []

    @a.am_result_notify_fn
    def callback(model, rid, state, user):
        calls.append(rid)
        entered.set()

    for limited in [True, False]:
        ctx, model = base.initialize(lib, args.runtime_dir), None
        try:
            model = t.t.load(lib, ctx, args.tensor_dir / "image.onnx")
            if limited:
                policy = sized(a.am_queue_policy_t)
                check(lib.amv_get_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
                policy.max_items = 1
                check(lib.amv_set_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
                before = t.snapshot(lib, ctx)
                status = lib.amv_result_notify_callback_set(model, callback, None)
                after = t.snapshot(lib, ctx)
                result["one_item"] = {"status": status, "threads_before": before["threads_active"],
                                      "threads_after": after["threads_active"]}
                if not args.observe_baseline:
                    check(status, a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
                    assert before["threads_active"] == after["threads_active"]
                check(lib.amv_result_notify_callback_set(model, a.am_result_notify_fn(), None))
            else:
                for index in range(100):
                    entered.clear()
                    count = len(calls)
                    check(lib.amv_result_notify_callback_set(model, callback, None))
                    measured = t.healthy(lib, ctx, model, 1)
                    assert entered.wait(2)
                    check(lib.amv_result_notify_callback_set(model, a.am_result_notify_fn(), None))
                    assert len(calls) == count + 1
                    snap = t.snapshot(lib, ctx)
                    result["cycles"].append({"cycle": index, "threads": snap["threads_active"],
                                              "infer_call_ms": measured["max_ms"]})
                assert len({r["threads"] for r in result["cycles"]}) == 1
                count = len(calls)
                t.healthy(lib, ctx, model, 5)
                assert len(calls) == count
        finally:
            if model:
                base.close_model(lib, model)
                assert t.snapshot(lib, ctx)["threads_active"] == 0
            t.shared.close(lib, ctx, False)
    result["model_sha256"] = hashlib.sha256((args.tensor_dir / "image.onnx").read_bytes()).hexdigest()
    (out / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps({"one_item": result["one_item"], "cycles": len(result["cycles"])}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ["dll-dir", "runtime-dir", "tensor-dir", "output"]:
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--observe-baseline", action="store_true")
    main(parser.parse_args())
