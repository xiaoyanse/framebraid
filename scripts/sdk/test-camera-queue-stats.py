"""独立解码输入队列统计与reset/close出账；使用显式合成样本和运行库。"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("capture_test", Path(__file__).with_name("test-camera-capture.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
a, size, check = t.a, t.size, t.check


def initialize(lib, runtime):
    params = size(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    dirs = (c.c_char_p * 1)(str(runtime.resolve()).encode())
    params.runtime_dirs_utf8, params.runtime_dir_count = dirs, 1
    ctx = c.POINTER(a.amc_context_t)()
    check(lib.amc_init(c.byref(params), c.byref(ctx)))
    return ctx


def run(args):
    lib = c.CDLL(str((args.dll_dir / "am_camera.dll").resolve()))
    a.bind(lib, "amc_")
    ctx, other = initialize(lib, args.runtime_dir), initialize(lib, args.runtime_dir)
    cfg = size(a.am_decoder_config_t)
    cfg.codec, cfg.output_pixel_format, cfg.hw_policy = a.AM_CODEC_H264, a.AM_PF_RGBA8, a.AM_DHW_SOFTWARE_ONLY
    cfg.queue_max_items, cfg.queue_max_bytes = 2, 1024 * 1024
    cfg.time_base_num, cfg.time_base_den = 1, 90000
    dec = c.POINTER(a.amc_decoder_t)()
    check(lib.amc_decoder_create(ctx, c.byref(cfg), c.byref(dec)))
    data = args.sample.read_bytes()
    buffer = (c.c_ubyte * len(data)).from_buffer_copy(data)
    packet = size(a.am_decoder_input_t)
    packet.data, packet.size, packet.flags = buffer, len(data), a.AM_PKT_AU_COMPLETE
    packet.pts, packet.dts = 90000, 90000
    assert t.stats(lib, ctx) == (0, 0, 0)
    check(lib.amc_decoder_submit(dec, c.byref(packet)))
    assert t.stats(lib, ctx) == (1, len(data) + 64, 0)
    assert t.stats(lib, other) == (0, 0, 0)
    frame = c.POINTER(a.amc_frame_t)()
    code = lib.amc_decoder_receive(dec, 0, c.byref(frame))
    if code == a.AM_ERR_SDK_TRY_AGAIN:
        check(lib.amc_decoder_drain(dec))
        check(lib.amc_decoder_receive(dec, 0, c.byref(frame)))
    else:
        check(code)
    assert t.stats(lib, ctx) == (0, 0, 1)
    check(lib.amc_frame_release(frame))
    check(lib.amc_decoder_reset(dec, None))
    check(lib.amc_decoder_submit(dec, c.byref(packet)))
    assert t.stats(lib, ctx) == (1, len(data) + 64, 0)
    check(lib.amc_decoder_reset(dec, None))
    assert t.stats(lib, ctx) == (0, 0, 0)
    check(lib.amc_decoder_submit(dec, c.byref(packet)))
    t.close_object(lib, dec)
    assert t.stats(lib, ctx) == (0, 0, 0)
    t.shared.close(lib, other, True)
    t.shared.close(lib, ctx, True)
    print(json.dumps({"decoderInputAggregated": True, "receivedFrameIsExternal": True,
                      "resetAndCloseUnaccount": True, "contextsIsolated": True}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--sample", type=Path, required=True)
    run(parser.parse_args())
