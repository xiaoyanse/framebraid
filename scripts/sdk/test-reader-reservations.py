"""为Rust Reader预算测试提供随机回环RTSP；只管理本轮子进程。"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
from camera_recovery_fixture import Stream
from local_paths import local_path


def main(args):
    work = local_path(args.work_dir.resolve(), "tests", fresh=True)
    work.mkdir(parents=True)
    target = {"metric": "MET-2 M-RECOVER S5", "packets": 25,
              "tiny_budget_reject_before_copy": True, "after_reader_and_packet_drop_bytes": 0,
              "scope": "real loopback H264; SDK payload/configuration reservation, not native RSS/hardware/soak"}
    (work / "target-card.json").write_text(json.dumps(target, indent=2), encoding="utf-8")
    with Stream(work, args.mediamtx.resolve()) as stream:
        env = dict(os.environ)
        env.update({"AM_RT_FFMPEG_DIR": str(args.runtime_dir.resolve()),
                    "AM_SDK_LOCAL_RTSP_URL": stream.url,
                    "AM_SDK_LOCAL_RTSP_USER": stream.user.decode(),
                    "AM_SDK_LOCAL_RTSP_PASSWORD": stream.password.decode()})
        command = ["cargo", "test", "-p", "am-camera-ffi", "--all-features", "--lib",
                   "native_loopback_reader_reserves_before_copy_and_transfers_packet_ownership", "--",
                   "--ignored", "--test-threads=1", "--nocapture"]
        with (work / "rust-result.txt").open("w", encoding="utf-8") as log:
            result = subprocess.run(command, cwd=Path(__file__).resolve().parents[2], env=env,
                                    stdout=log, stderr=subprocess.STDOUT, timeout=90)
        record = {"code": result.returncode, "command": command, "scope": target["scope"]}
        (work / "result.json").write_text(json.dumps(record, indent=2), encoding="utf-8")
        print(json.dumps(record))
        assert result.returncode == 0


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    for name in ["runtime-dir", "mediamtx", "work-dir"]:
        p.add_argument("--" + name, type=Path, required=True)
    main(p.parse_args())
