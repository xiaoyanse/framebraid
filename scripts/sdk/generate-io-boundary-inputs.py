"""生成模型元数据数量边界夹具；只写隔离测试目录。"""
import argparse
import hashlib
import json
from pathlib import Path
import onnx
from onnx import helper, TensorProto
from local_paths import local_path

parser = argparse.ArgumentParser()
parser.add_argument("--output", required=True, type=Path)
args = parser.parse_args()
out = local_path(args.output, "tests")
out.mkdir(parents=True, exist_ok=True)
manifest = {}
for name, inputs, outputs in (("inputs33", 33, 1), ("outputs33", 1, 33), ("boundary32", 32, 32)):
    ins = [helper.make_tensor_value_info("input%d" % i, TensorProto.BOOL if inputs == 33 and i == 32 else TensorProto.FLOAT, [1]) for i in range(inputs)]
    outs = [helper.make_tensor_value_info("output%d" % i, TensorProto.BOOL if outputs == 33 and i == 32 else TensorProto.FLOAT, [1]) for i in range(outputs)]
    initializers = [helper.make_tensor("boolean", TensorProto.BOOL, [1], [True])] if outputs == 33 else []
    nodes = [helper.make_node("Identity", ["boolean" if outputs == 33 and i == 32 else "input0"], ["output%d" % i]) for i in range(outputs)]
    model = helper.make_model(helper.make_graph(nodes, name, ins, outs, initializers), opset_imports=[helper.make_opsetid("", 17)], ir_version=8)
    onnx.checker.check_model(model)
    path = out / (name + ".onnx")
    onnx.save(model, path)
    manifest[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
(out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
print(json.dumps(manifest))
