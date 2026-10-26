"""回环恢复测试的私有源/订阅和Windows线程观测；不访问现场设备。"""
import collections
import ctypes as c
import hashlib
import importlib.util
import json
import socket
import subprocess
from pathlib import Path
import sys
import threading
import time

sys.dont_write_bytecode = True


def module(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), Path(__file__).with_name(name))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


t = module("test-camera-transport.py")
lifecycle = module("test-module-lifecycle.py")
a, sized, checked, close = t.a, t.sized, t.checked, t.close


def until(predicate, timeout=10):
    deadline = time.monotonic() + timeout
    while True:
        result = predicate()
        if result:
            return result
        assert time.monotonic() < deadline, "owned recovery scenario deadline"
        time.sleep(0.01)


def media_workers(k):
    snap = k.CreateToolhelp32Snapshot(4, 0)
    assert snap != c.c_void_p(-1).value
    entry = lifecycle.ThreadEntry()
    entry.size = c.sizeof(entry)
    names = []
    try:
        more = k.Thread32First(snap, c.byref(entry))
        while more:
            if entry.pid == __import__("os").getpid():
                handle = k.OpenThread(0x0800, 0, entry.tid)
                if not handle and c.get_last_error() == 87:
                    more = k.Thread32Next(snap, c.byref(entry))
                    continue
                assert handle, (entry.tid, c.get_last_error())
                try:
                    label = c.c_void_p()
                    assert k.GetThreadDescription(handle, c.byref(label)) >= 0
                    try:
                        name = c.wstring_at(label) if label else ""
                        if name.startswith("amc-"):
                            names.append(name)
                    finally:
                        k.LocalFree(label)
                finally:
                    k.CloseHandle(handle)
            more = k.Thread32Next(snap, c.byref(entry))
    finally:
        k.CloseHandle(snap)
    return sorted(names)


def resources(k):
    value = lifecycle.resources(k)
    value.pop("sdkWorkers")
    value["mediaWorkers"] = media_workers(k)
    return value


def sdk_clock_ms():
    k = c.WinDLL("kernel32")
    for name in ["QueryPerformanceCounter", "QueryPerformanceFrequency"]:
        fn = getattr(k, name)
        fn.argtypes, fn.restype = [c.POINTER(c.c_int64)], c.c_int
    counter, frequency = c.c_int64(), c.c_int64()
    assert k.QueryPerformanceCounter(c.byref(counter)) and k.QueryPerformanceFrequency(c.byref(frequency))
    return counter.value * 1000 // frequency.value


class Stream:
    def __init__(self, work, executable):
        self.work, self.executable = work, executable
        self.port = t.free_port()
        self.url = f"rtsp://127.0.0.1:{self.port}/recovery"
        self.user, self.password = b"synthetic-recovery", __import__("secrets").token_hex(12).encode()
        self.server = self.publisher = None
        self.logs = []
        self.config = work / "mediamtx.yml"
        self.children = []

    def __enter__(self):
        self.config.write_text(f"""logLevel: warn
rtspAddress: 127.0.0.1:{self.port}
rtspTransports: [tcp]
rtmp: no
hls: no
webrtc: no
srt: no
moq: no
authMethod: internal
authInternalUsers:
  - user: any
    ips: [127.0.0.1]
    permissions:
      - action: publish
  - user: {self.user.decode()}
    pass: {self.password.decode()}
    ips: [127.0.0.1]
    permissions:
      - action: read
paths:
  all_others:
""", encoding="utf-8")
        try:
            self.server = self.spawn([str(self.executable), str(self.config)], "server.log")
            def ready():
                assert self.server.poll() is None, "owned MediaMTX failed; inspect private log"
                try:
                    with socket.create_connection(("127.0.0.1", self.port), timeout=0.1):
                        return True
                except OSError:
                    return False
            until(ready)
            self.publish()
            return self
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def spawn(self, command, log):
        handle = (self.work / log).open("ab")
        self.logs.append(handle)
        p = subprocess.Popen(command, cwd=self.work, stdout=handle, stderr=subprocess.STDOUT)
        self.children.append(p)
        return p

    def publish(self):
        assert self.publisher is None or self.publisher.poll() is not None
        self.publisher = self.spawn(["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-re",
            "-f", "lavfi", "-i", "testsrc2=size=256x144:rate=10", "-an", "-c:v", "libx264",
            "-threads", "1", "-preset", "ultrafast", "-tune", "zerolatency", "-g", "5",
            "-f", "rtsp", "-rtsp_transport", "tcp", self.url], "publisher.log")
        time.sleep(0.2)
        assert self.publisher.poll() is None, "owned publisher failed; inspect private log"

    def disconnect(self):
        t.stop(self.publisher)

    def __exit__(self, *unused):
        for p in reversed(self.children):
            t.stop(p)
        assert all(p.poll() is not None for p in self.children)
        for handle in self.logs:
            handle.close()
        (self.work / "owned-processes.json").write_text(json.dumps(
            [{"pid": p.pid, "exit_code": p.poll()} for p in self.children], indent=2), encoding="utf-8")
        # 只回收本轮新建的合成认证配置，不保留不必要的测试密钥。
        self.config.unlink(missing_ok=True)


class Session:
    def __init__(self, lib, ctx, stream, push=False, block=False):
        self.lib, self.ctx, self.stream, self.push = lib, ctx, stream, push
        self.source = c.POINTER(a.amc_source_t)()
        self.encoded = c.POINTER(a.amc_encoded_sub_t)()
        self.frames = c.POINTER(a.amc_frame_sub_t)()
        self.errors, self.events = [], collections.deque(maxlen=128)
        self.counts = [0, 0, 0]
        self.block, self.gate, self.entered = block, threading.Event(), threading.Event()
        self.creds = sized(a.am_credential_data_t)
        self.creds.username_utf8, self.creds.password_utf8 = stream.user, stream.password
        self.callbacks = []
        try:
            desc = sized(a.am_source_desc_t)
            desc.kind, desc.url_utf8, desc.stream_kind_utf8 = a.AM_SRC_RTSP_URL, stream.url.encode(), b"main"
            desc.creds = c.pointer(self.creds)
            checked(lib.amc_source_open(ctx, c.byref(desc), c.byref(self.source)))
            checked(lib.amc_source_set_decode_policy(self.source, a.AM_DHW_SOFTWARE_ONLY))
            policy = sized(a.am_reconnect_policy_t)
            policy.initial_backoff_ms, policy.max_backoff_ms = 25, 100
            checked(lib.amc_source_set_reconnect_policy(self.source, c.byref(policy)))
            @a.am_source_event_fn
            def event(src, kind, detail, user):
                try:
                    assert detail
                    self.counts[2] += 1
                    self.events.append((time.perf_counter(), kind, detail.contents.generation, detail.contents.fault_class))
                except Exception as error:
                    self.errors.append(type(error).__name__ + ": " + str(error))
            self.callbacks.append(event)
            checked(lib.amc_source_set_event_callback(self.source, event, None))
            desc = sized(a.am_encoded_sub_desc_t)
            desc.codec_mask, desc.mode = a.AM_CODEC_H264, a.AM_SUB_PUSH if push else a.AM_SUB_PULL
            desc.queue_max_items, desc.queue_max_bytes = 8, 1024 * 1024
            t.subscribe_when_ready(lib.amc_subscribe_encoded, self.source, desc, self.encoded, lib)
            frames = sized(a.am_frame_sub_desc_t)
            frames.pixel_format, frames.sampling_max_fps = a.AM_PF_RGBA8, 10
            frames.mode, frames.queue_max_items, frames.queue_max_bytes = desc.mode, 2, 1024 * 1024
            t.subscribe_when_ready(lib.amc_subscribe_frames, self.source, frames, self.frames, lib)
            if push:
                self.set_push()
        except BaseException:
            self.close()
            raise

    def status(self):
        value = sized(a.am_source_status_t)
        checked(self.lib.amc_source_get_status(self.source, c.byref(value)))
        return value

    def set_push(self):
        @a.am_encoded_cb_fn
        def encoded(sub, packet, user):
            try:
                assert packet and packet.contents.size > 0
                self.counts[0] += 1
            except Exception as error:
                self.errors.append(type(error).__name__ + ": " + str(error))
        @a.am_frame_cb_fn
        def frame(sub, value, user):
            try:
                self.frame(value)
                self.counts[1] += 1
                if self.block and not self.entered.is_set():
                    state = c.c_uint32()
                    code = self.lib.amc_source_close(self.source, 0, c.byref(state))
                    assert code == a.AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION, code
                    self.entered.set()
                    assert self.gate.wait(10), "owned callback gate deadline"
            except Exception as error:
                self.errors.append(type(error).__name__ + ": " + str(error))
        self.callbacks.extend([encoded, frame])
        checked(self.lib.amc_encoded_set_callback(self.encoded, encoded, None))
        checked(self.lib.amc_frame_set_callback(self.frames, frame, None))

    def frame(self, pointer):
        meta, view = sized(a.am_frame_meta_t), sized(a.am_frame_view_t)
        checked(self.lib.amc_frame_get_meta(pointer, c.byref(meta)))
        checked(self.lib.amc_frame_get_view(pointer, c.byref(view)))
        assert (view.width, view.height, view.format) == (256, 144, a.AM_PF_RGBA8)
        raw = c.string_at(view.planes[0], view.plane_size_bytes[0])
        assert len(set(raw)) > 8, "expected real decoded test pattern"
        return {"generation": meta.source_generation, "source_id": meta.source_id,
                "frame_id": meta.frame_id, "received_ms": meta.received_at_ms,
                "pixel_sha256": hashlib.sha256(raw).hexdigest()}

    def pull_frame(self, timeout=100):
        value = c.POINTER(a.amc_frame_t)()
        code = self.lib.amc_frame_pull(self.frames, timeout, c.byref(value))
        if code in (a.AM_ERR_SDK_TRY_AGAIN, a.AM_ERR_SDK_DEADLINE_EXCEEDED):
            assert not value
            return None
        checked(code)
        try:
            return self.frame(value)
        finally:
            checked(self.lib.amc_frame_release(value))

    def pull_packet(self):
        value = c.POINTER(a.am_encoded_packet_t)()
        code = self.lib.amc_encoded_pull(self.encoded, 100, c.byref(value))
        if code in (a.AM_ERR_SDK_TRY_AGAIN, a.AM_ERR_SDK_DEADLINE_EXCEEDED):
            assert not value
            return None
        checked(code)
        try:
            row = value.contents
            payload = c.string_at(row.data, row.size)
            return {"generation": row.source_generation, "flags": row.flags,
                    "nals": t.nal_types(payload, a.AM_CODEC_H264)}
        finally:
            checked(self.lib.amc_encoded_packet_release(value))

    def recovered(self, generation, started, restored_ms):
        first = until(lambda: self.pull_frame())
        assert first["generation"] > generation, (first["generation"], generation)
        assert first["received_ms"] >= restored_ms
        first_ms = (time.perf_counter() - started) * 1000
        deadline = time.perf_counter() + 2
        packet = until(self.pull_packet)
        assert packet["generation"] == first["generation"] and packet["flags"] & a.AM_PKT_KEYFRAME
        assert {5, 7, 8}.issubset(packet["nals"]), packet
        rows = [first]
        while len(rows) < 5:
            assert time.perf_counter() < deadline, "recovered delivery window deadline"
            value = self.pull_frame()
            if value:
                rows.append(value)
        assert len({r["generation"] for r in rows}) == 1
        assert len({r["source_id"] for r in rows}) == 1
        assert all(r["received_ms"] >= restored_ms for r in rows)
        assert [r["frame_id"] for r in rows] == sorted({r["frame_id"] for r in rows})
        assert first_ms <= 10000
        return {"first_ms": first_ms, "five_frame_ms": (time.perf_counter() - started) * 1000,
                "generation": first["generation"], "source_id": first["source_id"], "frames": len(rows),
                "first_packet_keyframe_with_parameter_sets": True, "received_after_restore": True}

    def close(self):
        started = time.perf_counter()
        if self.frames:
            if self.block and self.entered.is_set():
                state = c.c_uint32()
                checked(self.lib.amc_frame_unsubscribe(self.frames, 0, c.byref(state)))
                assert state.value == a.AM_LIFECYCLE_CLOSING
            self.gate.set()
            close(self.lib, self.frames, self.lib.amc_frame_unsubscribe)
            self.frames = c.POINTER(a.amc_frame_sub_t)()
        if self.encoded:
            close(self.lib, self.encoded, self.lib.amc_encoded_unsubscribe)
            self.encoded = c.POINTER(a.amc_encoded_sub_t)()
        if self.source:
            checked(self.lib.amc_source_set_event_callback(self.source, a.am_source_event_fn(), None))
            close(self.lib, self.source, self.lib.amc_source_close)
            self.source = c.POINTER(a.amc_source_t)()
        assert not self.errors, self.errors
        return (time.perf_counter() - started) * 1000


def stats(lib, ctx):
    value = c.POINTER(a.am_stats_t)()
    checked(lib.amc_get_stats(ctx, None, c.byref(value)))
    try:
        v = value.contents
        return {n: getattr(v, n) for n in ["queue_items", "queue_bytes", "external_held_items", "external_held_bytes", "reconnect_count", "dropped", "resync_count"]}
    finally:
        checked(lib.amc_stats_release(value))
