"""独立解码元数据低预算拒绝；仅本仓合成数据和软件解码。"""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from camera_recovery_fixture import module
from local_paths import local_path

m = module("test-decoder-retained.py")
a, check, size = m.a, m.check, m.sized


def reduce_control(lib, ctx):
    policy = size(a.am_queue_policy_t)
    check(lib.amc_get_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
    policy.max_bytes = 1
    check(lib.amc_set_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))


def main(args):
    out = local_path(args.output.resolve(), "evidence", fresh=True)
    out.mkdir(parents=True)
    target = {"metric": "MET-2 M-RECOVER/S5", "control_max_bytes": 1,
              "create_and_reset": "resourceExhausted before new storage/native context",
              "protection": "old generations, held frame and continued software decoding"}
    (out / "target-card.json").write_bytes((json.dumps(target, indent=2) + "\n").encode())
    dll = (args.dll_dir / "am_camera.dll").resolve()
    lib = c.CDLL(str(dll))
    a.bind(lib, "amc_")
    sample = args.sample.read_bytes()
    rows = []
    for late in [False, True]:
        ctx = m.initialize(lib, args.runtime_dir)
        dec, frames = c.POINTER(a.amc_decoder_t)(), []
        try:
            if not late:
                reduce_control(lib, ctx)
            cfg, owner = m.config(b"")
            code = lib.amc_decoder_create(ctx, c.byref(cfg), c.byref(dec))
            if not late:
                check(code, a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
                assert not dec
                rows.append({"mode": "create", "status": code, "output_is_null": True})
                continue
            check(code)
            frames.append(m.picture(lib, dec, sample))
            expected = m.pixels(lib, frames[0])
            before = m.status(lib, dec)
            reduce_control(lib, ctx)
            code = lib.amc_decoder_reset(dec, None)
            check(code, a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
            assert m.status(lib, dec) == before
            frames.append(m.picture(lib, dec, sample))
            assert m.pixels(lib, frames[0]) == m.pixels(lib, frames[1]) == expected
            rows.append({"mode": "reset", "status": code, "before": before,
                         "after": m.status(lib, dec), "old_frame_and_continued_decode": True})
        finally:
            for frame in frames:
                check(lib.amc_frame_release(frame))
            if dec:
                state = c.c_uint32()
                check(lib.amc_decoder_close(dec, 100, c.byref(state)))
                assert state.value == a.AM_LIFECYCLE_CLOSED
                check(lib.amc_object_destroy(dec))
            m.t.shared.close(lib, ctx, True)
    result = {"dll_sha256": hashlib.sha256(dll.read_bytes()).hexdigest(),
              "sample_sha256": hashlib.sha256(sample).hexdigest(), "rows": rows}
    (out / "result.json").write_bytes((json.dumps(result, indent=2) + "\n").encode())
    print(json.dumps(result))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ["dll-dir", "runtime-dir", "sample", "output"]:
        parser.add_argument("--" + name, type=Path, required=True)
    main(parser.parse_args())
