"""显式选择 --rtsp 才订阅 RTSP，否则单次 ISAPI；不输出地址和凭据。"""
import argparse
import os
from pathlib import Path
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "bindings/python"))
from safe_sdk import CameraContext


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", required=True)
    parser.add_argument("--runtime-dir", required=True)
    parser.add_argument("--rtsp", action="store_true")
    args = parser.parse_args()
    # 地址和凭据通过调用方环境注入，不进入命令行、示例输出或 SDK 持久化。
    with CameraContext(Path(args.dll_dir)/"am_camera.dll", [args.runtime_dir]) as camera, \
         camera.open_source(os.environ["AM_SDK_SOURCE_URL"], os.environ.get("AM_SDK_USERNAME"),
                            os.environ.get("AM_SDK_PASSWORD"), args.rtsp) as source:
        if args.rtsp:
            with source.frames() as subscription, subscription.pull() as frame:
                print("Frame plane bytes:", [len(plane) for plane in frame.copy_planes()])
        else:
            with source.capture() as image:
                print("Captured JPEG bytes:", len(image.jpeg()))


if __name__ == "__main__":
    main()
