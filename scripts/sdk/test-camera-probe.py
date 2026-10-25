"""真实 DLL 探测、Digest、时效缓存、并发上限与所有权；只连接本脚本回环服务。"""
import argparse
import ctypes as c
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
from pathlib import Path
import re
import struct
import sys
import threading
import time
import zlib

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("shared", Path(__file__).with_name("test-shared-frames.py"))
h = importlib.util.module_from_spec(spec)
spec.loader.exec_module(h)
a = h.a


def png(width, height):
    def chunk(name, data):
        return struct.pack("!I", len(data)) + name + data + struct.pack("!I", zlib.crc32(name + data))
    pixels = b"".join(b"\0" + bytes([16, 32, 64]) * width for _ in range(height))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack("!IIBBBBB", width, height, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b"")


class Fixture(BaseHTTPRequestHandler):
    lock = threading.Lock()
    count = 0
    active = 0
    peak = 0
    width = 3
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def send(self, code, body=b"", headers=None):
        self.send_response(code)
        for name, value in (headers or {}).items():
            self.send_header(name, value)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        with self.lock:
            Fixture.count += 1
            Fixture.active += 1
            Fixture.peak = max(Fixture.peak, Fixture.active)
        try:
            if self.path.startswith("/slow"):
                time.sleep(.35)
            elif self.path.startswith("/parallel"):
                time.sleep(.12)
            if self.path.startswith("/digest"):
                auth = self.headers.get("Authorization", "")
                if not auth:
                    self.send(401, headers={"WWW-Authenticate": 'Digest realm="sdk-test", nonce="fixture", qop="auth", algorithm=MD5'})
                    return
                fields = {
                    match[0]: match[1] or match[2] for match in re.findall(r'(\w+)=(?:"([^"]*)"|([^,\s]+))', auth)
                }
                md5 = lambda text: hashlib.md5(text.encode()).hexdigest()
                ha1 = md5("fixture-user:sdk-test:fixture-password")
                response = md5(f'{ha1}:fixture:{fields.get("nc")}:{fields.get("cnonce")}:auth:{md5("GET:" + self.path)}')
                if fields.get("username") != "fixture-user" or fields.get("response") != response:
                    self.send(403)
                    return
            if self.path.startswith("/failed"):
                self.send(503)
            elif self.path.startswith("/unknown"):
                self.send(200, b"not an image")
            else:
                self.send(200, png(Fixture.width, 2), {"Content-Type": "image/png"})
        except (BrokenPipeError, ConnectionResetError):
            pass
        finally:
            with self.lock:
                Fixture.active -= 1


class FixtureServer(ThreadingHTTPServer):
    def handle_error(self, request, client_address):
        error = sys.exc_info()[1]
        # 超时探测主动关连接，Windows 服务端写回可能得到已中止错误。
        if isinstance(error, (BrokenPipeError, ConnectionResetError, ConnectionAbortedError)):
            return
        if isinstance(error, OSError) and getattr(error, "winerror", None) in (10053, 10054):
            return
        super().handle_error(request, client_address)


def credentials(password=b"fixture-password"):
    value = h.sized(a.am_credential_data_t)
    value.username_utf8, value.password_utf8 = b"fixture-user", password
    return value


def probe(camera, ctx, urls, creds=None, force=False, timeout=1000, expected=0):
    references = (c.c_char_p * len(urls))(*(url.encode() for url in urls))
    req = h.sized(a.am_probe_request_t)
    req.candidate_count, req.references_utf8 = len(urls), references
    req.force_refresh, req.timeout_ms = int(force), timeout
    if creds is not None:
        req.creds = c.pointer(creds)
    result = c.POINTER(a.am_probe_result_t)()
    h.checked(camera.amc_probe_streams(ctx, c.byref(req), c.byref(result)), expected)
    if expected:
        assert not result
        return None, []
    assert result.contents.candidate_count == len(urls)
    rows = []
    for index, row in enumerate(result.contents.candidates[:len(urls)]):
        assert row.reference_utf8.decode() == urls[index]
        assert row.struct_size == c.sizeof(a.am_probe_candidate_t)
        rows.append({"status": row.status, "codec": row.codec_utf8.decode() if row.codec_utf8 else None,
                     "width": row.width, "height": row.height, "fps": row.fps,
                     "observed": row.observed_at_ms, "until": row.valid_until_ms})
    return result, rows


def release(camera, result):
    h.checked(camera.amc_probe_result_release(result))
    h.checked(camera.amc_probe_result_release(result), a.AM_ERR_SDK_HANDLE_INVALID)


def run(args):
    camera = c.CDLL(str(args.dll_dir.resolve() / "am_camera.dll"))
    a.bind(camera, "amc_")
    p = h.sized(a.amc_init_params_t)
    p.abi_version = a.AM_ABI_VERSION
    if args.runtime_dir:
        dirs = (c.c_char_p * 1)(str(args.runtime_dir.resolve()).encode())
        p.runtime_dirs_utf8, p.runtime_dir_count = dirs, 1
    ctx = c.POINTER(a.amc_context_t)()
    h.checked(camera.amc_init(c.byref(p), c.byref(ctx)))
    server = FixtureServer(("127.0.0.1", 0), Fixture)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    origin = f"http://127.0.0.1:{server.server_port}"
    evidence = {}
    try:
        urls = [origin + suffix for suffix in ["/digest", "/slow", "/failed", "/unknown"]] + ["ftp://127.0.0.1/unsupported"]
        result, rows = probe(camera, ctx, urls, credentials(), timeout=120)
        assert [row["status"] for row in rows] == [a.AM_PROBE_OK, a.AM_PROBE_TIMEOUT, a.AM_PROBE_UNREACHABLE, a.AM_PROBE_UNSUPPORTED, a.AM_PROBE_UNSUPPORTED], rows
        assert (rows[0]["codec"], rows[0]["width"], rows[0]["height"], rows[0]["fps"]) == ("png", 3, 2, -1)
        assert rows[0]["until"] - rows[0]["observed"] == 300000
        assert all(row["until"] - row["observed"] == 30000 for row in rows[1:])
        evidence["independentCandidates"] = rows
        release(camera, result)
        previous = Fixture.count
        Fixture.width = 7
        result, cached = probe(camera, ctx, [urls[0]], credentials())
        assert Fixture.count == previous and cached[0] == rows[0]
        release(camera, result)
        result, bad = probe(camera, ctx, [urls[0]], credentials(b"wrong"))
        assert bad[0]["status"] == a.AM_PROBE_AUTH_FAILED
        release(camera, result)
        result, forced = probe(camera, ctx, [urls[0]], credentials(), force=True)
        assert forced[0]["status"] == a.AM_PROBE_OK and forced[0]["width"] == 7
        release(camera, result)
        evidence["cacheCredentialIsolationForce"] = True
        # 超时请求的服务端线程先退出，避免把其残余活跃量计作 SDK 并发。
        time.sleep(.4)
        Fixture.peak = 0
        start = time.monotonic()
        result, parallel = probe(camera, ctx, [origin + f"/parallel/{n}" for n in range(8)])
        elapsed = time.monotonic() - start
        assert all(row["status"] == a.AM_PROBE_OK for row in parallel)
        assert 2 <= Fixture.peak <= 4, Fixture.peak
        release(camera, result)
        evidence["parallel"] = {"candidateCount": 8, "peak": Fixture.peak, "elapsedSeconds": elapsed}
        probe(camera, ctx, [origin] * 17, expected=a.AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
        probe(camera, ctx, [origin.replace("http://", "http://user:password@")], expected=a.AM_ERR_CAMERA_URL_INVALID)
        h.checked(camera.amc_probe_result_release(c.cast(ctx, c.POINTER(a.am_probe_result_t))), a.AM_ERR_SDK_HANDLE_INVALID)
        result, _ = probe(camera, ctx, [origin + "/ok"])
        state = c.c_uint32()
        h.checked(camera.amc_shutdown(ctx, 0, c.byref(state)))
        assert state.value == a.AM_LIFECYCLE_CLOSING
        probe(camera, ctx, [origin + "/ok"], expected=a.AM_ERR_SDK_HANDLE_CLOSED)
        release(camera, result)
        h.checked(camera.amc_shutdown(ctx, 100, c.byref(state)))
        assert state.value == a.AM_LIFECYCLE_CLOSED
        h.checked(camera.amc_object_destroy(ctx))
        evidence["closingOwnership"] = True
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    print(json.dumps(evidence, ensure_ascii=False))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path)
    run(parser.parse_args())
