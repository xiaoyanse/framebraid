"""真实DLL命名张量入口：五dtype、多IO、标量、复制所有权、档案与背压。"""
import argparse
import ctypes as c
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import sys
sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("inference_test", Path(__file__).with_name("test-inference.py"))
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
a, size, check = base.a, base.size, base.check


class Tensor:
    def __init__(self, name, dtype, shape, data, assoc=None):
        self.name = name.encode()
        self.shape = (c.c_int64*len(shape))(*shape)
        # 故意使用未对齐的原始字节；SDK须复制到自有对齐缓冲后交给ORT。
        self.storage = (c.c_ubyte*(len(data)+1))()
        c.memmove(c.byref(self.storage, 1), data, len(data))
        self.view = a.am_tensor_in_t()
        self.view.name, self.view.dtype = self.name, dtype
        self.view.shape, self.view.shape_len = self.shape, len(shape)
        self.view.data, self.view.data_size = c.addressof(self.storage)+1, len(data)
        self.view.flags = a.AM_TENSOR_CONTIGUOUS
        if assoc is not None:
            self.view.assoc = c.pointer(assoc)

    def clear(self):
        c.memset(self.storage, 0, c.sizeof(self.storage))
        c.memset(self.shape, 0, c.sizeof(self.shape))


def numeric(rows=2):
    values = [("float", a.AM_DTYPE_F32, [rows, 3], struct.pack("<"+"f"*(rows*3), *range(rows*3))),
              ("half", a.AM_DTYPE_F16, [2], struct.pack("<ee", 0.5, -3.25)),
              ("long", a.AM_DTYPE_I64, [], struct.pack("<q", 123456789123)),
              ("int", a.AM_DTYPE_I32, [2, 2], struct.pack("<iiii", -7, 0, 8, 2147483647)),
              ("byte", a.AM_DTYPE_U8, [3], bytes([1, 17, 255]))]
    return [Tensor(*value) for value in reversed(values)], {
        "out_"+n: (d, shape, data) for n, d, shape, data in values}


def load(vision, ctx, path, profile=None):
    desc = size(a.am_model_desc_t)
    desc.path_utf8 = str(path.resolve()).encode()
    desc.load_mode = a.AM_LOAD_RAW if profile is None else a.AM_LOAD_PROFILE
    desc.profile_id = None if profile is None else profile.encode()
    model = c.POINTER(a.amv_model_t)()
    check(vision.amv_model_load(ctx, c.byref(desc), c.byref(model)))
    return model


def output(result):
    value = result.contents
    assert not value.transform
    actual = {}
    for i in range(value.raw_output_count):
        t = value.raw_outputs[i]
        assert t.is_contiguous == 1
        actual[t.name.decode()] = (t.dtype, list(t.shape[:t.shape_len]), c.string_at(t.data, t.data_size))
    return actual


def invoke(vision, ctx, model, tensors, asynchronous=False, params=None):
    array = (a.am_tensor_in_t*len(tensors))(*(t.view for t in tensors))
    rid, result = c.c_uint64(), c.POINTER(a.amv_result_t)()
    params_ptr = None if params is None else c.byref(params)
    if asynchronous:
        code = vision.amv_infer_tensors_async(model, len(array), array, params_ptr, c.byref(rid))
    else:
        code = vision.amv_infer_tensors_sync(model, len(array), array, params_ptr, c.byref(result), c.byref(rid))
    check(code, a.AM_ERR_SDK_DEADLINE_EXCEEDED if (not asynchronous and params and params.sync_deadline_ms == 1) else 0)
    assert rid.value
    for t in tensors:
        t.clear()
    c.memset(array, 0, c.sizeof(array))
    if not result:
        base.wait(vision, ctx, rid.value)
        check(vision.amv_result_fetch(ctx, rid.value, c.byref(result)))
    return result, rid.value


def release(vision, ctx, result, rid):
    check(vision.amv_result_ack(ctx, rid))
    # ack移除缓存不能使外部指针失效。
    output(result)
    check(vision.amv_result_release(result))


def multi_io(vision, ctx, directory):
    model = load(vision, ctx, directory/"numeric.onnx")
    info = size(a.am_model_info_t)
    check(vision.amv_model_get_info(model, c.byref(info)))
    assert (info.input_count, info.output_count, info.native_batch) == (5, 5, 0)
    for rows in [2, 4, 65]:
        for asynchronous in [False, True]:
            tensors, expected = numeric(rows)
            tensors[2].view.shape = None
            result, rid = invoke(vision, ctx, model, tensors, asynchronous)
            assert result.contents.task == a.AM_TASK_GENERIC
            assert output(result) == expected
            release(vision, ctx, result, rid)
    # 同一输入组中多个明确关联必须一致，不能随输入数组顺序覆盖。
    tensors, expected = numeric()
    assoc = size(a.am_frame_assoc_t)
    assoc.source_id, assoc.frame_id, assoc.valid = 987, 654, a.AM_ASSOC_SOURCE | a.AM_ASSOC_FRAME
    assoc.pts, assoc.time_base_num, assoc.time_base_den = 9000, 1, 90000
    for t in tensors:
        t.view.assoc = c.pointer(assoc)
    result, rid = invoke(vision, ctx, model, tensors, True)
    c.memset(c.byref(assoc), 0, c.sizeof(assoc))
    assert result.contents.frame.contents.frame_id == 654
    assert result.contents.frame.contents.source_id == 987
    assert output(result) == expected
    release(vision, ctx, result, rid)
    base.close_model(vision, model)
    return {"dtypes": 5, "inputs": 5, "outputs": 5, "scalar": True,
            "namedOrderReversed": True, "dynamicRows": [2, 4, 65], "syncAsync": True, "associationCopied": True}


def rejects(vision, ctx, directory):
    model = load(vision, ctx, directory/"numeric.onnx")
    cases = []

    def case(label, change, expected):
        tensors, _ = numeric()
        change(tensors)
        array = (a.am_tensor_in_t*len(tensors))(*(t.view for t in tensors))
        for asynchronous in [False, True]:
            rid, result = c.c_uint64(123), c.POINTER(a.amv_result_t)()
            if asynchronous:
                code = vision.amv_infer_tensors_async(model, len(array), array, None, c.byref(rid))
            else:
                code = vision.amv_infer_tensors_sync(model, len(array), array, None, c.byref(result), c.byref(rid))
            check(code, expected)
            assert rid.value == 0 and not result
        cases.append(label)

    case("unknownName", lambda t: setattr(t[0].view, "name", b"missing"), a.AM_ERR_VISION_INPUT_NAME_UNKNOWN)
    case("duplicateName", lambda t: setattr(t[0].view, "name", t[1].name), a.AM_ERR_VISION_INPUT_NAME_UNKNOWN)
    case("unsupportedDtype", lambda t: setattr(t[0].view, "dtype", 99), a.AM_ERR_VISION_TENSOR_TYPE_UNSUPPORTED)
    case("dtypeMismatch", lambda t: setattr(t[2].view, "dtype", a.AM_DTYPE_F16), a.AM_ERR_VISION_INPUT_BYTES_MISMATCH)
    case("bytesMismatch", lambda t: setattr(t[0].view, "data_size", 1), a.AM_ERR_VISION_INPUT_BYTES_MISMATCH)
    case("rankTooLarge", lambda t: setattr(t[0].view, "shape_len", 9), a.AM_ERR_VISION_INPUT_SHAPE_MISMATCH)
    case("negativeDimension", lambda t: t[0].shape.__setitem__(0, -1), a.AM_ERR_VISION_INPUT_SHAPE_MISMATCH)
    case("nonContiguous", lambda t: setattr(t[0].view, "flags", 0), a.AM_ERR_VISION_IO_UNSUPPORTED)
    case("unknownFlags", lambda t: setattr(t[0].view, "flags", 3), a.AM_ERR_VISION_IO_UNSUPPORTED)
    case("nullData", lambda t: setattr(t[0].view, "data", None), a.AM_ERR_SDK_NULL_ARGUMENT)
    case("nullShape", lambda t: setattr(t[0].view, "shape", None), a.AM_ERR_SDK_NULL_ARGUMENT)
    # 元数据合法但超SDK动态包络：不访问故意不足的原始数据。
    def dynamic(t):
        t[0].shape[0], t[0].view.data_size = 8193, 8193
    case("dynamicRange", dynamic, a.AM_ERR_VISION_DYNAMIC_RANGE_EXCEEDED)
    def fixed(t):
        t[1].shape[0], t[1].view.data_size = 1, 8
    case("fixedShape", fixed, a.AM_ERR_VISION_INPUT_SHAPE_MISMATCH)
    def dtype(t):
        t[1].view.dtype = a.AM_DTYPE_F32
    case("sameWidthWrongDtype", dtype, a.AM_ERR_VISION_DTYPE_MISMATCH)
    assocs = [size(a.am_frame_assoc_t), size(a.am_frame_assoc_t)]
    assocs[0].frame_id, assocs[1].frame_id = 1, 2
    def conflicting(t):
        t[0].view.assoc, t[1].view.assoc = c.pointer(assocs[0]), c.pointer(assocs[1])
    case("conflictingAssociation", conflicting, a.AM_ERR_SDK_INVALID_ARGUMENT)
    # 关掉准入后输入即使仅有声明长度，也不得先复制再发现预算已满。
    limits = size(a.am_limits_t)
    check(vision.amv_get_limits(ctx, c.byref(limits)))
    limits.preprocess_budget_bytes = 0
    check(vision.amv_set_limits(ctx, c.byref(limits)))
    case("zeroBudget", lambda t: None, a.AM_ERR_SDK_QUEUE_FULL)
    base.close_model(vision, model)
    return cases


def timeout(vision, ctx, directory):
    model = load(vision, ctx, directory/"matmul.onnx")
    tensor = Tensor("matrix", a.AM_DTYPE_F32, [1536, 1536], struct.pack("<f", 1.0)*(1536*1536))
    params = size(a.am_infer_params_t)
    params.confidence_threshold, params.sync_deadline_ms, params.slot_key = 0.25, 1, 17
    result, rid = invoke(vision, ctx, model, [tensor], False, params)
    actual = output(result)["product"]
    assert actual == (a.AM_DTYPE_F32, [1536, 1536], struct.pack("<f", 1536.0)*(1536*1536))
    release(vision, ctx, result, rid)
    base.close_model(vision, model)
    return {"deadlineMs": 1, "copiedBeforeReturn": True, "resultAfterCallerZeroedInput": True}


def profiles(vision, ctx, directory):
    registry = json.loads((base.ROOT/"crates/vision/resources/profiles.json").read_text(encoding="utf-8"))["profiles"]
    filenames = ["yolo26n-pose.onnx", "yolo11n.onnx", "yolo11n-cls.onnx", "yolo11n-seg.onnx", "yolo11n-obb.onnx"]
    records = []
    for index, (profile, filename) in enumerate(zip(registry, filenames)):
        path = directory/filename
        assert hashlib.sha256(path.read_bytes()).hexdigest() == profile["sampleSha256"]
        results = []
        for mode in [profile["id"], None]:
            model = load(vision, ctx, path, mode)
            shape = profile["input"]["shape"]
            count = 1
            for v in shape:
                count *= v
            tensors = [Tensor("images", a.AM_DTYPE_F32, shape, bytes(count*4))]
            result, rid = invoke(vision, ctx, model, tensors, True)
            assert result.contents.task == (index+1 if mode else a.AM_TASK_GENERIC)
            if mode:
                for task in ["pose", "detect", "classify", "segment", "obb"]:
                    value = getattr(result.contents, task)
                    if task != ["pose", "detect", "classify", "segment", "obb"][index]:
                        assert value.count == 0
                        pointer = "detections" if task == "pose" else ("boxes" if task == "detect" else ("entries" if task == "classify" else "instances"))
                        assert not getattr(value, pointer)
            results.append(output(result))
            release(vision, ctx, result, rid)
            base.close_model(vision, model)
        assert results[0] == results[1]
        records.append(profile["id"])
    return records


def nv12(vision, ctx, directory):
    model = load(vision, ctx, directory/"image.onnx")
    y, uv = (c.c_ubyte*4)(100, 100, 100, 100), (c.c_ubyte*2)(150, 180)
    frame = size(a.am_frame_view_t)
    frame.width, frame.height, frame.format, frame.plane_count = 2, 2, a.AM_PF_NV12, 2
    frame.planes[0], frame.planes[1] = y, uv
    frame.strides[0], frame.strides[1] = 2, 2
    frame.plane_size_bytes[0], frame.plane_size_bytes[1] = 4, 2
    view = size(a.am_pixel_view_t)
    view.magic, view.view = a.AM_PIXEL_VIEW_MAGIC, c.pointer(frame)
    image = size(a.am_image_input_t)
    image.kind, image.view = a.AM_IMG_PIXEL_VIEW, c.pointer(view)
    preprocess = size(a.am_raw_preprocess_t)
    preprocess.target_w, preprocess.target_h, preprocess.resize = 2, 2, 1
    params = size(a.am_infer_params_t)
    params.confidence_threshold, params.raw_preprocess = 0.25, c.pointer(preprocess)
    # 独立按Kr/Kb公式核对，避免复用实现里的系数表或把枚举值整体平移。
    for matrix, (kr, kb) in enumerate([(0.299, 0.114), (0.2126, 0.0722), (0.2627, 0.0593)]):
        for full in [0, 1]:
            frame.color_matrix, frame.color_range = matrix, full
            luma = 100 if full else (100-16)*255/219
            u, v = 22*(1 if full else 255/224), 52*(1 if full else 255/224)
            expected = [luma+2*(1-kr)*v,
                        luma-2*kb*(1-kb)/(1-kr-kb)*u-2*kr*(1-kr)/(1-kr-kb)*v,
                        luma+2*(1-kb)*u]
            result, rid = c.POINTER(a.amv_result_t)(), c.c_uint64()
            check(vision.amv_infer_image_sync(model, c.byref(image), c.byref(params), c.byref(result), c.byref(rid)))
            assert result.contents.transform.contents.valid == 1
            data = base.outputs(result)[0][2]
            values = struct.unpack("<12f", data)
            for channel in range(3):
                assert all(abs(value-expected[channel]) < 0.0002 for value in values[channel*4:channel*4+4]), (matrix, full, values, expected)
            check(vision.amv_result_ack(ctx, rid.value))
            check(vision.amv_result_release(result))
    base.close_model(vision, model)
    return {"matrices": ["BT601=0", "BT709=1", "BT2020NCL=2"], "ranges": ["limited", "full"], "pixelReference": "Kr/Kb independent formula"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--sample-dir", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    args = parser.parse_args()
    for item in json.loads((args.sample_dir/"manifest.json").read_text())["files"]:
        assert hashlib.sha256((args.sample_dir/item["file"]).read_bytes()).hexdigest() == item["sha256"]
    vision = c.CDLL(str((args.dll_dir/"am_vision.dll").resolve()))
    missing = a.bind(vision, "amv_")
    ctx = base.initialize(vision, args.runtime_dir)
    cap = size(a.amv_capability_t)
    check(vision.amv_query_capability(ctx, c.byref(cap)))
    assert cap.feature_bits & a.AMV_CAP_TENSOR_INPUT
    results = {"numeric": multi_io(vision, ctx, args.sample_dir), "timeout": timeout(vision, ctx, args.sample_dir),
               "profiles": profiles(vision, ctx, args.model_dir), "nv12": nv12(vision, ctx, args.sample_dir)}
    base.shared.close(vision, ctx, False)
    ctx = base.initialize(vision, args.runtime_dir)
    results["rejects"] = rejects(vision, ctx, args.sample_dir)
    base.shared.close(vision, ctx, False)
    results["actualVisionExports"] = sum(n.startswith("amv_") for n in a.FUNCTIONS)-len(missing)
    results["scope"] = "actual CPU tensor inference; other SDK capabilities and final G0-G3 verified separately"
    print(json.dumps(results, ensure_ascii=False))


if __name__ == "__main__":
    main()
