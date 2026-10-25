"""生成隔离的数值ONNX样本；生成期需要onnx/numpy，SDK调用测试不依赖它们。"""
import argparse
import hashlib
import json
from pathlib import Path
import sys
sys.dont_write_bytecode = True
import numpy as np
import onnx
from onnx import TensorProto as T, helper as h, numpy_helper


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    types = [("float", T.FLOAT, ["rows", 3]), ("half", T.FLOAT16, [2]),
             ("long", T.INT64, []), ("int", T.INT32, [2, 2]), ("byte", T.UINT8, ["count"])]
    graph = h.make_graph(
        [h.make_node("Identity", [name], ["out_"+name]) for name, _, _ in types],
        "five_numeric_types",
        [h.make_tensor_value_info(name, dtype, shape) for name, dtype, shape in types],
        [h.make_tensor_value_info("out_"+name, dtype, shape) for name, dtype, shape in types])
    records = []
    for name, g in [("numeric.onnx", graph), ("matmul.onnx", h.make_graph(
        [h.make_node("MatMul", ["matrix", "weights"], ["product"])], "deadline_copy",
        [h.make_tensor_value_info("matrix", T.FLOAT, [1536, 1536])],
        [h.make_tensor_value_info("product", T.FLOAT, [1536, 1536])],
        [numpy_helper.from_array(np.ones((1536, 1536), dtype=np.float32), "weights")])) ,
        ("image.onnx", h.make_graph([h.make_node("Identity", ["images"], ["pixels"])], "pixel_reference",
            [h.make_tensor_value_info("images", T.FLOAT, [1, 3, 2, 2])],
            [h.make_tensor_value_info("pixels", T.FLOAT, [1, 3, 2, 2])]))]:
        model = h.make_model(g, producer_name="AM-RT SDK test", opset_imports=[h.make_opsetid("", 17)])
        model.ir_version = 10
        onnx.checker.check_model(model)
        target = args.output/name
        onnx.save_model(model, target)
        records.append({"file": name, "sha256": hashlib.sha256(target.read_bytes()).hexdigest()})
    (args.output/"manifest.json").write_text(json.dumps({"onnx": onnx.__version__, "files": records}, indent=2), encoding="utf-8")
    print(json.dumps(records))


if __name__ == "__main__":
    main()
