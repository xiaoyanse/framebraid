"""H2.3 定向矩阵：RTSP 断流恢复与订阅在途关闭竞争；回环合成流，无现场设备。"""
import argparse
import ctypes as c
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import time

from local_paths import local_path
from perf_common import file_sha256

spec = importlib.util.spec_from_file_location("rtsp_sampling", Path(__file__).with_name("benchmark-rtsp-sampling.py"))
h = importlib.util.module_from_spec(spec)
spec.loader.exec_module(h)
a = h.a


def open_source(lib, ctx, url, software):
    credentials = h.sized(a.am_credential_data_t)
    credentials.username_utf8, credentials.password_utf8 = b"synthetic", b"local-fixture"
    desc = h.sized(a.am_source_desc_t)
    desc.kind, desc.url_utf8, desc.creds = a.AM_SRC_RTSP_URL, url.encode(), c.pointer(credentials)
    source = c.POINTER(a.amc_source_t)()
    h.checked(lib.amc_source_open(ctx, c.byref(desc), c.byref(source)))
    h.checked(lib.amc_source_set_decode_policy(source, a.AM_DHW_SOFTWARE_ONLY if software else a.AM_DHW_PREFER_HW))
    es = h.sized(a.am_encoded_sub_desc_t)
    es.codec_mask, es.mode, es.queue_max_items, es.queue_max_bytes = a.AM_CODEC_H264, a.AM_SUB_PULL, 36, 8 * 1024 * 1024
    encoded = c.POINTER(a.amc_encoded_sub_t)()
    h.subscribe(lib, lib.amc_subscribe_encoded, source, es, encoded)
    fs = h.sized(a.am_frame_sub_desc_t)
    fs.pixel_format, fs.mode = a.AM_PF_RGBA8, a.AM_SUB_PULL
    pixels = c.POINTER(a.amc_frame_sub_t)()
    h.subscribe(lib, lib.amc_subscribe_frames, source, fs, pixels)
    return source, encoded, pixels


def drain_frame(lib, pixels, timeout_s):
    """拉一帧；返回 (meta 行, 0) 或超时 (None, DEADLINE)。"""
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        frame = c.POINTER(a.amc_frame_t)()
        code = lib.amc_frame_pull(pixels, 0, c.byref(frame))
        if code == a.AM_ERR_SDK_DEADLINE_EXCEEDED:
            time.sleep(.01)
            continue
        h.checked(code)
        try:
            meta = h.sized(a.am_frame_meta_t)
            h.checked(lib.amc_frame_get_meta(frame, c.byref(meta)))
            return dict(pts=meta.pts, generation=meta.source_generation,
                        frame_id=meta.frame_id), code
        finally:
            h.checked(lib.amc_frame_release(frame))
    return None, a.AM_ERR_SDK_DEADLINE_EXCEEDED


def resume_case(lib, ctx, url, software, publisher_factory, work, tag):
    """断流恢复：拉帧→杀推流→静默期→重启推流→新帧到达，代次递增、身份单调。"""
    publisher = None
    source = encoded = pixels = None
    try:
        publisher = publisher_factory(tag)
        time.sleep(.8)
        source, encoded, pixels = open_source(lib, ctx, url, software)
        first, code = drain_frame(lib, pixels, 10)
        assert first is not None, "推流正常时应拉到首帧"
        before_generation = first["generation"]
        before_stats = h.stats_snapshot(lib, ctx)
        h.stop(publisher); publisher = None
        # 断流后管线尾帧允许有限排空；排空窗口后不得再出帧。
        deadline = time.monotonic() + 1.5
        while time.monotonic() < deadline:
            drain_frame(lib, pixels, .2)
        deadline = time.monotonic() + 3
        silent = None
        while time.monotonic() < deadline:
            silent, _ = drain_frame(lib, pixels, .3)
            if silent is not None:
                break
        assert silent is None, f"断流后不应继续出帧：{silent}"
        h.report(lib, source)
        # 重启推流（新连接、新关键帧）：恢复出帧且源代次递增。
        publisher = publisher_factory(tag)
        recovered, code = drain_frame(lib, pixels, 15)
        assert recovered is not None, "重启推流后应在关键帧处恢复出帧"
        assert recovered["generation"] > before_generation, \
            f"源重连应递增代次：{before_generation} -> {recovered['generation']}"
        after_stats = h.stats_snapshot(lib, ctx)
        assert after_stats["reconnect_count"] > before_stats["reconnect_count"], \
            f"重连计数应递增：{before_stats['reconnect_count']} -> {after_stats['reconnect_count']}"
        follow, _ = drain_frame(lib, pixels, 5)
        assert follow is not None and follow["pts"] > recovered["pts"], "恢复后帧身份应单调"
        assert follow["frame_id"] != recovered["frame_id"]
        return dict(case="resume", software=software, first=first, recovered=recovered, follow=follow,
                    reconnect_count=[before_stats["reconnect_count"], after_stats["reconnect_count"]])
    finally:
        h.stop(publisher)
        h.close(lib, pixels, lib.amc_frame_unsubscribe)
        h.close(lib, encoded, lib.amc_encoded_unsubscribe)
        h.close(lib, source, lib.amc_source_close)


def close_race_case(lib, ctx, url, software, publisher_factory, work, tag):
    """关闭竞争：帧持续在途时关闭源，排空语义与后续拒绝。"""
    publisher = None
    source = encoded = pixels = None
    try:
        publisher = publisher_factory(tag)
        time.sleep(.8)
        source, encoded, pixels = open_source(lib, ctx, url, software)
        first, _ = drain_frame(lib, pixels, 10)
        assert first is not None, "关闭前应已出帧"
        # 在途订阅未关闭时先关源（仅排空关闭，不销毁）；按合同不撕裂。
        state = c.c_uint32()
        h.checked(lib.amc_source_close(source, 2000, c.byref(state)))
        frame = c.POINTER(a.amc_frame_t)()
        code = lib.amc_frame_pull(pixels, 0, c.byref(frame))
        assert code == a.AM_ERR_SDK_HANDLE_CLOSED, f"源关闭后帧拉取应拒绝：{code}"
        packet = c.POINTER(a.am_encoded_packet_t)()
        code = lib.amc_encoded_pull(encoded, 0, c.byref(packet))
        assert code == a.AM_ERR_SDK_HANDLE_CLOSED, f"源关闭后包拉取应拒绝：{code}"
        h.close(lib, pixels, lib.amc_frame_unsubscribe)
        h.close(lib, encoded, lib.amc_encoded_unsubscribe)
        h.checked(lib.amc_object_destroy(source))
        assert h.stats_snapshot(lib, ctx)["external_held_items"] == 0
        return dict(case="close-race", software=software, closed_then_rejected=True, external_held_zero=True)
    finally:
        h.stop(publisher)
        try:
            h.close(lib, pixels, lib.amc_frame_unsubscribe)
            h.close(lib, encoded, lib.amc_encoded_unsubscribe)
        except Exception:
            pass
        try:
            if source is not None:
                lib.amc_source_close(source, 500, c.byref(c.c_uint32()))
                lib.amc_object_destroy(source)
        except Exception:
            pass


def main(args):
    work = local_path(str(args.work.resolve()), "tests", fresh=True)
    output = local_path(str(args.output.resolve()), "evidence", fresh=True)
    work.mkdir(); output.mkdir()
    search = os.add_dll_directory(str(args.dll_dir.resolve()))
    server = None
    try:
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0)); port = sock.getsockname()[1]
        config = work / "mediamtx.yml"
        config.write_text(f"logLevel: warn\nrtspAddress: 127.0.0.1:{port}\nrtspTransports: [tcp]\nrtmp: no\nhls: no\nwebrtc: no\nsrt: no\nmoq: no\npaths:\n  all_others:\n", encoding="utf-8")
        with (work / "server.log").open("wb") as log:
            server = subprocess.Popen([str(args.mediamtx.resolve()), str(config)], stdout=log, stderr=subprocess.STDOUT)
            deadline = time.monotonic() + 8
            while True:
                try:
                    with socket.create_connection(("127.0.0.1", port), .2):
                        break
                except OSError:
                    if time.monotonic() > deadline or server.poll() is not None:
                        raise RuntimeError("回环RTSP未就绪")
                    time.sleep(.05)
            dll = args.dll_dir.resolve() / "am_camera.dll"
            lib = c.CDLL(str(dll)); a.bind(lib, "amc_", require_all=True)
            stream = work / "loop.h264"
            stream.write_bytes((args.inputs / "one.h264").read_bytes() * 60)

            def publisher_factory(name):
                url = f"rtsp://127.0.0.1:{port}/{name}"
                log = (work / f"publisher-{name}.log").open("ab")
                process = subprocess.Popen(["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error",
                                            "-re", "-stream_loop", "-1", "-r", "10", "-f", "h264",
                                            "-i", str(stream), "-an", "-c:v", "copy", "-f", "rtsp",
                                            "-rtsp_transport", "tcp", url],
                                           stdout=log, stderr=subprocess.STDOUT)
                process.url = url
                return process

            results = []
            for software in [True, False]:
                ctx = ctx_for(lib)
                try:
                    row = resume_case(lib, ctx, f"rtsp://127.0.0.1:{port}/resume-{int(software)}",
                                      software, publisher_factory, work, f"resume-{int(software)}")
                finally:
                    h.close(lib, ctx, lib.amc_shutdown)
                results.append(row)
                (output / f"resume-{int(software)}.json").write_text(json.dumps(row, indent=2), encoding="utf-8")
            for software in [True, False]:
                ctx = ctx_for(lib)
                try:
                    row = close_race_case(lib, ctx, f"rtsp://127.0.0.1:{port}/race-{int(software)}",
                                          software, publisher_factory, work, f"race-{int(software)}")
                finally:
                    h.close(lib, ctx, lib.amc_shutdown)
                results.append(row)
                (output / f"race-{int(software)}.json").write_text(json.dumps(row, indent=2), encoding="utf-8")
            identity = dict(dll_sha256=file_sha256(dll), input_sha256=file_sha256(args.inputs / "one.h264"),
                            scope="loopback fault matrix; resume and close semantics; not performance")
            (output / "identity.json").write_text(json.dumps(identity, indent=2), encoding="utf-8")
            print(json.dumps(dict(cases=len(results), output=str(output))))
    finally:
        h.stop(server); search.close()


def ctx_for(lib):
    dirs = (c.c_char_p * 1)(str(RUNTIME_DIR).encode())
    init = h.sized(a.amc_init_params_t)
    init.abi_version, init.runtime_dirs_utf8, init.runtime_dir_count = a.AM_ABI_VERSION, dirs, 1
    ctx = c.POINTER(a.amc_context_t)()
    h.checked(lib.amc_init(c.byref(init), c.byref(ctx)))
    return ctx


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("dll-dir", "runtime", "inputs", "mediamtx", "work", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    RUNTIME_DIR = args.runtime.resolve()
    main(args)
