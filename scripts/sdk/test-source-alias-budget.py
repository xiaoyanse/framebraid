"""真实DLL源别名预算/共享所有权验证；无凭据、不发起相机连接。"""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
import amrt_sdk as a


def check(code, expected=0):
    assert code == expected, (code, expected)


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def close(lib, value, context=False):
    state = c.c_uint32()
    check((lib.amc_shutdown if context else lib.amc_source_close)(value, 100, c.byref(state)))
    assert state.value == a.AM_LIFECYCLE_CLOSED
    check(lib.amc_object_destroy(value))


def run(lib, budget):
    params = sized(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    ctx = c.POINTER(a.amc_context_t)()
    check(lib.amc_init(c.byref(params), c.byref(ctx)))
    active = []
    try:
        policy = sized(a.am_queue_policy_t)
        check(lib.amc_get_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
        policy.max_bytes = budget
        check(lib.amc_set_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
        desc = sized(a.am_source_desc_t)
        desc.kind, desc.reuse = a.AM_SRC_ISAPI_URL, 1
        desc.url_utf8 = b"http://127.0.0.1:9/ISAPI/Streaming/channels/101/picture"
        codes, ids = [], []
        cycles = 257 if budget == 48 else 2
        for n in range(cycles):
            source = c.POINTER(a.amc_source_t)()
            code = lib.amc_source_open(ctx, c.byref(desc), c.byref(source))
            expected = a.AM_ERR_SDK_RESOURCE_EXHAUSTED if budget == 15 or (budget == 47 and n > 0) else 0
            check(code, expected)
            codes.append(code)
            if code:
                assert not source
                continue
            active.append(source)
            status = sized(a.am_source_status_t)
            check(lib.amc_source_get_status(source, c.byref(status)))
            assert status.state == a.AM_SRC_WAITING_CREDENTIALS
            ids.append(status.source_id)
            if n > 0:
                close(lib, active.pop())
        assert len(set(ids)) <= 1
        return {"control_bytes": budget, "open_codes": codes, "same_source_id": True,
                "accepted": codes.count(0), "rejected": len(codes) - codes.count(0)}
    finally:
        for source in reversed(active):
            close(lib, source)
        close(lib, ctx, context=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    args = parser.parse_args()
    dll = args.dll_dir.resolve() / "am_camera.dll"
    lib = c.CDLL(str(dll))
    a.bind(lib, "amc_", require_all=True)
    rows = [run(lib, budget) for budget in (15, 47, 48)]
    print(json.dumps({"passed": True, "dll_sha256": hashlib.sha256(dll.read_bytes()).hexdigest(),
                      "cases": rows}))


if __name__ == "__main__":
    main()
