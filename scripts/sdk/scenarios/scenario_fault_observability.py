"""R8 场景：坏数据源状态转 RECONNECT_WAIT 并携带 fault 分类，恢复后回到 LIVE。

HTTP 替身可切三种模式（正常/HTTP 500/坏载荷），驱动抓图失败并轮询源状态：
进入 RECONNECT_WAIT 且 fault 类别可观测（500 → network；坏载荷在图片通道
当前也映射 network，观测值如实记录供复核）；恢复供图后状态回 LIVE、抓图成功。
判读：退出码 0=过。
用法：python scenario_fault_observability.py --dll-dir <release> --runtime-dir <ffmpeg bin>
      [--output result.json]
"""
import argparse
import ctypes as c
import importlib.util
import json
import platform
import sys
import threading
import time
from http.server import ThreadingHTTPServer
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "sdk" / "bindings" / "python"))
import amrt_sdk as a  # noqa: E402

spec = importlib.util.spec_from_file_location("capture_fixture", ROOT / "scripts" / "sdk" / "test-camera-capture.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)

BAD_PAYLOAD = b"MJPEG-fault-fixture-not-a-jpeg"
FAULT_DEADLINE_S = 15
FAULT_NAMES = {
    a.AM_FAULT_NONE: "none",
    a.AM_FAULT_NETWORK: "network",
    a.AM_FAULT_AUTH: "auth",
    a.AM_FAULT_RESOURCE: "resource",
    a.AM_FAULT_KEYFRAME_WAIT: "keyframe_wait",
    a.AM_FAULT_DECODE: "decode",
}


class FaultyFixture(fixture.Fixture):
    mode = "good"

    def do_GET(self):
        mode = FaultyFixture.mode
        if mode == "http_500":
            self.send_response(500)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if not self.headers.get("Authorization", "").startswith("Digest "):
            self.send_response(401)
            self.send_header("WWW-Authenticate", 'Digest realm="sdk-test", nonce="abcdef", qop="auth", algorithm=MD5')
            self.end_headers()
            return
        if mode == "bad_payload":
            self.send_response(200)
            self.send_header("Content-Type", "image/jpeg")
            self.send_header("Content-Length", str(len(BAD_PAYLOAD)))
            self.end_headers()
            self.wfile.write(BAD_PAYLOAD)
            return
        fixture.Fixture.do_GET(self)


def checked(lib, code, stage=""):
    if code != 0:
        name = lib.amc_status_name(code)
        message = name.decode() if name else ""
        raise AssertionError(f"{stage} code={code} {message}")


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def init_context(dll_dir, runtime_dir):
    lib = c.CDLL(str((dll_dir / "am_camera.dll").resolve()))
    a.bind(lib, "amc_")
    dirs = (c.c_char_p * 1)(str(runtime_dir.resolve()).encode())
    params = sized(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    params.runtime_dirs_utf8, params.runtime_dir_count = dirs, 1
    ctx = c.POINTER(a.amc_context_t)()
    checked(lib, lib.amc_init(c.byref(params), c.byref(ctx)), "amc_init")
    return lib, ctx


def status_of(lib, source):
    status = sized(a.am_source_status_t)
    checked(lib, lib.amc_source_get_status(source, c.byref(status)), "source_status")
    return status


def recover(lib, source):
    # 图片通道状态只在抓图尝试时更新，恢复期必须持续抓图才能回到 LIVE
    until = time.monotonic() + FAULT_DEADLINE_S
    while time.monotonic() < until:
        ok, _ = try_capture(lib, source)
        status = status_of(lib, source)
        if ok and status.state == a.AM_SRC_LIVE:
            return status
        time.sleep(0.05)
    raise AssertionError(f"恢复未回 LIVE，最后={last_status(lib, source)}")


def try_capture(lib, source):
    """抓一帧；返回 (成功?, 错误码)。故障注入期失败属预期。"""
    req = sized(a.am_capture_request_t)
    req.output_kind, req.deadline_ms = 0, 1000
    out = c.POINTER(a.am_captured_image_t)()
    code = lib.amc_capture_sync(source, c.byref(req), c.byref(out))
    if code != 0:
        return False, code
    checked(lib, lib.amc_image_release(out), "image_release")
    return True, 0


def capture_ok(lib, source):
    ok, code = try_capture(lib, source)
    assert ok, f"正常期抓图失败 code={code}"


def drive_fault(lib, source, mode):
    FaultyFixture.mode = mode
    try:
        until = time.monotonic() + FAULT_DEADLINE_S
        errors = []
        while time.monotonic() < until:
            ok, code = try_capture(lib, source)
            if not ok:
                errors.append(code)
            status = status_of(lib, source)
            if status.state == a.AM_SRC_RECONNECT_WAIT:
                assert status.fault_class != a.AM_FAULT_NONE, \
                    f"{mode}: RECONNECT_WAIT 但 fault 不可观测"
                return {
                    "mode": mode,
                    "state": "RECONNECT_WAIT",
                    "observed_fault_class": FAULT_NAMES.get(status.fault_class, status.fault_class),
                    "reconnect_attempts": status.reconnect_attempts,
                    "capture_error_codes": sorted(set(errors)),
                }
            time.sleep(0.05)
        raise AssertionError(f"{mode}: {FAULT_DEADLINE_S}s 内未进入 RECONNECT_WAIT，最后={last_status(lib, source)}")
    finally:
        FaultyFixture.mode = "good"


def last_status(lib, source):
    status = status_of(lib, source)
    return (status.state, FAULT_NAMES.get(status.fault_class, status.fault_class), status.reconnect_attempts)


def case_fault_cycle(lib, ctx, port, mode):
    source = fixture.open_source(lib, ctx, port)
    try:
        capture_ok(lib, source)
        before = status_of(lib, source)
        assert before.state == a.AM_SRC_LIVE, before.state
        row = drive_fault(lib, source, mode)
        recover(lib, source)
        capture_ok(lib, source)
        after = status_of(lib, source)
        row["generation"] = [before.generation, after.generation]
        row["recovered_state"] = "LIVE"
        return row
    finally:
        fixture.close_object(lib, source)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    stage = "init"
    try:
        lib, ctx = init_context(args.dll_dir, args.runtime_dir)
        server = None
        result = {"ok": True, "schema": "framebraid.scenario-fault/1", "os": platform.platform(),
                  "cases": []}
        try:
            stage = "server"
            server = ThreadingHTTPServer(("127.0.0.1", 0), FaultyFixture)
            server.daemon_threads = True
            threading.Thread(target=server.serve_forever, daemon=True).start()
            port = server.server_address[1]
            stage = "http_500"
            row = case_fault_cycle(lib, ctx, port, "http_500")
            result["cases"].append(row)
            print(f"[fault] http_500 -> RECONNECT_WAIT/{row['observed_fault_class']} "
                  f"attempts={row['reconnect_attempts']} 恢复代次 {row['generation']}", flush=True)
            stage = "bad_payload"
            row = case_fault_cycle(lib, ctx, port, "bad_payload")
            result["cases"].append(row)
            print(f"[fault] bad_payload -> RECONNECT_WAIT/{row['observed_fault_class']} "
                  f"attempts={row['reconnect_attempts']} 恢复代次 {row['generation']}", flush=True)
            if args.output:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps(result, ensure_ascii=False, indent=1), encoding="utf-8")
            print(json.dumps(result, ensure_ascii=False))
        finally:
            if server is not None:
                server.shutdown()
                server.server_close()
            fixture.shared.close(lib, ctx, True)
    except Exception as error:
        print(json.dumps({"ok": False, "stage": stage, "error": repr(error)}, ensure_ascii=False))
        sys.exit(1)


if __name__ == "__main__":
    main()
