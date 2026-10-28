"""R2b 节拍验证：单订阅窗口内物理 GET 计数 ≈ 窗口/目标间隔。

复用 test-camera-capture.py 的 HTTP 替身（内建请求计数），断言连续订阅的
物理抓取频率贴近目标节拍（串行重置语义曾令 GET 频率恒 ~1/2 目标）。
用法：python test-capture-beat.py --dll-dir <含 am_camera.dll 的目录>
"""
import argparse
import importlib.util
import sys
import threading
import time
from http.server import ThreadingHTTPServer
from pathlib import Path

sys.dont_write_bytecode = True
HERE = Path(__file__).parent
spec = importlib.util.spec_from_file_location("fixture", HERE / "test-camera-capture.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
a, check, size, c = fixture.a, fixture.check, fixture.size, fixture.c


def main(dll_dir: Path):
    lib = c.CDLL(str((dll_dir / "am_camera.dll").resolve()))
    a.bind(lib, "amc_")
    params = size(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    ctx = c.POINTER(a.amc_context_t)()
    check(lib.amc_init(c.byref(params), c.byref(ctx)))
    server = ThreadingHTTPServer(("127.0.0.1", 0), fixture.Fixture)
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        source = fixture.open_source(lib, ctx, server.server_address[1])
        desc = size(a.am_capture_continuous_desc_t)
        target_ms = 100
        desc.target_interval_ms, desc.request_timeout_ms = target_ms, 1000
        desc.max_inflight, desc.queue_max_items, desc.queue_max_bytes = 1, 4, 32 << 20
        desc.mode = a.AM_SUB_PULL
        sub = c.POINTER(a.amc_capture_sub_t)()
        check(lib.amc_capture_continuous_start(source, c.byref(desc), c.byref(sub)))
        window_s = 3.0
        before = fixture.Fixture.requests
        deadline = time.monotonic() + window_s
        frames = 0
        while time.monotonic() < deadline:
            out = c.POINTER(a.am_captured_image_t)()
            code = lib.amc_capture_sub_pull(sub, 20, c.byref(out))
            if code == a.AM_ERR_SDK_TRY_AGAIN:
                continue
            check(code)
            frames += 1
            check(lib.amc_image_release(out))
        gets = fixture.Fixture.requests - before
        expected = window_s * 1000.0 / target_ms
        ratio = gets / expected
        print(f"window={window_s}s target={target_ms}ms gets={gets} frames={frames} "
              f"expected~{expected:.0f} ratio={ratio:.2f}")
        fixture.close_object(lib, sub)
        fixture.close_object(lib, source)
        assert gets >= expected * 0.8, f"GET 频率不足目标 80%: ratio={ratio:.2f}"
        assert frames >= expected * 0.7, f"送达帧数不足: frames={frames}"
    finally:
        server.shutdown()
        server.server_close()
        fixture.shared.close(lib, ctx, True)
    print("beat-parity-ok")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    main(parser.parse_args().dll_dir)
