"""使用独立 Python ORT 检查实际 Pose 模型与 SDK 同输入推理。"""
import argparse
from pathlib import Path
import sys

import numpy as np
import onnxruntime as ort
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
from safe_sdk import RgbBuffer, VisionContext


def preprocess(rgb):
    height, width, _ = rgb.shape
    scale = np.minimum(np.float32(640) / np.float32(width), np.float32(640) / np.float32(height))
    # 正数按 Rust f32 round 的半数远离零规则处理。
    sw = max(1, int(np.floor(np.float32(width) * scale + np.float32(.5))))
    sh = max(1, int(np.floor(np.float32(height) * scale + np.float32(.5))))
    left, top = (640 - sw) // 2, (640 - sh) // 2
    xx = np.clip((np.arange(sw, dtype=np.float32) + np.float32(.5)) / scale - np.float32(.5), 0, width - 1)
    yy = np.clip((np.arange(sh, dtype=np.float32) + np.float32(.5)) / scale - np.float32(.5), 0, height - 1)
    ix, iy = xx.astype(np.int32), yy.astype(np.int32)
    fx, fy = (xx - ix).astype(np.float32)[None, :, None], (yy - iy).astype(np.float32)[:, None, None]
    source = rgb.astype(np.float32)
    a = source[iy[:, None], ix[None, :]]
    b = source[iy[:, None], np.minimum(ix + 1, width - 1)[None, :]]
    c = source[np.minimum(iy + 1, height - 1)[:, None], ix[None, :]]
    d = source[np.minimum(iy + 1, height - 1)[:, None], np.minimum(ix + 1, width - 1)[None, :]]
    image = np.full((640, 640, 3), np.float32(114), dtype=np.float32)
    image[top:top + sh, left:left + sw] = (a * (1 - fx) + b * fx) * (1 - fy) + (c * (1 - fx) + d * fx) * fy
    return np.ascontiguousarray((image / np.float32(255)).transpose(2, 0, 1)[None])


def main():
    parser = argparse.ArgumentParser()
    for name in ("dll", "runtime", "model", "image"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    with Image.open(args.image) as source:
        rgb = np.asarray(source.convert("RGB"))
    height, width, _ = rgb.shape
    with VisionContext(args.dll, [args.runtime]) as vision, \
         vision.load_model(args.model, "pose-yolo26-e2e-coco17-v1") as model, \
         RgbBuffer(width, height, rgb.tobytes()) as image:
        request, result = image.pixels().infer(model)
        with result:
            tensors = result.raw_outputs()
            assert len(tensors) == 1 and tensors[0][2] == (1, 300, 57)
            actual = np.frombuffer(tensors[0][3], dtype="<f4").reshape(tensors[0][2]).copy()
            count = len(result.copy_pose())
            vision.ack(request)
    session = ort.InferenceSession(str(args.model), providers=["CPUExecutionProvider"])
    expected = session.run(None, {session.get_inputs()[0].name: preprocess(rgb)})[0]
    difference = np.abs(actual - expected)
    assert actual.shape == expected.shape and count > 0
    actual_valid = actual[0][actual[0, :, 4] >= .25]
    expected_valid = expected[0][expected[0, :, 4] >= .25]
    assert actual_valid.shape == expected_valid.shape and len(actual_valid) == count
    valid_difference = np.abs(actual_valid - expected_valid)
    print({"ort_version": ort.__version__, "shape": list(actual.shape), "detections": count,
           "all_rows_max_abs": float(difference.max()),
           "valid_max_abs": float(valid_difference.max()),
           "valid_p99_abs": float(np.quantile(valid_difference, .99))})
    # 低分候选排序可能不同，不视作有效检测误差，但保留全行差异。
    assert np.allclose(actual_valid, expected_valid, rtol=1e-3, atol=1e-3)


if __name__ == "__main__":
    main()
