"""Run a frozen consumer binding against an explicitly selected candidate DLL pair."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", default="v0.6.5")
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--model", type=Path, required=True,
                        help="Synthetic image-dynamic.onnx from generate-batch-samples.py")
    args = parser.parse_args()
    dll_dir, runtime_dir, model_path = (p.resolve(strict=True) for p in
                                        (args.dll_dir, args.runtime_dir, args.model))
    root = Path(__file__).resolve().parents[2]
    commit = subprocess.check_output(
        ["git", "rev-parse", "--verify", "--end-of-options", args.baseline + "^{commit}"],
        cwd=root, text=True).strip()
    tests = root / ".local/tests"
    tests.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="binding-compat-", dir=tests) as tmp:
        old = Path(tmp)
        for name in ("amrt_sdk.py", "safe_sdk.py", "load_diagnostics.py"):
            data = subprocess.check_output(
                ["git", "show", commit + ":sdk/bindings/python/" + name], cwd=root)
            (old / name).write_bytes(data)
        result = subprocess.run(
            [sys.executable, "-B", str(Path(__file__).resolve()), "--",
             str(old), str(dll_dir), str(runtime_dir), str(model_path)],
            timeout=60, stdout=subprocess.PIPE, text=True)
        result.check_returncode()
        print(json.dumps({"baselineCommit": commit, **json.loads(result.stdout)}))


def smoke(old, dll_dir, runtime_dir, model_path):
    import struct
    sys.path.insert(0, old)
    import amrt_sdk as a
    from safe_sdk import CameraContext, RgbBuffer, VisionContext, sized

    dll_dir = Path(dll_dir)
    with CameraContext(dll_dir / "am_camera.dll"):
        pass
    preprocess = sized(a.am_raw_preprocess_t)
    preprocess.target_w = preprocess.target_h = 2
    preprocess.resize = a.AM_RESIZE_STRETCH
    with VisionContext(dll_dir / "am_vision.dll", [runtime_dir]) as vision:
        with vision.load_model(model_path) as model, RgbBuffer(2, 2, bytes([1, 2, 3] * 4)) as rgb:
            rid, result = rgb.pixels().infer(model, preprocess=preprocess)
            with result:
                _, _, shape, data = result.raw_outputs()[0]
                assert shape == (1, 3, 2, 2), shape
                assert struct.unpack("<12f", data) == (1.0,) * 4 + (2.0,) * 4 + (3.0,) * 4
            vision.ack(rid)
            candidate_abi = vision.library.amv_get_abi_version()
    print(json.dumps({"consumerAbi": a.AM_ABI_VERSION, "candidateAbi": candidate_abi,
                      "media": "init/shutdown/destroy", "vision": "inference/ack/shutdown/destroy"}))


if __name__ == "__main__":
    if sys.argv[1:2] == ["--"]:
        smoke(*sys.argv[2:])
    else:
        main()
