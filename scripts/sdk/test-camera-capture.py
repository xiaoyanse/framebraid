"""本机Digest HTTP替身覆盖ISAPI抓图；不连接现场相机或主服务。"""
import argparse
import ctypes as c
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("shared_test", Path(__file__).with_name("test-shared-frames.py"))
shared = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shared)
a, size, check = shared.a, shared.sized, shared.checked


def stats(lib, ctx):
    out = c.POINTER(a.am_stats_t)()
    check(lib.amc_get_stats(ctx, None, c.byref(out)))
    value = (out.contents.queue_items, out.contents.queue_bytes, out.contents.external_held_items)
    check(lib.amc_stats_release(out))
    return value


def fixture_image():
    # 远端可使用预先生成的合成图，不要求安装FFmpeg命令行工具。
    path = os.environ.get("AM_SDK_TEST_JPEG")
    if path:
        return Path(path).read_bytes()
    return subprocess.check_output(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "color=c=red:s=64x48",
                                    "-frames:v", "1", "-f", "image2pipe", "-c:v", "mjpeg", "pipe:1"])


class Fixture(BaseHTTPRequestHandler):
    image = fixture_image()
    delay = 0.0
    requests = 0
    active = 0
    peak = 0
    entered = threading.Event()
    lock = threading.Lock()

    def log_message(self, *_):
        pass

    def do_GET(self):
        if not self.headers.get("Authorization", "").startswith("Digest "):
            self.send_response(401)
            self.send_header("WWW-Authenticate", 'Digest realm="sdk-test", nonce="abcdef", qop="auth", algorithm=MD5')
            self.end_headers()
            return
        with Fixture.lock:
            Fixture.requests += 1
            Fixture.active += 1
            Fixture.peak = max(Fixture.peak, Fixture.active)
        Fixture.entered.set()
        try:
            time.sleep(Fixture.delay)
            self.send_response(200)
            self.send_header("Content-Type", "image/jpeg")
            self.send_header("Content-Length", str(len(self.image)))
            self.end_headers()
            self.wfile.write(self.image)
        except (BrokenPipeError, ConnectionResetError):
            pass
        finally:
            with Fixture.lock:
                Fixture.active -= 1


def open_source(lib, ctx, port):
    credentials = size(a.am_credential_data_t)
    credentials.username_utf8, credentials.password_utf8 = b"test", b"test"
    desc = size(a.am_source_desc_t)
    desc.kind, desc.reuse = a.AM_SRC_ISAPI_URL, 1
    desc.url_utf8 = f"http://127.0.0.1:{port}/ISAPI/Streaming/channels/101/picture".encode()
    desc.stream_kind_utf8, desc.creds = b"isapi", c.pointer(credentials)
    source = c.POINTER(a.amc_source_t)()
    check(lib.amc_source_open(ctx, c.byref(desc), c.byref(source)))
    return source


def close_object(lib, obj, timeout=5000):
    state = c.c_uint32()
    check(lib.amc_object_request_close(obj))
    until = time.monotonic() + timeout / 1000
    while True:
        check(lib.amc_object_query_state(obj, c.byref(state)))
        if state.value == a.AM_LIFECYCLE_CLOSED:
            break
        assert time.monotonic() < until, state.value
        time.sleep(.005)
    check(lib.amc_object_destroy(obj))


def captured(lib, source, output_kind=0):
    req = size(a.am_capture_request_t)
    req.output_kind, req.deadline_ms = output_kind, 1000
    out = c.POINTER(a.am_captured_image_t)()
    check(lib.amc_capture_sync(source, c.byref(req), c.byref(out)))
    return out


def async_request(lib, source):
    req = size(a.am_capture_request_t)
    req.deadline_ms = 1000
    rid = c.c_uint64()
    check(lib.amc_capture_async(source, c.byref(req), c.byref(rid)))
    return rid.value


def wait_terminal(lib, ctx, rid):
    until = time.monotonic() + 5
    while True:
        out = c.POINTER(a.am_request_snapshot_t)()
        check(lib.amc_request_query(ctx, rid, c.byref(out)))
        value = out.contents.state
        check(lib.amc_request_snapshot_release(out))
        if value >= a.AM_REQ_DONE:
            return value
        assert time.monotonic() < until
        time.sleep(.005)


def single(lib, ctx, source):
    assert stats(lib, ctx)[:2] == (0, 0)
    image = captured(lib, source)
    assert c.string_at(image.contents.encoded, image.contents.encoded_size) == Fixture.image
    assert image.contents.meta.contents.clock_mask == a.AM_CLK_RECEIVED
    assert image.contents.meta.contents.pts == -(1 << 63)
    check(lib.amc_image_retain(image))
    check(lib.amc_image_release(image))
    check(lib.amc_image_release(image))
    check(lib.amc_image_release(image), a.AM_ERR_SDK_HANDLE_INVALID)
    cpu = captured(lib, source, a.AM_CAP_CPU_FRAME)
    view = cpu.contents.view.contents
    assert (view.width, view.height, view.format) == (64, 48, a.AM_PF_RGBA8)
    assert view.plane_size_bytes[0] == 64 * 48 * 4
    assert c.string_at(view.planes[0], 4)[0] > 230
    check(lib.amc_image_release(cpu))
    rid = async_request(lib, source)
    assert wait_terminal(lib, ctx, rid) == a.AM_REQ_DONE
    assert stats(lib, ctx)[:2] == (1, len(Fixture.image))
    outputs = []
    for _ in range(2):
        out = c.POINTER(a.am_captured_image_t)()
        check(lib.amc_captured_image_fetch(ctx, rid, c.byref(out)))
        outputs.append(out)
    assert stats(lib, ctx) == (1, len(Fixture.image), 2)
    check(lib.amc_captured_image_ack(ctx, rid))
    assert stats(lib, ctx) == (0, 0, 2)
    check(lib.amc_request_query(ctx, rid, c.byref(c.POINTER(a.am_request_snapshot_t)())), a.AM_ERR_SDK_RESULT_EXPIRED)
    for out in outputs:
        assert c.string_at(out.contents.encoded, out.contents.encoded_size) == Fixture.image
        check(lib.amc_image_release(out))
    assert stats(lib, ctx) == (0, 0, 0)
    Fixture.delay = .15
    rid = async_request(lib, source)
    check(lib.amc_request_cancel(ctx, rid))
    assert wait_terminal(lib, ctx, rid) == a.AM_REQ_CANCELED
    time.sleep(.2)
    assert wait_terminal(lib, ctx, rid) == a.AM_REQ_CANCELED
    check(lib.amc_captured_image_ack(ctx, rid))
    Fixture.delay = 0
    return {"jpegExact": True, "cpuPixels": [64, 48, "RGBA8"], "fetchAckLifetime": True, "cancelTerminalImmutable": True}


def subscriptions(lib, ctx, source):
    desc = size(a.am_capture_continuous_desc_t)
    desc.target_interval_ms, desc.request_timeout_ms = 20, 1000
    desc.max_inflight, desc.queue_max_items, desc.queue_max_bytes = 1, 2, 100000
    desc.mode = a.AM_SUB_PULL
    pull = c.POINTER(a.amc_capture_sub_t)()
    check(lib.amc_capture_continuous_start(source, c.byref(desc), c.byref(pull)))
    check(lib.amc_capture_sub_set_callback(pull, a.am_capture_cb_fn(), None), a.AM_ERR_CAMERA_MODE_CONFLICT)
    time.sleep(.15)
    queued = stats(lib, ctx)
    assert queued[0] >= 2 and queued[1] >= 2 * len(Fixture.image), queued
    out = c.POINTER(a.am_captured_image_t)()
    check(lib.amc_capture_sub_pull(pull, 0, c.byref(out)))
    assert out.contents.request_id > 1, "queue did not drop oldest"
    check(lib.amc_image_release(out))
    capture_stats = size(a.am_capture_stats_t)
    check(lib.amc_capture_sub_get_stats(pull, c.byref(capture_stats)))
    assert capture_stats.actual_fps > 0 and capture_stats.shared_consumers == 1
    desc.mode = a.AM_SUB_PUSH
    push = c.POINTER(a.amc_capture_sub_t)()
    check(lib.amc_capture_continuous_start(source, c.byref(desc), c.byref(push)))
    check(lib.amc_capture_sub_pull(push, 0, c.byref(out)), a.AM_ERR_CAMERA_MODE_CONFLICT)
    held, errors = [], []
    entered, leave = threading.Event(), threading.Event()

    @a.am_capture_cb_fn
    def callback(sub, image, _):
        try:
            check(lib.amc_image_retain(image))
            held.append(image)
            check(lib.amc_capture_sub_set_callback(sub, a.am_capture_cb_fn(), None), a.AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION)
            entered.set()
            assert leave.wait(3)
        except Exception as error:
            errors.append(repr(error))

    check(lib.amc_capture_sub_set_callback(push, callback, None))
    assert entered.wait(3)
    completed = threading.Event()
    codes = []

    def unregister():
        codes.append(lib.amc_capture_sub_set_callback(push, a.am_capture_cb_fn(), None))
        completed.set()

    thread = threading.Thread(target=unregister)
    thread.start()
    assert not completed.wait(.04)
    check(lib.amc_capture_sub_pull(pull, 0, c.byref(out)))
    check(lib.amc_image_release(out))
    leave.set()
    thread.join(3)
    assert completed.is_set() and codes == [0] and not errors, (codes, errors)
    for image in held:
        assert image.contents.encoded_size == len(Fixture.image)
        check(lib.amc_image_release(image))
    close_object(lib, push)
    close_object(lib, pull)
    assert stats(lib, ctx)[:2] == (0, 0)
    return {"dropOldest": True, "pushPullExclusive": True, "callbackBarrier": True, "slowCallbackDoesNotBlockOtherSub": True}


def collectors(lib, source):
    desc = size(a.am_capture_continuous_desc_t)
    desc.target_interval_ms, desc.request_timeout_ms = 100, 1000
    desc.max_inflight, desc.mode = 1, a.AM_SUB_PULL
    first, second = c.POINTER(a.amc_capture_sub_t)(), c.POINTER(a.amc_capture_sub_t)()
    before = Fixture.requests
    check(lib.amc_capture_continuous_start(source, c.byref(desc), c.byref(first)))
    time.sleep(.04)
    check(lib.amc_capture_continuous_start(source, c.byref(desc), c.byref(second)))
    time.sleep(.53)
    images = []
    for sub in (first, second):
        seen = set()
        while True:
            out = c.POINTER(a.am_captured_image_t)()
            code = lib.amc_capture_sub_pull(sub, 0, c.byref(out))
            if code == a.AM_ERR_SDK_TRY_AGAIN:
                break
            check(code)
            seen.add((out.contents.meta.contents.frame_id, out.contents.meta.contents.received_at_ms))
            check(lib.amc_image_release(out))
        images.append(seen)
    requests = Fixture.requests - before
    close_object(lib, second)
    close_object(lib, first)
    assert requests <= 8, (requests, "compatible consumers duplicated physical captures")
    assert images[0] & images[1], images
    return {"offsetConsumersSharedRequests": requests, "frameIdentityAndReceiptPreserved": True}


def run(args):
    lib = c.CDLL(str((args.dll_dir / "am_camera.dll").resolve()))
    a.bind(lib, "amc_")
    params = size(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    ctx = c.POINTER(a.amc_context_t)()
    check(lib.amc_init(c.byref(params), c.byref(ctx)))
    server = ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        source = open_source(lib, ctx, server.server_address[1])
        result = {"single": single(lib, ctx, source), "subscriptions": subscriptions(lib, ctx, source), "collectors": collectors(lib, source)}
        image = captured(lib, source)
        close_object(lib, source)
        lifecycle = c.c_uint32()
        check(lib.amc_shutdown(ctx, 0, c.byref(lifecycle)))
        assert lifecycle.value == a.AM_LIFECYCLE_CLOSING
        check(lib.amc_image_release(image))
        shared.close(lib, ctx, True)
        result.update({"outputKeepsContextClosing": True, "digestRequests": Fixture.requests, "peakHttp": Fixture.peak})
        print(json.dumps(result))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(2)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    run(parser.parse_args())
