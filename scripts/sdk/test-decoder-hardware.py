"""真实Release DLL自动选择与不兼容码流回退；仅使用显式本地合成样本。"""
import argparse
import ctypes as c
import hashlib
import json
import os
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
import amrt_sdk as a


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def check(code):
    assert code == 0, code


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
    for codec, filename, expect_hw in [
        (a.AM_CODEC_H264, "frame.h264", True),
        (a.AM_CODEC_H265, "frame-main.h265", True),
        (a.AM_CODEC_H265, "frame-main10.h265", True),
        (a.AM_CODEC_H265, "frame.h265", False),
    ]:
        config = sized(a.am_decoder_config_t)
        config.codec = codec
        config.hw_policy = a.AM_DHW_SOFTWARE_ONLY if args.software else a.AM_DHW_PREFER_HW
        config.output_pixel_format = a.AM_PF_RGBA8
        config.time_base_num, config.time_base_den = 1, 1000
        dec = c.POINTER(a.amc_decoder_t)()
        start = time.perf_counter()
        check(camera.amc_decoder_create(ctx, c.byref(config), c.byref(dec)))
        create_ms = (time.perf_counter() - start) * 1000
        info = sized(a.am_decoder_info_t)
        check(camera.amc_decoder_get_info(dec, c.byref(info)))
        assert info.hw_accelerated == 0
        assert info.backend_name_utf8 == (b"ffmpeg-cpu" if args.software else None)
        data = Path(args.samples_dir, filename).read_bytes()
        owned = (c.c_ubyte * len(data)).from_buffer_copy(data)
        packet = sized(a.am_decoder_input_t)
        packet.data, packet.size = owned, len(data)
        packet.flags, packet.pts, packet.dts = a.AM_PKT_AU_COMPLETE, 123, 123
        check(camera.amc_decoder_submit(dec, c.byref(packet)))
        check(camera.amc_decoder_drain(dec))
        frame = c.POINTER(a.amc_frame_t)()
        start = time.perf_counter()
        check(camera.amc_decoder_receive(dec, 0, c.byref(frame)))
        receive_ms = (time.perf_counter() - start) * 1000
        meta, view = sized(a.am_frame_meta_t), sized(a.am_frame_view_t)
        check(camera.amc_frame_get_meta(frame, c.byref(meta)))
        check(camera.amc_frame_get_view(frame, c.byref(view)))
        check(camera.amc_decoder_get_info(dec, c.byref(info)))
        assert bool(info.hw_accelerated) == (expect_hw and not args.software), (filename, info.backend_name_utf8)
        assert meta.pts == 123 and view.width == 1920 and view.height == 1080
        assert meta.source_generation == (1 if expect_hw or args.software else 2)
        assert info.config_generation == meta.config_generation
        checksum = hashlib.sha256(c.string_at(view.planes[0], view.plane_size_bytes[0])).hexdigest()
        check(camera.amc_frame_release(frame))
        final = c.POINTER(a.amc_frame_t)()
        assert camera.amc_decoder_receive(dec, 0, c.byref(final)) == a.AM_ERR_SDK_DRAINED
        lifecycle = c.c_uint32()
        check(camera.amc_decoder_close(dec, 100, c.byref(lifecycle)))
        check(camera.amc_decoder_get_info(dec, c.byref(info)))
        check(camera.amc_object_destroy(dec))
        rows.append({"file": filename, "actual_backend": info.backend_name_utf8.decode(),
                     "hardware": bool(info.hw_accelerated), "generation": meta.source_generation,
                     "create_ms": create_ms, "first_receive_ms": receive_ms,
                     "pts": meta.pts, "closed_info_readable": True, "rgba_sha256": checksum})
    lifecycle = c.c_uint32()
    check(camera.amc_shutdown(ctx, 100, c.byref(lifecycle)))
    assert lifecycle.value == a.AM_LIFECYCLE_CLOSED
    check(camera.amc_object_destroy(ctx))
    print(json.dumps({"cases": rows, "policy": "software-only" if args.software else "prefer-hardware",
                      "scope": "single AU; actual hardware required for prefer-hardware; not throughput acceptance"}))
    search.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", required=True)
    parser.add_argument("--runtime-dir", required=True)
    parser.add_argument("--samples-dir", required=True)
    parser.add_argument("--software", action="store_true", help="same-build CPU control, not historical-binary comparison")
    main(parser.parse_args())
