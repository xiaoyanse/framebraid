"""生成可证明原生组批的五dtype模型；只写显式隔离目录。"""
import argparse
import hashlib
import json
from pathlib import Path
import sys
sys.dont_write_bytecode = True
import onnx
from onnx import TensorProto as T, helper as h


def make(batch, declaration, broken_output=False):
    signatures = [("float", T.FLOAT, [batch, 3]), ("half", T.FLOAT16, [batch, 2]),
                  ("long", T.INT64, [batch]), ("int", T.INT32, [batch, 2, 2]),
                  ("byte", T.UINT8, [batch, 3])]
    # 跨batch求均值再相减：若伪装成逐帧循环，所有输出都为0，能被独立期望明确识别。
    nodes = [h.make_node("ReduceMean", ["float"], ["mean"], axes=[0], keepdims=1),
             h.make_node("Sub", ["float", "mean"], ["out_float"])]
    nodes += [h.make_node("Identity", [name], ["out_"+name]) for name, _, _ in signatures[1:]]
    initializers = []
    if broken_output == "runtime":
        initializers = [h.make_tensor("index", T.INT64, [1], [0]), h.make_tensor("one", T.INT64, [1], [1])]
        nodes = [h.make_node("Shape", ["float"], ["shape"]),
                 h.make_node("Gather", ["shape", "index"], ["n"]),
                 h.make_node("Sub", ["n", "one"], ["end"]),
                 h.make_node("Slice", ["float", "index", "end", "index", "one"], ["out_float"])] + nodes[2:]
    elif broken_output:
        nodes[1] = h.make_node("Identity", ["mean"], ["out_float"])
    graph = h.make_graph(nodes, "native_batch_reference",
        [h.make_tensor_value_info(name, dtype, shape) for name, dtype, shape in signatures],
        [h.make_tensor_value_info("out_"+name, dtype, shape) for name, dtype, shape in signatures], initializers)
    model = h.make_model(graph, producer_name="AM-RT SDK batch test", opset_imports=[h.make_opsetid("", 17)])
    model.ir_version = 10
    if declaration is not None:
        h.set_model_props(model, {"am.sdk.batch_axis": declaration})
    onnx.checker.check_model(model)
    return model


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    models = [("dynamic.onnx", make("batch", "0")), ("fixed4.onnx", make(4, "0")),
              ("unproved.onnx", make("batch", None)), ("wrong-axis.onnx", make("batch", "1")),
              ("bad-output.onnx", make("batch", "0", True)),
              ("runtime-bad-output.onnx", make("batch", "0", "runtime"))]
    image_graph = h.make_graph([h.make_node("Identity", ["images"], ["pixels"])], "image_batch",
        [h.make_tensor_value_info("images", T.FLOAT, ["batch", 3, 2, 2])],
        [h.make_tensor_value_info("pixels", T.FLOAT, ["batch", 3, 2, 2])])
    image_model = h.make_model(image_graph, producer_name="AM-RT SDK batch test", opset_imports=[h.make_opsetid("", 17)])
    image_model.ir_version = 10
    h.set_model_props(image_model, {"am.sdk.batch_axis": "0"})
    onnx.checker.check_model(image_model)
    models.append(("image-dynamic.onnx", image_model))
    records = []
    for name, model in models:
        path = args.output/name
        onnx.save(model, path)
        records.append({"file": name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "bytes": path.stat().st_size})
    manifest = {"onnx": onnx.__version__, "opset": 17, "ir": 10, "files": records}
    (args.output/"manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(json.dumps(manifest))


if __name__ == "__main__":
    main()
