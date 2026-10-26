"""真实媒体DLL入口的回环参数变化：无现场设备，无帧率收益声明。"""
import argparse
import ctypes as c
import hashlib
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


def exercise(lib, runtime, url, codec, software):
    dirs = (c.c_char_p * 1)(str(runtime).encode())
    init = h.sized(a.amc_init_params_t)
    init.abi_version, init.runtime_dirs_utf8, init.runtime_dir_count = a.AM_ABI_VERSION, dirs, 1
    ctx, source = c.POINTER(a.amc_context_t)(), c.POINTER(a.amc_source_t)()
    encoded, pixels = c.POINTER(a.amc_encoded_sub_t)(), c.POINTER(a.amc_frame_sub_t)()
    held = c.POINTER(a.amc_frame_t)()
    h.checked(lib.amc_init(c.byref(init), c.byref(ctx)))
    rows, packets, initial = [], [], None
    try:
        credentials = h.sized(a.am_credential_data_t)
        credentials.username_utf8, credentials.password_utf8 = b"synthetic", b"local-fixture"
        desc = h.sized(a.am_source_desc_t)
        desc.kind, desc.url_utf8, desc.creds = a.AM_SRC_RTSP_URL, url.encode(), c.pointer(credentials)
        h.checked(lib.amc_source_open(ctx, c.byref(desc), c.byref(source)))
        h.checked(lib.amc_source_set_decode_policy(source, a.AM_DHW_SOFTWARE_ONLY if software else a.AM_DHW_PREFER_HW))
        es = h.sized(a.am_encoded_sub_desc_t)
        es.codec_mask, es.mode, es.queue_max_items, es.queue_max_bytes = codec, a.AM_SUB_PULL, 36, 8 * 1024 * 1024
        h.subscribe(lib, lib.amc_subscribe_encoded, source, es, encoded)
        fs = h.sized(a.am_frame_sub_desc_t)
        fs.pixel_format, fs.mode = a.AM_PF_RGBA8, a.AM_SUB_PULL
        h.subscribe(lib, lib.amc_subscribe_frames, source, fs, pixels)
        deadline = time.monotonic() + 14
        while time.monotonic() < deadline:
            packet = c.POINTER(a.am_encoded_packet_t)()
            code = lib.amc_encoded_pull(encoded, 0, c.byref(packet))
            if code == a.AM_OK:
                try:
                    p = packet.contents
                    packets.append((p.seq, p.source_generation, p.config_generation))
                finally:
                    h.checked(lib.amc_encoded_packet_release(packet))
            elif code != a.AM_ERR_SDK_DEADLINE_EXCEEDED:
                h.checked(code)
            frame = c.POINTER(a.amc_frame_t)()
            code = lib.amc_frame_pull(pixels, 0, c.byref(frame))
            if code == a.AM_ERR_SDK_DEADLINE_EXCEEDED:
                time.sleep(.001)
                continue
            h.checked(code)
            try:
                view, meta = h.sized(a.am_frame_view_t), h.sized(a.am_frame_meta_t)
                h.checked(lib.amc_frame_get_view(frame, c.byref(view)))
                h.checked(lib.amc_frame_get_meta(frame, c.byref(meta)))
                dims = (view.width, view.height)
                rows.append(dict(dimensions=dims, pts=meta.pts, generation=meta.source_generation,
                                 config_generation=meta.config_generation, frame_id=meta.frame_id))
                current = h.report(lib, source)
                assert current["stream_verified"]
                assert current["actual_backend"] == ("ffmpeg-cpu" if software else "ffmpeg-d3d11va"), current
                if initial is None:
                    initial = current
                    h.checked(lib.amc_frame_retain(frame))
                    held = frame
                    held_view = view
                    held_hash = hashlib.sha256(c.string_at(view.planes[0], view.plane_size_bytes[0])).hexdigest()
                elif dims != tuple(rows[0]["dimensions"]):
                    assert current["decode_generation"] > initial["decode_generation"]
                    assert current["selection"] == initial["selection"], "参数重建不重新选择硬件"
                    assert hashlib.sha256(c.string_at(held_view.planes[0], held_view.plane_size_bytes[0])).hexdigest() == held_hash
                    break
            finally:
                h.checked(lib.amc_frame_release(frame))
        assert {tuple(row["dimensions"]) for row in rows} == {(256, 144), (128, 96)}, rows
        assert len({row["generation"] for row in rows}) == 1
        assert all(a["pts"] < b["pts"] for a, b in zip(rows, rows[1:]))
        assert len({row["frame_id"] for row in rows}) == len(rows)
        assert packets and len({p[1] for p in packets}) == 1
        assert all(x[0] < y[0] for x, y in zip(packets, packets[1:]))
        return dict(software=software, codec=codec, frames=rows, packets=packets,
                    initial=initial, final=h.report(lib, source), held_frame_unchanged=True)
    finally:
        try:
            if held:
                h.checked(lib.amc_frame_release(held))
            if pixels:
                h.close(lib, pixels, lib.amc_frame_unsubscribe)
            if encoded:
                h.close(lib, encoded, lib.amc_encoded_unsubscribe)
        finally:
            try:
                if source:
                    h.close(lib, source, lib.amc_source_close)
                assert h.stats_snapshot(lib, ctx)["external_held_items"] == 0
            finally:
                h.close(lib, ctx, lib.amc_shutdown)


def main(args):
    work = local_path(str(args.work.resolve()), "tests", fresh=True)
    output = local_path(str(args.output.resolve()), "evidence", fresh=True)
    work.mkdir(); output.mkdir()
    server = publisher = None
    search = os.add_dll_directory(str(args.dll_dir.resolve()))
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
            results = []
            for codec, files, fmt in [(a.AM_CODEC_H264, ("one.h264", "alternate.h264"), "h264"),
                                      (a.AM_CODEC_H265, ("first-lowdelay.hevc", "alternate-lowdelay.hevc"), "hevc")]:
                stream = work / f"switch.{fmt}"
                stream.write_bytes(b"".join((args.inputs / name).read_bytes() * 40 for name in files))
                for software in [True, False]:
                    url = f"rtsp://127.0.0.1:{port}/switch-{codec}-{int(software)}"
                    with (work / f"publisher-{codec}-{int(software)}.log").open("wb") as log:
                        command = ["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-re", "-stream_loop", "-1",
                                   "-r", "10", "-f", fmt, "-i", str(stream), "-an", "-c:v", "copy", "-f", "rtsp", "-rtsp_transport", "tcp", url]
                        publisher = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
                        try:
                            time.sleep(.8)
                            row = exercise(lib, args.runtime.resolve(), url, codec, software)
                            results.append(row)
                            (output / f"case-{codec}-{int(software)}.json").write_text(json.dumps(row, indent=2), encoding="utf-8")
                        finally:
                            h.stop(publisher); publisher = None
            version = h.sized(a.am_version_info_t); h.checked(lib.amc_get_version_info(c.byref(version)))
            identity = dict(dll_sha256=file_sha256(dll), build_notes=version.runtime_notes_utf8.decode(),
                            inputs={name: file_sha256(args.inputs / name) for name in ("one.h264", "alternate.h264", "first-lowdelay.hevc", "alternate-lowdelay.hevc")},
                            runtime={p.name: file_sha256(p) for p in args.runtime.glob("*.dll")},
                            observer_sha256=file_sha256(Path(__file__)), scope="loopback parameter switch; not performance or soak")
            (output / "identity.json").write_text(json.dumps(identity, indent=2), encoding="utf-8")
            print(json.dumps(dict(cases=len(results), output=str(output), source_generations_unchanged=True, retained_pixels_unchanged=True)))
    finally:
        h.stop(publisher); h.stop(server); search.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("dll-dir", "runtime", "inputs", "mediamtx", "work", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    main(parser.parse_args())
