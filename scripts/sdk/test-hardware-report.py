"""ABI 1.1实际探测/快照与1.0初始化兼容；仅显式本地合成数据。"""
import argparse
import ctypes as c
import json
import os
from pathlib import Path
import sys
import threading
import time
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
import amrt_sdk as a
CONTRACT = json.loads((ROOT / "sdk/abi/decode-hardware-report-v1.schema.json").read_text(encoding="utf-8"))


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def check(code):
    assert code == 0, code


def snapshot(lib, owner):
    count = c.c_uint64()
    check(lib.amc_get_decode_hardware_report(owner, None, 0, c.byref(count)))
    assert 1 <= count.value <= 65536
    short = c.create_string_buffer(b"x", 1)
    assert lib.amc_get_decode_hardware_report(owner, short, 1, c.byref(count)) == a.AM_ERR_SDK_LIMIT_OUT_OF_RANGE
    assert short.raw == b"\0"
    output = c.create_string_buffer(count.value + 16)
    for i in range(count.value, len(output)):
        output[i] = b"z"
    check(lib.amc_get_decode_hardware_report(owner, output, count.value, c.byref(count)))
    assert output.raw[count.value:] == b"z" * 16
    value = json.loads(output.value)
    assert value["schema"] == "framebraid.decode-hardware/1"
    assert set(CONTRACT["required"]).issubset(value)
    for name in ("kind", "state", "actual_backend"):
        assert value[name] in CONTRACT["properties"][name]["enum"]
    assert type(value["stream_verified"]) is bool
    if value["kind"] == "source":
        for name in ("requested_policy", "effective_policy"):
            assert value[name] in (0, 1)
        assert type(value["decoder_active"]) is bool
        assert type(value["policy_locked"]) is bool
        assert value["decode_generation"] >= 1
        if value["state"] == "closed":
            assert value["actual_backend"] is None and value["selection"] is None
            assert not value["decoder_active"] and not value["stream_verified"]
    if value["selection"] is not None:
        contract = CONTRACT["properties"]["selection"]["oneOf"][1]
        assert set(contract["required"]).issubset(value["selection"])
        assert value["selection"]["policy"] in contract["properties"]["policy"]["enum"]
    return value


def main(args):
    directory = os.add_dll_directory(str(Path(args.dll_dir).resolve()))
    lib = c.CDLL(str(Path(args.dll_dir).resolve() / "am_camera.dll"))
    assert not a.bind(lib, "amc_", require_all=True)
    assert lib.amc_get_abi_version() == 0x10001
    paths = (c.c_char_p * 1)(str(Path(args.runtime_dir).resolve()).encode())
    params = sized(a.amc_init_params_t)
    params.runtime_dirs_utf8, params.runtime_dir_count = paths, 1
    params.abi_version = 0x10000
    ctx = c.POINTER(a.amc_context_t)()
    check(lib.amc_init(c.byref(params), c.byref(ctx)))
    assert snapshot(lib, ctx)["state"] == "not_probed"
    probes = []
    for codec in (a.AM_CODEC_H264, a.AM_CODEC_H265):
        check(lib.amc_probe_decode_hardware(ctx, codec, 1500))
        value = snapshot(lib, ctx)
        assert value["state"] == "probe_complete" and not value["stream_verified"]
        assert value["actual_backend"] is None
        if args.require_hardware:
            assert value["selection"]["selected"] is not None
        # 普通查询不能悄悄重测或刷新完成时间。
        assert snapshot(lib, ctx) == value
        probes.append(value)
    if args.require_hardware:
        completed = []
        worker = threading.Thread(target=lambda: completed.append(lib.amc_probe_decode_hardware(ctx, a.AM_CODEC_H264, 1500)))
        worker.start()
        try:
            until = time.monotonic() + 2
            while worker.is_alive() and time.monotonic() < until:
                if snapshot(lib, ctx)["state"] == "not_probed":
                    break
                time.sleep(.001)
            else:
                raise AssertionError("could not observe the active native probe")
            assert lib.amc_probe_decode_hardware(ctx, a.AM_CODEC_H264, 1500) == a.AM_ERR_SDK_TRY_AGAIN
        finally:
            worker.join(timeout=10)
        assert not worker.is_alive() and completed == [0]
    cases = []
    for codec, filename, hardware in ((a.AM_CODEC_H264, "frame.h264", True),
                                      (a.AM_CODEC_H265, "frame.h265", False)):
        config = sized(a.am_decoder_config_t)
        config.codec, config.hw_policy = codec, a.AM_DHW_PREFER_HW
        config.output_pixel_format = a.AM_PF_RGBA8
        config.time_base_num, config.time_base_den = 1, 1000
        decoder = c.POINTER(a.amc_decoder_t)()
        check(lib.amc_decoder_create(ctx, c.byref(config), c.byref(decoder)))
        before = snapshot(lib, decoder)
        assert before["state"] == "initialized" and not before["stream_verified"]
        data = Path(args.samples_dir, filename).read_bytes()
        owned = (c.c_ubyte * len(data)).from_buffer_copy(data)
        packet = sized(a.am_decoder_input_t)
        packet.data, packet.size, packet.flags = owned, len(data), a.AM_PKT_AU_COMPLETE
        packet.pts = packet.dts = 123
        check(lib.amc_decoder_submit(decoder, c.byref(packet)))
        check(lib.amc_decoder_drain(decoder))
        frame = c.POINTER(a.amc_frame_t)()
        check(lib.amc_decoder_receive(decoder, 0, c.byref(frame)))
        actual = snapshot(lib, decoder)
        assert actual["stream_verified"]
        if hardware and args.require_hardware:
            assert actual["actual_backend"] in ("ffmpeg-d3d11va", "ffmpeg-dxva2")
        if not hardware and args.require_hardware:
            assert actual["state"] == "recovered_cpu" and actual["last_failure"] is not None
            assert actual["actual_backend"] == "ffmpeg-cpu"
        check(lib.amc_frame_release(frame))
        state = c.c_uint32()
        check(lib.amc_decoder_close(decoder, 100, c.byref(state)))
        closed = snapshot(lib, decoder)
        assert closed["state"] == "closed"
        assert closed["selection"] == actual["selection"]
        check(lib.amc_object_destroy(decoder))
        cases.append(actual)
    state = c.c_uint32()
    check(lib.amc_shutdown(ctx, 100, c.byref(state)))
    assert state.value == a.AM_LIFECYCLE_CLOSED
    check(lib.amc_object_destroy(ctx))
    # 新客户端请求未来ABI时不能按现有布局继续运行。
    params.abi_version = 0x10002
    rejected = c.POINTER(a.amc_context_t)()
    assert lib.amc_init(c.byref(params), c.byref(rejected)) == a.AM_ERR_SDK_ABI_UNSUPPORTED
    assert not rejected
    print(json.dumps({"old_abi_init": True, "future_abi_rejected": True,
                      "probes": probes, "decoder_cases": cases, "canaries": True}))
    directory.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", required=True)
    parser.add_argument("--runtime-dir", required=True)
    parser.add_argument("--samples-dir", required=True)
    parser.add_argument("--require-hardware", action="store_true")
    main(parser.parse_args())
