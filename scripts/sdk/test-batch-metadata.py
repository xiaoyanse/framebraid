"""本阶段验证批量轴元数据与普通张量入口；不冒充尚未导出的batch回执API。"""
import argparse
import ctypes as c
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import sys
sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("tensor_test", Path(__file__).with_name("test-tensors.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
a, base, check, size = t.a, t.base, t.check, t.size


def inputs(count):
    values = [float(i+channel) for i in range(count) for channel in range(3)]
    return [t.Tensor("float", a.AM_DTYPE_F32, [count, 3], struct.pack("<"+"f"*len(values), *values)),
            t.Tensor("half", a.AM_DTYPE_F16, [count, 2], struct.pack("<ee", 1, -2)*count),
            t.Tensor("long", a.AM_DTYPE_I64, [count], struct.pack("<q", 123)*count),
            t.Tensor("int", a.AM_DTYPE_I32, [count, 2, 2], struct.pack("<iiii", -1, 2, 3, 4)*count),
            t.Tensor("byte", a.AM_DTYPE_U8, [count, 3], b"\x01\x00\xff"*count)]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--sample-dir", type=Path, required=True)
    args = parser.parse_args()
    for record in json.loads((args.sample_dir/"manifest.json").read_text())["files"]:
        assert hashlib.sha256((args.sample_dir/record["file"]).read_bytes()).hexdigest() == record["sha256"]
    vision = c.CDLL(str((args.dll_dir/"am_vision.dll").resolve()))
    missing = a.bind(vision, "amv_")
    assert ("amv_infer_batch" in missing) == ("amv_receipt_release" in missing)
    ctx = base.initialize(vision, args.runtime_dir)
    results = []
    for name, batch, count in [("dynamic.onnx", 1, 3), ("fixed4.onnx", 4, 4), ("unproved.onnx", 0, 65)]:
        model = t.load(vision, ctx, args.sample_dir/name)
        info = size(a.am_model_info_t)
        check(vision.amv_model_get_info(model, c.byref(info)))
        assert info.native_batch == batch
        if batch == 1 or batch == 0:
            for i in range(info.input_count):
                assert info.inputs[i].dyn_max[0] == (64 if batch else 8192)
        result, rid = t.invoke(vision, ctx, model, inputs(count), True)
        actual = t.output(result)["out_float"]
        floats = struct.unpack("<"+"f"*(count*3), actual[2])
        assert actual[:2] == (a.AM_DTYPE_F32, [count, 3])
        assert all(abs(value-(i//3-(count-1)/2)) < 0.00001 for i, value in enumerate(floats))
        t.release(vision, ctx, result, rid)
        if batch:
            rejected = inputs(65 if batch == 1 else 3)
            array = (a.am_tensor_in_t*len(rejected))(*(v.view for v in rejected))
            rid = c.c_uint64(777)
            code = vision.amv_infer_tensors_async(model, len(array), array, None, c.byref(rid))
            check(code, a.AM_ERR_VISION_DYNAMIC_RANGE_EXCEEDED if batch == 1 else a.AM_ERR_VISION_INPUT_SHAPE_MISMATCH)
            assert rid.value == 0
        base.close_model(vision, model)
        results.append({"file": name, "nativeBatch": batch, "executedCount": count})
    for filename in ["wrong-axis.onnx", "bad-output.onnx"]:
        desc = size(a.am_model_desc_t)
        desc.path_utf8, desc.load_mode = str((args.sample_dir/filename).resolve()).encode(), a.AM_LOAD_RAW
        model = c.POINTER(a.amv_model_t)()
        check(vision.amv_model_load(ctx, c.byref(desc), c.byref(model)), a.AM_ERR_VISION_BATCH_UNSUPPORTED)
        assert not model
    base.shared.close(vision, ctx, False)
    print(json.dumps({"models": results, "invalidDeclarationRejected": True,
                     "batchExportsAvailable": "amv_infer_batch" not in missing,
                     "scope": "batch proof and whole-tensor execution; grouped DLL receipts tested separately"}))


if __name__ == "__main__":
    main()
