"""探测前预算准入：合成HTTP与随机回环RTSP，不连接现场设备。"""
import argparse
import ctypes as c
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import subprocess
import time
from urllib.parse import quote
import threading

sys.dont_write_bytecode = True
from camera_recovery_fixture import Stream
from local_paths import local_path

spec = importlib.util.spec_from_file_location("probe_test", Path(__file__).with_name("test-camera-probe.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
a, h = t.a, t.h


def main(args):
    work = local_path(args.work_dir.resolve(), "tests", fresh=True)
    work.mkdir(parents=True)
    target = {"metric": "MET-2 M-RECOVER S5", "tiny_frame_budget_bytes": 64,
              "tiny_status": "pending", "fresh_default_context_status": "ok",
              "scope": "probe SDK buffers; native FFmpeg/network memory is separate"}
    (work / "target-card.json").write_text(json.dumps(target, indent=2), encoding="utf-8")
    dll = args.dll_dir.resolve() / "am_camera.dll"
    lib = c.CDLL(str(dll))
    a.bind(lib, "amc_")
    params = h.sized(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    dirs = (c.c_char_p * 1)(str(args.runtime_dir.resolve()).encode())
    params.runtime_dirs_utf8, params.runtime_dir_count = dirs, 1
    ctx = c.POINTER(a.amc_context_t)()
    server = t.FixtureServer(("127.0.0.1", 0), t.Fixture)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    result = {"dll_sha256": hashlib.sha256(dll.read_bytes()).hexdigest(), "cases": {}}
    try:
        with Stream(work, args.mediamtx.resolve()) as stream:
            # 进程存活不等于已发布首帧；先以独立读取确认夹具就绪，不重试SDK断言。
            ready_url = stream.url.replace("rtsp://", "rtsp://" + quote(stream.user.decode(), safe="") + ":" + quote(stream.password.decode(), safe="") + "@", 1)
            ready_until = time.monotonic() + 10
            while True:
                ready = subprocess.run(["ffprobe", "-v", "error", "-rtsp_transport", "tcp",
                                        "-show_entries", "stream=codec_name,width,height", "-of", "json", ready_url],
                                       capture_output=True, text=True, timeout=5)
                if ready.returncode == 0 and any(v.get("codec_name") == "h264" and v.get("width") == 256 and v.get("height") == 144 for v in json.loads(ready.stdout).get("streams", [])):
                    result["fixture_ready_ffprobe"] = True
                    break
                assert time.monotonic() < ready_until, "owned RTSP publisher did not become readable"
                time.sleep(0.1)
            creds = h.sized(a.am_credential_data_t)
            creds.username_utf8, creds.password_utf8 = stream.user, stream.password
            limits = h.sized(a.am_limits_t)
            for name, url, credential in [("http", f"http://127.0.0.1:{server.server_port}/ok", None),
                                           ("rtsp", stream.url, creds)]:
                statuses = []
                for cap in [None, 64, None]:
                    h.checked(lib.amc_init(c.byref(params), c.byref(ctx)))
                    if cap is not None:
                        h.checked(lib.amc_get_limits(ctx, c.byref(limits)))
                        limits.frame_budget_bytes = cap
                        h.checked(lib.amc_set_limits(ctx, c.byref(limits)))
                    before = t.Fixture.count
                    owned, rows = t.probe(lib, ctx, [url], credential, force=True, timeout=5000)
                    statuses.append(rows[0]["status"])
                    t.release(lib, owned)
                    h.close(lib, ctx, True)
                    ctx = c.POINTER(a.amc_context_t)()
                    if name == "http" and cap == 64:
                        result["http_tiny_network_requests"] = t.Fixture.count - before
                result["cases"][name] = statuses
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
        if ctx:
            h.close(lib, ctx, True)
    (work / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps(result))
    if not args.observe_baseline:
        for statuses in result["cases"].values():
            assert statuses == [a.AM_PROBE_OK, a.AM_PROBE_PENDING, a.AM_PROBE_OK], statuses
        assert result["http_tiny_network_requests"] == 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ["dll-dir", "runtime-dir", "mediamtx", "work-dir"]:
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--observe-baseline", action="store_true")
    main(parser.parse_args())
