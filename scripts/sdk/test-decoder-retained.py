"""真实软件解码的配置预算与失败重置；只读合成样本，无网络/现场设备。"""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import re
import sys
import time

sys.dont_write_bytecode = True
from camera_recovery_fixture import module
from local_paths import local_path

t = module("test-decoder.py")
a, check, sized = t.a, t.check, t.shared.sized


def initialize(lib, runtime):
    params = sized(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    dirs = (c.c_char_p * 1)(str(runtime.resolve()).encode())
    params.runtime_dirs_utf8, params.runtime_dir_count = dirs, 1
    ctx = c.POINTER(a.amc_context_t)()
    check(lib.amc_init(c.byref(params), c.byref(ctx)))
    limits = sized(a.am_limits_t)
    check(lib.amc_get_limits(ctx, c.byref(limits)))
    limits.frame_budget_bytes = 1024 * 1024
    check(lib.amc_set_limits(ctx, c.byref(limits)))
    return ctx


def config(extra):
    data = (c.c_ubyte * len(extra)).from_buffer_copy(extra)
    cfg = sized(a.am_decoder_config_t)
    cfg.codec, cfg.bitstream_format = a.AM_CODEC_H264, a.AM_BSF_ANNEXB
    cfg.output_pixel_format, cfg.hw_policy = a.AM_PF_RGBA8, a.AM_DHW_SOFTWARE_ONLY
    cfg.extradata, cfg.extradata_size = data, len(data)
    return cfg, data


def status(lib, decoder):
    value = sized(a.am_decoder_status_t)
    check(lib.amc_decoder_get_status(decoder, c.byref(value)))
    return [value.state, value.decoder_generation, value.config_generation]


def picture(lib, decoder, sample):
    data = (c.c_ubyte * len(sample)).from_buffer_copy(sample)
    packet = sized(a.am_decoder_input_t)
    packet.data, packet.size, packet.flags = data, len(data), a.AM_PKT_AU_COMPLETE
    packet.pts = packet.dts = -(2**63)
    check(lib.amc_decoder_submit(decoder, c.byref(packet)))
    frame = c.POINTER(a.amc_frame_t)()
    until = time.monotonic() + 5
    while True:
        code = lib.amc_decoder_receive(decoder, 0, c.byref(frame))
        if code == 0:
            return frame
        check(code, a.AM_ERR_SDK_TRY_AGAIN)
        assert time.monotonic() < until
        time.sleep(.002)


def pixels(lib, frame):
    view = sized(a.am_frame_view_t)
    check(lib.amc_frame_get_view(frame, c.byref(view)))
    return c.string_at(view.planes[0], view.plane_size_bytes[0])


def main(args):
    out = local_path(args.output.resolve(), "evidence", fresh=True)
    out.mkdir(parents=True)
    (out / "target-card.json").write_text(json.dumps({"metric": "MET-2 M-RECOVER S5",
        "frame_budget": 1024**2, "extra_filler": 2 * 1024**2,
        "oversized_config": "resourceExhausted before copy", "failed_reset": "old state and owned pixels unchanged",
        "scope": "real H264 CPU, configuration ownership; not hardware/soak"}, indent=2), encoding="utf-8")
    sample = args.sample.read_bytes()
    starts = list(re.finditer(b"\x00\x00(?:\x00)?\x01", sample))
    nals = [sample[m.end():starts[i + 1].start() if i + 1 < len(starts) else len(sample)]
            for i, m in enumerate(starts)]
    small = b"".join(b"\0\0\0\1" + n for n in nals if n and n[0] & 31 in [7, 8])
    assert small and len(small) < 1024
    big = small + b"\0\0\0\1\x0c" + b"\xff" * (2 * 1024**2) + b"\x80"
    dll = args.dll_dir.resolve() / "am_camera.dll"
    lib = c.CDLL(str(dll))
    a.bind(lib, "amc_")
    result = {"dll_sha256": hashlib.sha256(dll.read_bytes()).hexdigest(),
              "sample_sha256": hashlib.sha256(sample).hexdigest(), "extra_bytes": len(big)}
    for reset in [False, True]:
        ctx, decoder, held = initialize(lib, args.runtime_dir), c.POINTER(a.amc_decoder_t)(), []
        try:
            cfg, owner = config(small if reset else big)
            code = lib.amc_decoder_create(ctx, c.byref(cfg), c.byref(decoder))
            if not reset:
                result["create_status"] = code
                if not args.observe_baseline:
                    check(code, a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
                    assert not decoder
                continue
            check(code)
            first = picture(lib, decoder, sample)
            held.append(first)
            expected, before = pixels(lib, first), status(lib, decoder)
            large, large_owner = config(big)
            code = lib.amc_decoder_reset(decoder, c.byref(large))
            after = status(lib, decoder)
            result["reset"] = {"status": code, "before": before, "after": after}
            if not args.observe_baseline:
                check(code, a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
                assert before == after
            second = picture(lib, decoder, sample)
            held.append(second)
            assert pixels(lib, first) == pixels(lib, second) == expected
            check(lib.amc_decoder_reset(decoder, None))
            third = picture(lib, decoder, sample)
            held.append(third)
            assert pixels(lib, third) == expected
            result["owned_pixels_unchanged"] = True
            result["null_reset_ok"] = True
        finally:
            for frame in held:
                check(lib.amc_frame_release(frame))
            if decoder:
                state = c.c_uint32()
                check(lib.amc_decoder_close(decoder, 100, c.byref(state)))
                assert state.value == a.AM_LIFECYCLE_CLOSED
                check(lib.amc_object_destroy(decoder))
            t.shared.close(lib, ctx, True)
    (out / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps(result))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ["dll-dir", "runtime-dir", "sample", "output"]:
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--observe-baseline", action="store_true")
    main(parser.parse_args())
