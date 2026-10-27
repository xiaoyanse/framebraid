"""H2.4 定向补齐：多尺寸/profile 硬解矩阵与非随机访问点起流重同步；仅本地合成样本。"""
import argparse
import ctypes as c
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
import amrt_sdk as a

# x264 无 profile 参数选项（-profile:v 会被 x264-params 解析覆盖、未知 params 项被静默吞掉），
# 因此 H.264 用纯 ffmpeg 选项、HEVC 经 x265-params 注入；生成后用 ffprobe 断言真实 profile。
MATRIX = [
    # (文件名, 编码, 宽, 高, ffmpeg附加参数, x265-params附加, 期望profile, 期望硬件可解)
    ("h264-360p-main.h264", "h264", 640, 360, ["-profile:v", "main"], None, "Main", True),
    ("h264-720p-main.h264", "h264", 1280, 720, ["-profile:v", "main"], None, "Main", True),
    ("h264-720p-baseline.h264", "h264", 1280, 720, ["-profile:v", "baseline"], None,
     "Constrained Baseline", True),
    ("h264-1080p-high.h264", "h264", 1920, 1080, ["-profile:v", "high"], None, "High", True),
    ("h264-1440p-high.h264", "h264", 2560, 1440, ["-profile:v", "high"], None, "High", True),
    ("h265-720p-main.h265", "h265", 1280, 720, [], ["profile=main"], "Main", True),
    ("h265-1440p-main.h265", "h265", 2560, 1440, [], ["profile=main"], "Main", True),
    ("h265-2160p-main10.h265", "h265", 3840, 2160, ["-pix_fmt", "yuv420p10le"],
     ["profile=main10"], "Main 10", True),
]
RESYNC = "h264-resync-30f.h264"


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def check(code):
    assert code == 0, code


def generate(root: Path):
    version = subprocess.check_output(["ffmpeg", "-version"], text=True).splitlines()[0]
    records = []
    for name, codec, width, height, ffmpeg_extra, params_extra, want_profile, _ in MATRIX:
        encoder = ["-c:v", "libx264", "-threads", "1", "-preset", "veryfast"]
        if codec == "h265":
            # keyint=1会迫使x265升Rext，单帧样本无需GOP选项。
            params = ":".join(["pools=none:frame-threads=1:log-level=error", *params_extra])
            encoder = ["-c:v", "libx265", "-x265-params", params]
        command = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-n", "-f", "lavfi",
                   "-i", f"testsrc2=size={width}x{height}:rate=1", "-frames:v", "1",
                   *encoder, *ffmpeg_extra,
                   "-f", ("h264" if codec == "h264" else "hevc"), str(root / name)]
        subprocess.run(command, check=True, stdin=subprocess.DEVNULL)
        profile = subprocess.check_output(
            ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=profile",
             "-of", "csv=p=0", str(root / name)]).decode().strip()
        assert profile == want_profile, f"{name} profile {profile} != {want_profile}（参数被覆盖）"
        records.append(_fingerprint(root, name, command))
    resync_command = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-n", "-f", "lavfi",
                      "-i", "testsrc2:size=1280x720:rate=6", "-frames:v", "30", "-c:v", "libx264",
                      "-threads", "1", "-preset", "ultrafast", "-x264-params",
                      "bframes=3:keyint=12:min-keyint=12:repeat-headers=1", "-f", "h264",
                      str(root / RESYNC)]
    subprocess.run(resync_command, check=True, stdin=subprocess.DEVNULL)
    records.append(_fingerprint(root, RESYNC, resync_command))
    (root / "samples.json").write_text(
        json.dumps({"generator": version, "files": records}, indent=2) + "\n", encoding="utf-8")


def _fingerprint(root, name, command):
    content = (root / name).read_bytes()
    return {"name": name, "bytes": len(content), "sha256": hashlib.sha256(content).hexdigest(),
            "arguments": command[1:]}


def packets_from_second_idr(stream: bytes) -> tuple[int, list[bytes]]:
    packets = json.loads(subprocess.check_output(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_packets", "-of", "json", "-"],
        input=stream).decode())["packets"]
    keyframes = [packet for packet in packets if "K" in packet.get("flags", "")]
    assert len(keyframes) >= 3, f"30帧keyint=12应至少有3个关键帧，实际{len(keyframes)}"
    second = keyframes[1]
    offset = int(second["pos"])
    tail = [stream[int(packet["pos"]):int(packet["pos"]) + int(packet["size"])]
            for packet in packets if int(packet["pos"]) >= offset]
    return offset, tail


def decode(camera, ctx, codec, packets_data, width, height, prefer_hw, expect_frames=None):
    config = sized(a.am_decoder_config_t)
    config.codec = codec
    config.hw_policy = a.AM_DHW_PREFER_HW if prefer_hw else a.AM_DHW_SOFTWARE_ONLY
    config.output_pixel_format = a.AM_PF_RGBA8
    config.time_base_num, config.time_base_den = 1, 1000
    decoder = c.POINTER(a.amc_decoder_t)()
    check(camera.amc_decoder_create(ctx, c.byref(config), c.byref(decoder)))
    for index, data in enumerate(packets_data):
        owned = (c.c_ubyte * len(data)).from_buffer_copy(data)
        packet = sized(a.am_decoder_input_t)
        packet.data, packet.size = owned, len(data)
        packet.flags, packet.pts, packet.dts = a.AM_PKT_AU_COMPLETE, index, index
        check(camera.amc_decoder_submit(decoder, c.byref(packet)))
    check(camera.amc_decoder_drain(decoder))
    frames = []
    while True:
        frame = c.POINTER(a.amc_frame_t)()
        code = camera.amc_decoder_receive(decoder, 0, c.byref(frame))
        if code == a.AM_ERR_SDK_DRAINED:
            break
        check(code)
        meta, view = sized(a.am_frame_meta_t), sized(a.am_frame_view_t)
        check(camera.amc_frame_get_meta(frame, c.byref(meta)))
        check(camera.amc_frame_get_view(frame, c.byref(view)))
        assert view.width == width and view.height == height, (view.width, view.height)
        frames.append({"pts": meta.pts, "generation": meta.source_generation})
        check(camera.amc_frame_release(frame))
    if expect_frames is not None:
        assert len(frames) == expect_frames, f"帧数{len(frames)}≠{expect_frames}"
    info = sized(a.am_decoder_info_t)
    check(camera.amc_decoder_get_info(decoder, c.byref(info)))
    backend = info.backend_name_utf8.decode() if info.backend_name_utf8 else None
    hardware = bool(info.hw_accelerated)
    lifecycle = c.c_uint32()
    check(camera.amc_decoder_close(decoder, 100, c.byref(lifecycle)))
    check(camera.amc_object_destroy(decoder))
    return {"backend": backend, "hardware": hardware, "frames": len(frames),
            "generations": sorted({row["generation"] for row in frames})}


def main(args):
    dll_path = Path(args.dll_dir).resolve()
    search = os.add_dll_directory(str(dll_path))
    camera = c.CDLL(str(dll_path / "am_camera.dll"))
    a.bind(camera, "amc_")
    directory = (c.c_char_p * 1)(str(Path(args.runtime_dir).resolve()).encode())
    params = sized(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    params.runtime_dirs_utf8, params.runtime_dir_count = directory, 1
    ctx = c.POINTER(a.amc_context_t)()
    check(camera.amc_init(c.byref(params), c.byref(ctx)))
    rows = []
    for name, codec, width, height, _, _, want_profile, expect_hw in MATRIX:
        data = Path(args.samples_dir, name).read_bytes()
        case = {"file": name, "width": width, "height": height, "profile": want_profile}
        for policy in (True, False):
            outcome = decode(camera, ctx, getattr(a, f"AM_CODEC_{codec.upper()}"), [data],
                             width, height, policy, expect_frames=1)
            if policy and expect_hw:
                assert outcome["hardware"], (name, outcome)
            if not policy:
                assert not outcome["hardware"], (name, outcome)
            case["prefer_hw" if policy else "software"] = outcome
        rows.append(case)
    # 重同步：30帧GOP(0,12,24)，从第二个IDR（帧12）起按AU逐包送，应解出18帧；
    # 前缀包（帧0-11）被丢弃，验证解码器从随机访问点恢复而非从乱码前缀起流。
    stream = Path(args.samples_dir, RESYNC).read_bytes()
    offset, tail_packets = packets_from_second_idr(stream)
    resync = {"file": RESYNC, "dropped_prefix_bytes": offset, "submitted_aus": len(tail_packets)}
    for policy in (True, False):
        outcome = decode(camera, ctx, a.AM_CODEC_H264, tail_packets, 1280, 720, policy,
                         expect_frames=18)
        resync["prefer_hw" if policy else "software"] = outcome
    rows.append(resync)
    lifecycle = c.c_uint32()
    check(camera.amc_shutdown(ctx, 100, c.byref(lifecycle)))
    assert lifecycle.value == a.AM_LIFECYCLE_CLOSED
    check(camera.amc_object_destroy(ctx))
    search.close()
    print(json.dumps({"cases": rows, "idr_offset": offset, "total_bytes": len(stream),
                      "scope": "synthetic single/mid-stream AU; actual backend identity checked; not throughput"},
                     ensure_ascii=False))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", required=True)
    parser.add_argument("--runtime-dir", required=True)
    parser.add_argument("--samples-dir", required=True, type=Path)
    parser.add_argument("--generate", action="store_true", help="先在新样本目录生成合成样本")
    args = parser.parse_args()
    if args.generate:
        if not args.samples_dir.is_absolute() or args.samples_dir.exists():
            raise SystemExit("--generate 必须配合新的绝对样本目录")
        args.samples_dir.mkdir(parents=True)
        generate(args.samples_dir)
    main(args)
