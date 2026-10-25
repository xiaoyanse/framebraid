"""记录干净提交的真实 Cargo Release 构建与输入指纹，输出放仓内忽略的 .local 分类。"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
from local_paths import local_path


ROOT = Path(__file__).resolve().parents[2]


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs():
    names = git("ls-files").splitlines()
    selected = [name for name in names if name.startswith(("crates/", "src/")) or
                name in ["Cargo.toml", "Cargo.lock"]]
    return [{"path": name, "bytes": (ROOT / name).stat().st_size,
             "sha256": digest(ROOT / name)} for name in selected]


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--record", type=Path, required=True)
    args = parser.parse_args()
    if git("status", "--porcelain"):
        raise ValueError("Build record requires a clean committed checkout")
    record = local_path(args.record, "evidence", fresh=True)
    if record.with_suffix(".log").exists():
        raise ValueError("Refuse existing build log")
    commit = git("rev-parse", "HEAD")
    before = inputs()
    command = ["cargo", "build", "--release", "--workspace", "--locked", "--offline", "--features",
               "am-camera-ffi/shared-frames,am-vision-ffi/shared-frames"]
    import os
    target = local_path(os.environ.get("CARGO_TARGET_DIR", ROOT / ".local/build/current"), "build")
    os.environ["CARGO_TARGET_DIR"] = str(target)
    start = time.monotonic()
    result = subprocess.run(command, cwd=ROOT, capture_output=True)
    record.parent.mkdir(parents=True, exist_ok=True)
    record.with_suffix(".log").write_bytes(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError("Cargo Release failed; see the recorded log")
    if before != inputs() or git("rev-parse", "HEAD") != commit or git("status", "--porcelain"):
        raise ValueError("Source changed during build")
    compiler = subprocess.check_output(["rustc", "-vV"], text=True)
    binaries = [{"file": name, "bytes": (target / "release" / name).stat().st_size,
                 "sha256": digest(target / "release" / name)}
                for name in ["am_camera.dll", "am_vision.dll"]]
    data = {"schemaVersion": 1, "configuration": "release", "exitCode": 0,
            "sourceCommit": commit, "command": command, "compiler": compiler,
            "elapsedSeconds": round(time.monotonic() - start, 3),
            "buildTimingScope": "Actual Cargo invocation; may reuse unchanged compilation artifacts",
            "buildInputs": before, "binaries": binaries}
    record.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"sourceCommit": commit, "binaries": binaries}))
