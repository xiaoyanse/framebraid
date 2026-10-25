"""独立解码DLL到共享帧实调；样本和运行库目录必须显式传入。"""
import argparse
import ctypes as c
import json
import os
from pathlib import Path
import sys
sys.dont_write_bytecode = True
import time
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdk" if (ROOT / "sdk/bindings").is_dir() else ROOT
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a
from importlib.util import spec_from_file_location, module_from_spec
spec = spec_from_file_location("shared_test", Path(__file__).with_name("test-shared-frames.py"))
shared = module_from_spec(spec)
spec.loader.exec_module(shared)


def qpc_ms():
    kernel = c.WinDLL("kernel32")
    counter, frequency = c.c_int64(), c.c_int64()
    assert kernel.QueryPerformanceCounter(c.byref(counter))
    assert kernel.QueryPerformanceFrequency(c.byref(frequency))
    return counter.value * 1000 // frequency.value


def check(code, expected=0):
    assert code == expected, (code, expected)


def bframes(camera, args, codec, filename):
    path = Path(args.samples_dir, filename)
    reference_dir = getattr(args, "reference_dir", None)
    if reference_dir:
        raw = json.loads(Path(reference_dir, filename + ".json").read_text(encoding="utf-8"))
    else:
        raw = json.loads(subprocess.check_output(["ffprobe", "-v", "error", "-select_streams", "v:0",
            "-show_streams", "-show_packets", "-show_data", "-of", "json", str(path)], text=True))
    stream, packets = raw["streams"][0], raw["packets"]
    num, den = map(int, stream["time_base"].split("/"))
    p = shared.sized(a.amc_init_params_t)
    p.abi_version = a.AM_ABI_VERSION
    dirs = (c.c_char_p * 1)(str(Path(args.runtime_dir).resolve()).encode())
    p.runtime_dirs_utf8, p.runtime_dir_count = dirs, 1
    ctx = c.POINTER(a.amc_context_t)()
    check(camera.amc_init(c.byref(p), c.byref(ctx)))
    cfg = shared.sized(a.am_decoder_config_t)
    cfg.codec, cfg.output_pixel_format, cfg.hw_policy = codec, a.AM_PF_RGBA8, a.AM_DHW_SOFTWARE_ONLY
    cfg.queue_max_items, cfg.queue_max_bytes = 32, 16*1024*1024
    cfg.time_base_num, cfg.time_base_den = num, den
    dec = c.POINTER(a.amc_decoder_t)()
    check(camera.amc_decoder_create(ctx, c.byref(cfg), c.byref(dec)))
    for row in packets:
        encoded = bytearray()
        for line in row["data"].splitlines():
            if ":" in line:
                encoded.extend(bytes.fromhex(line.split(":", 1)[1].strip().split("  ", 1)[0].replace(" ", "")))
        annex, at = bytearray(), 0
        while at < len(encoded):
            size = int.from_bytes(encoded[at:at+4], "big")
            at += 4
            assert size > 0 and at+size <= len(encoded)
            annex += b"\x00\x00\x00\x01" + encoded[at:at+size]
            at += size
        data = (c.c_ubyte * len(annex)).from_buffer_copy(annex)
        packet = shared.sized(a.am_decoder_input_t)
        packet.data, packet.size, packet.flags = data, len(annex), a.AM_PKT_AU_COMPLETE
        packet.pts, packet.dts = int(row["pts"]), int(row["dts"])
        check(camera.amc_decoder_submit(dec, c.byref(packet)))
    check(camera.amc_decoder_drain(dec))
    decoded, timestamps = [], []
    while True:
        frame = c.POINTER(a.amc_frame_t)()
        code = camera.amc_decoder_receive(dec, 0, c.byref(frame))
        if code == a.AM_ERR_SDK_DRAINED:
            break
        check(code)
        view, meta = shared.sized(a.am_frame_view_t), shared.sized(a.am_frame_meta_t)
        check(camera.amc_frame_get_view(frame, c.byref(view)))
        check(camera.amc_frame_get_meta(frame, c.byref(meta)))
        decoded.append(c.string_at(view.planes[0], view.plane_size_bytes[0]))
        timestamps.append(meta.pts)
        check(camera.amc_frame_release(frame))
    if reference_dir:
        reference = Path(reference_dir, filename + ".rgba").read_bytes()
    else:
        reference = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(path), "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
    actual = b"".join(decoded)
    assert len(actual) == len(reference) and len(decoded) == len(packets)
    error = max(abs(x-y) for x, y in zip(actual, reference))
    assert error <= 2, error
    assert timestamps == sorted(int(row["pts"]) for row in packets), timestamps
    state = c.c_uint32()
    check(camera.amc_decoder_close(dec, 100, c.byref(state)))
    check(camera.amc_object_destroy(dec))
    shared.close(camera, ctx, True)
    return {"codec": codec, "frames": len(decoded), "maxRgbaError": error, "reorderedPtsExact": True}


def main(args):
    dll_directory = os.add_dll_directory(str(Path(args.dll_dir).resolve()))
    camera = c.CDLL(str(Path(args.dll_dir).resolve() / "am_camera.dll"))
    vision = c.CDLL(str(Path(args.dll_dir).resolve() / "am_vision.dll"))
    a.bind(camera, "amc_")
    a.bind(vision, "amv_")
    rows = []
    for codec, filename in [(a.AM_CODEC_H264, "one.h264"), (a.AM_CODEC_H265, "one.hevc")]:
        p = shared.sized(a.amc_init_params_t)
        p.abi_version, p.feature_flags = a.AM_ABI_VERSION, a.AM_INIT_SHARED_FRAMES
        directory = str(Path(args.runtime_dir).resolve()).encode()
        dirs = (c.c_char_p * 1)(directory)
        p.runtime_dirs_utf8, p.runtime_dir_count = dirs, 1
        ctx = c.POINTER(a.amc_context_t)()
        check(camera.amc_init(c.byref(p), c.byref(ctx)))
        cfg = shared.sized(a.am_decoder_config_t)
        cfg.codec, cfg.output_pixel_format = codec, a.AM_PF_RGBA8
        cfg.hw_policy = a.AM_DHW_SOFTWARE_ONLY
        cfg.queue_max_items, cfg.queue_max_bytes = 1, 16 * 1024 * 1024
        cfg.time_base_num, cfg.time_base_den = 1, 90000
        dec = c.POINTER(a.amc_decoder_t)()
        bad_output = (c.c_ubyte * 24)()
        unaligned = c.cast(c.byref(bad_output, 1), c.POINTER(c.POINTER(a.amc_decoder_t)))
        check(camera.amc_decoder_create(ctx, c.byref(cfg), unaligned), a.AM_ERR_SDK_INVALID_ARGUMENT)
        check(camera.amc_decoder_create(ctx, c.byref(cfg), c.byref(dec)))
        info = shared.sized(a.am_decoder_info_t)
        check(camera.amc_decoder_get_info(dec, c.byref(info)))
        assert info.hw_accelerated == 0 and info.backend_name_utf8 == b"ffmpeg-cpu"
        assert info.codec == codec
        encoded = Path(args.samples_dir, filename).read_bytes()
        data = (c.c_ubyte * len(encoded)).from_buffer_copy(encoded)
        packet = shared.sized(a.am_decoder_input_t)
        packet.data, packet.size = data, len(encoded)
        packet.pts, packet.dts = 90000, 88000
        packet.flags = a.AM_PKT_AU_COMPLETE
        bad = shared.sized(a.am_decoder_input_t)
        bad.data, bad.size, bad.pts, bad.dts = data, len(encoded), 90000, 90000
        check(camera.amc_decoder_submit(dec, c.byref(bad)), a.AM_ERR_CAMERA_CODEC_UNSUPPORTED)
        before = qpc_ms()
        check(camera.amc_decoder_submit(dec, c.byref(packet)))
        after = qpc_ms()
        check(camera.amc_decoder_submit(dec, c.byref(packet)), a.AM_ERR_SDK_QUEUE_FULL)
        c.memset(data, 0, len(encoded))  # AM_OK后不再借用调用方的压缩缓冲。
        time.sleep(.03)
        frame = c.POINTER(a.amc_frame_t)()
        code = camera.amc_decoder_receive(dec, 0, c.byref(frame))
        if code == a.AM_ERR_SDK_TRY_AGAIN:
            check(camera.amc_decoder_drain(dec))
            check(camera.amc_decoder_receive(dec, 0, c.byref(frame)))
        else:
            check(code)
        view, meta = shared.sized(a.am_frame_view_t), shared.sized(a.am_frame_meta_t)
        check(camera.amc_frame_get_view(frame, c.byref(view)))
        check(camera.amc_frame_get_meta(frame, c.byref(meta)))
        assert (view.width, view.height, meta.pts) == (256, 144, 90000)
        assert (meta.time_base_num, meta.time_base_den) == (1, 90000)
        assert before <= meta.received_at_ms <= after, (before, meta.received_at_ms, after)
        assert meta.source_generation == 1 and meta.frame_id != 0
        assert meta.seq == 1
        pixels = c.string_at(view.planes[0], view.plane_size_bytes[0])
        assert all(pixels[i] >= 250 and pixels[i+1] <= 3 and pixels[i+2] <= 3 and pixels[i+3] == 255
                   for i in range(0, len(pixels), 4))
        vctx = shared.init(vision, False)
        pool_cfg = shared.sized(a.am_shared_pool_config_t)
        pool_cfg.slot_count, pool_cfg.max_frame_bytes = 1, len(pixels)
        pool_cfg.total_bytes, pool_cfg.max_consumers, pool_cfg.max_outstanding_borrows = len(pixels), 1, 2
        attach = shared.sized(a.am_shared_pool_attach_desc_t)
        pool = c.POINTER(a.amc_shared_pool_t)()
        check(camera.amc_shared_pool_create(ctx, c.byref(pool_cfg), c.byref(attach), c.byref(pool)))
        publication = c.c_uint64()
        check(camera.amc_shared_pool_publish(pool, frame, c.byref(publication)))
        consumer = shared.attach(vision, vctx, attach)
        borrowed = shared.borrow(vision, consumer, publication.value)
        assert borrowed.desc.frame_id == meta.frame_id
        assert borrowed.desc.received_at_ms == meta.received_at_ms
        assert c.string_at(borrowed.plane_data[0], len(pixels)) == pixels
        check(vision.amv_shared_pool_return(consumer, c.byref(borrowed)))
        shared.close(vision, consumer, False, True)
        shared.close(camera, pool, True, True)
        shared.close(vision, vctx, False)
        # 重置后旧帧仍可读，且新帧源代次递增，不能混成同一帧。
        check(camera.amc_decoder_reset(dec, None))
        assert c.string_at(view.planes[0], 4) == pixels[:4]
        data[:] = encoded
        packet.pts = -(2**63)
        packet.dts = -(2**63)
        check(camera.amc_decoder_submit(dec, c.byref(packet)))
        check(camera.amc_decoder_drain(dec))
        second = c.POINTER(a.amc_frame_t)()
        check(camera.amc_decoder_receive(dec, 0, c.byref(second)))
        next_meta = shared.sized(a.am_frame_meta_t)
        check(camera.amc_frame_get_meta(second, c.byref(next_meta)))
        assert next_meta.source_id == meta.source_id and next_meta.source_generation == 2
        assert next_meta.frame_id != meta.frame_id
        assert next_meta.seq == 1
        assert next_meta.pts == -(2**63) and next_meta.clock_mask & a.AM_CLK_PTS == 0
        assert next_meta.time_base_num == next_meta.time_base_den == 0
        check(camera.amc_frame_release(second))
        check(camera.amc_decoder_receive(dec, 0, c.byref(second)), a.AM_ERR_SDK_DRAINED)
        check(camera.amc_decoder_submit(dec, c.byref(packet)), a.AM_ERR_CAMERA_DECODER_STATE_INVALID)
        state = c.c_uint32()
        check(camera.amc_decoder_close(dec, 100, c.byref(state)))
        assert state.value == a.AM_LIFECYCLE_CLOSED
        check(camera.amc_object_destroy(dec))
        check(camera.amc_shutdown(ctx, 0, c.byref(state)))
        assert state.value == a.AM_LIFECYCLE_CLOSING  # 外部CPU帧仍占用ctx预算。
        check(camera.amc_frame_release(frame))
        shared.close(camera, ctx, True)
        rows.append({"codec": codec, "rgbaBytes": len(pixels), "pts": meta.pts,
                     "decodeToSharedByteExact": True, "resetKeepsOldFrame": True,
                     "receivedAtIsSubmitTime": True})
    reordered = [bframes(camera, args, codec, filename) for codec, filename in [
        (a.AM_CODEC_H264, "multi-h264.mp4"), (a.AM_CODEC_H265, "multi-h265.mp4")]]
    print(json.dumps({"nativeDecoders": rows, "bFrameReference": reordered,
                      "hardware": "software-only; no hardware acceptance"}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", required=True)
    parser.add_argument("--runtime-dir", required=True)
    parser.add_argument("--samples-dir", required=True)
    parser.add_argument("--reference-dir", help="预生成的ffprobe JSON与FFmpeg RGBA参考，供无CLI测试机使用")
    main(parser.parse_args())
