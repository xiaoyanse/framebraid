"""独立解码输入队列统计与reset/close出账；使用显式合成样本和运行库。"""
import argparse
import ctypes as c
import hashlib
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


def counters(lib, ctx):
    value = c.POINTER(a.am_stats_t)()
    check(lib.amc_get_stats(ctx, None, c.byref(value)))
    try:
        row = value.contents
        return {name: getattr(row, name) for name in ["accepted", "rejected_backpressure", "queue_items", "queue_bytes"]}
    finally:
        check(lib.amc_stats_release(value))


def admissions(lib, args):
    rows = []
    data = args.sample.read_bytes()
    storage = (c.c_ubyte * len(data)).from_buffer_copy(data)
    for mode, accepted, rejected in [("items", 2, a.AM_ERR_SDK_QUEUE_FULL),
                                      ("bytes", 1, a.AM_ERR_SDK_QUEUE_FULL),
                                      ("domain", 0, a.AM_ERR_SDK_RESOURCE_EXHAUSTED)]:
        ctx, dec = initialize(lib, args.runtime_dir), c.POINTER(a.amc_decoder_t)()
        try:
            if mode == "domain":
                limits = size(a.am_limits_t)
                check(lib.amc_get_limits(ctx, c.byref(limits)))
                limits.frame_budget_bytes = len(data) + 63
                check(lib.amc_set_limits(ctx, c.byref(limits)))
            cfg = size(a.am_decoder_config_t)
            cfg.codec, cfg.output_pixel_format, cfg.hw_policy = a.AM_CODEC_H264, a.AM_PF_RGBA8, a.AM_DHW_SOFTWARE_ONLY
            cfg.queue_max_items, cfg.queue_max_bytes = 2, len(data) + 64 if mode == "bytes" else 1024 * 1024
            check(lib.amc_decoder_create(ctx, c.byref(cfg), c.byref(dec)))
            packet = size(a.am_decoder_input_t)
            packet.data, packet.size, packet.flags = storage, len(data), a.AM_PKT_AU_COMPLETE
            packet.pts, packet.dts = -9223372036854775808, -9223372036854775808
            for _ in range(accepted):
                check(lib.amc_decoder_submit(dec, c.byref(packet)))
            check(lib.amc_decoder_submit(dec, c.byref(packet)), rejected)
            before_invalid = counters(lib, ctx)
            check(lib.amc_decoder_submit(dec, None), a.AM_ERR_SDK_NULL_ARGUMENT)
            after_invalid = counters(lib, ctx)
            assert before_invalid == after_invalid
            assert after_invalid["queue_items"] == accepted
            assert after_invalid["queue_bytes"] == accepted * (len(data) + 64)
            row = {"mode": mode, "accepted_calls": accepted, "rejected_calls": 1,
                   "rejected_code": rejected, "snapshot": after_invalid}
            check(lib.amc_decoder_reset(dec, None))
            after_reset = counters(lib, ctx)
            assert after_reset["queue_items"] == after_reset["queue_bytes"] == 0
            assert all(after_reset[n] == after_invalid[n] for n in ["accepted", "rejected_backpressure"])
            row["after_reset"] = after_reset
            rows.append(row)
        finally:
            if dec:
                t.close_object(lib, dec)
            t.shared.close(lib, ctx, True)
    return rows


def run(args):
    lib = c.CDLL(str((args.dll_dir / "am_camera.dll").resolve()))
    a.bind(lib, "amc_")
    admission_rows = admissions(lib, args)
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
                      "resetAndCloseUnaccount": True, "contextsIsolated": True, "admissions": admission_rows,
                      "identity": {"dll_sha256": hashlib.sha256((args.dll_dir / "am_camera.dll").read_bytes()).hexdigest(),
                                   "sample_sha256": hashlib.sha256(args.sample.read_bytes()).hexdigest()}}))
    if args.require_admission_stats:
        for row in admission_rows:
            assert row["snapshot"]["accepted"] == row["accepted_calls"], row
            assert row["snapshot"]["rejected_backpressure"] == row["rejected_calls"], row


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--sample", type=Path, required=True)
    parser.add_argument("--require-admission-stats", action="store_true")
    run(parser.parse_args())
