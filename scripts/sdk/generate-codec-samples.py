"""用本机FFmpeg生成离线合成样本，新目录保存来源和指纹，不覆盖已有资料。"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("--output", required=True, type=Path)
args = parser.parse_args()
root = args.output.resolve()
if not args.output.is_absolute() or root.exists():
    raise SystemExit("必须使用新的绝对样本目录，不覆盖已有文件")
root.mkdir(parents=True)
version = subprocess.check_output(["ffmpeg", "-version"], text=True).splitlines()[0]
jobs = [
    ("one.h264", ["-f", "lavfi", "-i", "color=c=red:s=256x144:r=1", "-frames:v", "1", "-c:v", "libx264",
                  "-threads", "1", "-preset", "ultrafast", "-tune", "zerolatency", "-f", "h264"]),
    ("one.hevc", ["-f", "lavfi", "-i", "color=c=red:s=256x144:r=1", "-frames:v", "1", "-c:v", "libx265",
                  "-x265-params", "pools=none:frame-threads=1:log-level=error", "-f", "hevc"]),
    ("multi-h264.mp4", ["-f", "lavfi", "-i", "testsrc2=size=64x64:rate=6", "-frames:v", "12", "-c:v", "libx264",
                        "-threads", "1", "-x264-params", "bframes=3:keyint=12:repeat-headers=1", "-f", "mp4"]),
    ("multi-h265.mp4", ["-f", "lavfi", "-i", "testsrc2=size=64x64:rate=6", "-frames:v", "12", "-c:v", "libx265",
                        "-x265-params", "pools=none:frame-threads=1:bframes=3:keyint=12:repeat-headers=1:log-level=error",
                        "-tag:v", "hev1", "-f", "mp4"]),
]
records = []
for name, parameters in jobs:
    command = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-n", *parameters, str(root / name)]
    subprocess.run(command, check=True, stdin=subprocess.DEVNULL)
    content = (root / name).read_bytes()
    records.append({"name": name, "bytes": len(content), "sha256": hashlib.sha256(content).hexdigest(),
                    "arguments": command[1:]})
(root / "samples.json").write_text(json.dumps({"generator": version, "files": records}, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"output": str(root), "syntheticFiles": len(records)}))
