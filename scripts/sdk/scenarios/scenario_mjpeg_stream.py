"""R7 场景：MJPEG 流订阅帧送达 + 断流重连（本地 multipart 服务器）。

TcpListener 最小 multipart 替身（模式同 camera-ffi mjpeg/tests.rs）：
用例一订阅后收帧、尺寸/格式/状态 LIVE；用例二连接送两帧后硬断、
重连后续供，拉满 3 帧且源代次递增。
判读：退出码 0=过。
用法：python scenario_mjpeg_stream.py --dll-dir <release> --runtime-dir <ffmpeg bin>
      [--output result.json]
"""
import argparse
import ctypes as c
import json
import platform
import sys
import threading
import time
from pathlib import Path
from socket import socket

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "sdk" / "bindings" / "python"))
import amrt_sdk as a  # noqa: E402
import cv2  # noqa: E402
import numpy as np  # noqa: E402

WIDTH, HEIGHT = 64, 48
PULL_TIMEOUT_MS = 5000
BARRIER_TIMEOUT = 15


def fixture_jpeg():
    rng = np.random.default_rng(20261010)
    pixels = rng.integers(0, 256, (HEIGHT, WIDTH, 3), dtype=np.uint8)
    ok, encoded = cv2.imencode(".jpg", pixels, [cv2.IMWRITE_JPEG_QUALITY, 85])
    assert ok
    return encoded.tobytes()


def checked(lib, code, stage=""):
    if code != 0:
        name = lib.amc_status_name(code)
        message = name.decode() if name else ""
        raise AssertionError(f"{stage} code={code} {message}")


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def bind_listener():
    listener = socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(4)
    return listener


def read_request(conn):
    conn.settimeout(BARRIER_TIMEOUT)
    data = b""
    while not data.endswith(b"\r\n\r\n"):
        chunk = conn.recv(1)
        assert chunk, "客户端提前断开"
        data += chunk
        assert len(data) < 16 * 1024


def multipart_response(jpeg, count):
    response = (b"HTTP/1.1 200 OK\r\n"
                b"Content-Type: multipart/x-mixed-replace; boundary=frame\r\n"
                b"Connection: close\r\n\r\n")
    for _ in range(count):
        response += (b"--frame\r\nContent-Type: image/jpeg\r\nContent-Length: "
                     + str(len(jpeg)).encode() + b"\r\n\r\n" + jpeg + b"\r\n")
    return response


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


def open_mjpeg_source(lib, ctx, port):
    creds = sized(a.am_credential_data_t)
    creds.username_utf8, creds.password_utf8 = b"sdk-user", b"sdk-local-test-only"
    desc = sized(a.am_source_desc_t)
    desc.kind = a.AM_SRC_MJPEG_URL
    desc.url_utf8 = f"http://127.0.0.1:{port}/video".encode()
    desc.creds = c.pointer(creds)
    source = c.POINTER(a.amc_source_t)()
    checked(lib, lib.amc_source_open(ctx, c.byref(desc), c.byref(source)), "source_open")
    return source


def subscribe_frames(lib, source):
    desc = sized(a.am_frame_sub_desc_t)
    desc.pixel_format = a.AM_PF_RGBA8
    desc.mode = a.AM_SUB_PULL
    sub = c.POINTER(a.amc_frame_sub_t)()
    checked(lib, lib.amc_subscribe_frames(source, c.byref(desc), c.byref(sub)), "subscribe_frames")
    return sub


def pull_frame(lib, sub):
    frame = c.POINTER(a.amc_frame_t)()
    checked(lib, lib.amc_frame_pull(sub, PULL_TIMEOUT_MS, c.byref(frame)), "frame_pull")
    try:
        view = sized(a.am_frame_view_t)
        checked(lib, lib.amc_frame_get_view(frame, c.byref(view)), "frame_get_view")
        assert (view.width, view.height, view.format) == (WIDTH, HEIGHT, a.AM_PF_RGBA8), \
            (view.width, view.height, view.format)
        assert view.plane_size_bytes[0] == WIDTH * HEIGHT * 4, view.plane_size_bytes[0]
    finally:
        checked(lib, lib.amc_frame_release(frame), "frame_release")


def wait_closed(lib, obj, closer, label):
    state = c.c_uint32()
    checked(lib, closer(obj, PULL_TIMEOUT_MS, c.byref(state)), label)
    assert state.value == a.AM_LIFECYCLE_CLOSED, state.value
    checked(lib, lib.amc_object_destroy(obj), f"{label}/destroy")


def serve_delivery(listener, jpeg, ready, done):
    conn, _ = listener.accept()
    read_request(conn)
    ready.wait(BARRIER_TIMEOUT)
    conn.sendall(multipart_response(jpeg, 3))
    done.wait(BARRIER_TIMEOUT)
    conn.close()


def serve_reconnect(listener, jpeg, ready, done):
    first, _ = listener.accept()
    read_request(first)
    ready.wait(BARRIER_TIMEOUT)
    first.sendall(multipart_response(jpeg, 2))
    first.close()
    second, _ = listener.accept()
    read_request(second)
    second.sendall(multipart_response(jpeg, 3))
    done.wait(BARRIER_TIMEOUT)
    second.close()


def assert_live(lib, source, min_generation=0):
    status = sized(a.am_source_status_t)
    checked(lib, lib.amc_source_get_status(source, c.byref(status)), "source_status")
    assert status.state == a.AM_SRC_LIVE, f"源状态 {status.state} != LIVE"
    assert status.generation >= min_generation, f"代次 {status.generation} < {min_generation}"
    return status.generation


def case_delivery(lib, ctx, jpeg):
    listener = bind_listener()
    ready, done = threading.Barrier(2), threading.Barrier(2)
    thread = threading.Thread(target=serve_delivery, args=(listener, jpeg, ready, done), daemon=True)
    thread.start()
    source = open_mjpeg_source(lib, ctx, listener.getsockname()[1])
    sub = subscribe_frames(lib, source)
    ready.wait(BARRIER_TIMEOUT)
    for _ in range(2):
        pull_frame(lib, sub)
    generation = assert_live(lib, source)
    wait_closed(lib, sub, lib.amc_frame_unsubscribe, "frame_unsubscribe")
    done.wait(BARRIER_TIMEOUT)
    wait_closed(lib, source, lib.amc_source_close, "source_close")
    thread.join(5)
    listener.close()
    return {"frames": 2, "generation": generation, "state": "LIVE"}


def case_reconnect(lib, ctx, jpeg):
    listener = bind_listener()
    ready, done = threading.Barrier(2), threading.Barrier(2)
    thread = threading.Thread(target=serve_reconnect, args=(listener, jpeg, ready, done), daemon=True)
    thread.start()
    source = open_mjpeg_source(lib, ctx, listener.getsockname()[1])
    sub = subscribe_frames(lib, source)
    ready.wait(BARRIER_TIMEOUT)
    for _ in range(3):
        pull_frame(lib, sub)
    generation = assert_live(lib, source, min_generation=1)
    wait_closed(lib, sub, lib.amc_frame_unsubscribe, "frame_unsubscribe")
    done.wait(BARRIER_TIMEOUT)
    wait_closed(lib, source, lib.amc_source_close, "source_close")
    thread.join(5)
    listener.close()
    return {"frames": 3, "generation": generation, "reconnected": True}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    stage = "init"
    try:
        jpeg = fixture_jpeg()
        lib, ctx = init_context(args.dll_dir, args.runtime_dir)
        try:
            stage = "delivery"
            delivery = case_delivery(lib, ctx, jpeg)
            print(f"[mjpeg] 送达 {delivery}", flush=True)
            stage = "reconnect"
            reconnect = case_reconnect(lib, ctx, jpeg)
            print(f"[mjpeg] 断流重连 {reconnect}", flush=True)
            result = {"ok": True, "schema": "framebraid.scenario-mjpeg/1", "os": platform.platform(),
                      "delivery": delivery, "reconnect": reconnect}
            if args.output:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps(result, ensure_ascii=False, indent=1), encoding="utf-8")
            print(json.dumps(result, ensure_ascii=False))
        finally:
            checked(lib, lib.amc_shutdown(ctx, 100, c.byref(c.c_uint32())), "shutdown")
            lib.amc_object_destroy(ctx)
    except Exception as error:
        print(json.dumps({"ok": False, "stage": stage, "error": repr(error)}, ensure_ascii=False))
        sys.exit(1)


if __name__ == "__main__":
    main()
