"""真实相机 DLL 的隔离 RTSP 回归：随机本地端口、自产流，不接触现场设备。"""
import argparse
import ctypes as c
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdk" if (ROOT / "sdk/bindings").is_dir() else ROOT
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a
import importlib.util
_report_spec = importlib.util.spec_from_file_location("hardware_report_check", Path(__file__).with_name("test-hardware-report.py"))
_report_module = importlib.util.module_from_spec(_report_spec)
_report_spec.loader.exec_module(_report_module)
snapshot = _report_module.snapshot


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def checked(code):
    assert code == a.AM_OK, (code, a.AM_ERR_SDK_TRY_AGAIN)


def subscribe_when_ready(function, source, desc, out, lib):
    deadline = time.monotonic() + 10
    while True:
        code = function(source, c.byref(desc), c.byref(out))
        if code != a.AM_ERR_CAMERA_PROBE_PENDING:
            checked(code)
            return
        if time.monotonic() >= deadline:
            status = sized(a.am_source_status_t)
            checked(lib.amc_source_get_status(source, c.byref(status)))
            raise AssertionError(f"Source metadata timeout: state={status.state}, fault={status.fault_class}, attempts={status.reconnect_attempts}, last_frame={status.last_frame_at_ms}")
        time.sleep(.05)


def close(lib, obj, method):
    state = c.c_uint32()
    deadline = time.monotonic() + 8
    while True:
        checked(method(obj, 250, c.byref(state)))
        if state.value == a.AM_LIFECYCLE_CLOSED:
            break
        assert time.monotonic() < deadline, state.value
    checked(lib.amc_object_destroy(obj))


def nal_types(data, codec):
    types = []
    index = 0
    while index + 3 < len(data):
        size = 4 if data[index:index+4] == b"\0\0\0\1" else (
            3 if data[index:index+3] == b"\0\0\1" else 0)
        if size:
            byte = data[index + size]
            types.append((byte & 31) if codec == a.AM_CODEC_H264 else ((byte >> 1) & 63))
            index += size
        else:
            index += 1
    return types


def native_child(args):
    """直接读取锁定 FFmpeg 63 前缀，与 ffprobe 核对真实 AVStream.time_base 布局。"""
    directory = os.add_dll_directory(str(Path(args.runtime_dir).resolve()))
    libraries = [c.CDLL(str(Path(args.runtime_dir).resolve() / name)) for name in
                 ["avutil-61.dll", "swresample-7.dll", "swscale-10.dll", "avcodec-63.dll", "avformat-63.dll"]]
    util, codec, fmt = libraries[0], libraries[3], libraries[4]
    class Rational(c.Structure):
        _fields_ = [("num", c.c_int), ("den", c.c_int)]
    class Parameters(c.Structure):
        _fields_ = [("type", c.c_int), ("codec", c.c_int), ("tag", c.c_uint),
                    ("extra", c.c_void_p), ("extra_size", c.c_int), ("side", c.c_void_p),
                    ("side_count", c.c_int), ("format", c.c_int), ("bitrate", c.c_int64),
                    ("coded_bits", c.c_int), ("raw_bits", c.c_int), ("profile", c.c_int),
                    ("level", c.c_int), ("width", c.c_int), ("height", c.c_int)]
    class Stream(c.Structure):
        _fields_ = [("class_", c.c_void_p), ("index", c.c_int), ("id", c.c_int),
                    ("params", c.POINTER(Parameters)), ("private", c.c_void_p), ("timebase", Rational)]
    class Format(c.Structure):
        _fields_ = [("class_", c.c_void_p), ("input", c.c_void_p), ("output", c.c_void_p),
                    ("private", c.c_void_p), ("io", c.c_void_p), ("flags", c.c_int),
                    ("count", c.c_uint), ("streams", c.POINTER(c.POINTER(Stream)))]
    interrupt_type=c.CFUNCTYPE(c.c_int,c.c_void_p)
    class Interrupt(c.Structure):
        _fields_=[("callback",interrupt_type),("opaque",c.c_void_p)]
    class FullFormat(Format):
        _fields_=[("group_count",c.c_uint),("groups",c.c_void_p),("chapter_count",c.c_uint),
            ("chapters",c.c_void_p),("url",c.c_void_p),("start",c.c_int64),("duration",c.c_int64),
            ("bitrate",c.c_int64),("packet_size",c.c_uint),("max_delay",c.c_int),("flags_full",c.c_int),
            ("probe_size",c.c_int64),("analyze_duration",c.c_int64),("key",c.c_void_p),("keylen",c.c_int),
            ("program_count",c.c_uint),("programs",c.c_void_p),("video_codec",c.c_int),("audio_codec",c.c_int),
            ("subtitle_codec",c.c_int),("data_codec",c.c_int),("metadata",c.c_void_p),("realtime",c.c_int64),
            ("fps_probe",c.c_int),("error_recognition",c.c_int),("interrupt",Interrupt)]
    class Packet(c.Structure):
        _fields_ = [("buffer", c.c_void_p), ("pts", c.c_int64), ("dts", c.c_int64),
                    ("data", c.c_void_p), ("size", c.c_int), ("stream", c.c_int), ("flags", c.c_int)]
    util.av_dict_set.argtypes = [c.POINTER(c.c_void_p), c.c_char_p, c.c_char_p, c.c_int]
    util.av_dict_free.argtypes = [c.POINTER(c.c_void_p)]
    util.av_log_get_level.restype = c.c_int
    util.av_log_set_level.argtypes = [c.c_int]
    fmt.avformat_open_input.argtypes = [c.POINTER(c.POINTER(Format)), c.c_char_p, c.c_void_p, c.POINTER(c.c_void_p)]
    fmt.avformat_find_stream_info.argtypes = [c.POINTER(Format), c.c_void_p]
    fmt.avformat_close_input.argtypes = [c.POINTER(c.POINTER(Format))]
    fmt.avformat_alloc_context.restype = c.POINTER(Format)
    fmt.av_read_frame.argtypes = [c.POINTER(Format), c.POINTER(Packet)]
    fmt.av_find_best_stream.argtypes = [c.POINTER(Format),c.c_int,c.c_int,c.c_int,c.c_void_p,c.c_int]
    codec.av_packet_alloc.restype = c.POINTER(Packet)
    codec.av_packet_unref.argtypes = [c.POINTER(Packet)]
    codec.av_packet_free.argtypes = [c.POINTER(c.POINTER(Packet))]
    codec.avcodec_get_name.argtypes=[c.c_int]
    codec.avcodec_get_name.restype=c.c_char_p
    context, options = fmt.avformat_alloc_context(), c.c_void_p()
    callback_deadline=time.monotonic()+8
    callback=interrupt_type(lambda _opaque: int(time.monotonic()>=callback_deadline))
    c.cast(context,c.POINTER(FullFormat)).contents.interrupt=Interrupt(callback,None)
    packet = c.POINTER(Packet)()
    level = util.av_log_get_level()
    util.av_log_set_level(-8)
    try:
        assert fmt.avformat_version() >> 16 == 63
        for key, value in [(b"rtsp_transport", b"tcp"), (b"rw_timeout", b"3000000"),
                           (b"analyzeduration", b"500000"), (b"probesize", b"32768"),
                           (b"fflags", b"nobuffer"), (b"max_delay", b"0"), (b"reorder_queue_size", b"0")]:
            assert util.av_dict_set(c.byref(options), key, value, 0) >= 0
        native_url=args.url.replace("rtsp://","rtsp://sdk-transport-user:sdk-transport-secret@",1)
        assert fmt.avformat_open_input(c.byref(context), native_url.encode(), None, c.byref(options)) >= 0
        assert fmt.avformat_find_stream_info(context, None) >= 0
        assert fmt.av_find_best_stream(context,0,-1,-1,None,0)>=0
        stream = next(context.contents.streams[i].contents for i in range(context.contents.count)
                      if context.contents.streams[i].contents.params.contents.type == 0)
        timebase = f"{stream.timebase.num}/{stream.timebase.den}"
        assert timebase == args.timebase, (timebase, args.timebase)
        actual_codec=codec.avcodec_get_name(stream.params.contents.codec).decode()
        expected_codec = {a.AM_CODEC_H264: "h264", a.AM_CODEC_H265: "hevc", a.AM_CODEC_MJPEG: "mjpeg"}[args.codec]
        assert actual_codec == expected_codec,actual_codec
        assert (stream.params.contents.width, stream.params.contents.height) == (256, 144), (stream.params.contents.width, stream.params.contents.height, stream.params.contents.codec, stream.params.contents.extra_size)
        packet = codec.av_packet_alloc()
        assert packet
        rows = []
        while len(rows) < 8:
            assert fmt.av_read_frame(context, packet) >= 0
            if packet.contents.stream == stream.index:
                assert 0 < packet.contents.size <= 16 * 1024 * 1024
                rows.append(packet.contents.pts)
            codec.av_packet_unref(packet)
        return {"codec": args.codec, "nativeLayoutValidated": True, "timeBase": timebase,
                "nativeCodecId":stream.params.contents.codec,"nativeCodecName":actual_codec,
                "interruptOffset":FullFormat.interrupt.offset,
                "streamTimebaseOffset": Stream.timebase.offset, "packets": len(rows)}
    finally:
        if packet:
            codec.av_packet_free(c.byref(packet))
        if context:
            fmt.avformat_close_input(c.byref(context))
        util.av_dict_free(c.byref(options))
        util.av_log_set_level(level)
        directory.close()


def child(args):
    directory = os.add_dll_directory(str(Path(args.dll_dir).resolve()))
    lib = c.CDLL(str(Path(args.dll_dir).resolve() / "am_camera.dll"))
    a.bind(lib, "amc_")
    util=c.CDLL(str(Path(args.runtime_dir).resolve()/"avutil-61.dll"))
    util.av_log_get_level.restype=c.c_int
    original_log_level=util.av_log_get_level()
    params = sized(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    runtime = (c.c_char_p * 1)(str(Path(args.runtime_dir).resolve()).encode())
    params.runtime_dirs_utf8, params.runtime_dir_count = runtime, 1
    ctx = c.POINTER(a.amc_context_t)()
    checked(lib.amc_init(c.byref(params), c.byref(ctx)))
    credentials = sized(a.am_credential_data_t)
    credentials.username_utf8, credentials.password_utf8 = b"sdk-transport-user", b"sdk-transport-secret"
    desc = sized(a.am_source_desc_t)
    desc.kind, desc.url_utf8, desc.stream_kind_utf8 = a.AM_SRC_RTSP_URL, args.url.encode(), b"main"
    desc.creds, desc.reuse = c.pointer(credentials), 1
    source = c.POINTER(a.amc_source_t)()
    checked(lib.amc_source_open(ctx, c.byref(desc), c.byref(source)))
    policy = a.AM_DHW_SOFTWARE_ONLY if args.decode_policy == "software" else a.AM_DHW_PREFER_HW
    checked(lib.amc_source_set_decode_policy(source, policy))
    encoded_desc = sized(a.am_encoded_sub_desc_t)
    encoded_desc.codec_mask, encoded_desc.mode = args.codec, a.AM_SUB_PULL
    encoded_desc.queue_max_items, encoded_desc.queue_max_bytes = 36, 8 * 1024 * 1024
    sub = c.POINTER(a.amc_encoded_sub_t)()
    subscribe_when_ready(lib.amc_subscribe_encoded, source, encoded_desc, sub, lib)
    assert util.av_log_get_level()==-8,"RTSP credentials must not reach native diagnostic logs"
    slow_desc = sized(a.am_encoded_sub_desc_t)
    slow_desc.codec_mask, slow_desc.mode = args.codec, a.AM_SUB_PULL
    slow_desc.queue_max_items, slow_desc.queue_max_bytes = 2, 1024 * 1024
    slow = c.POINTER(a.amc_encoded_sub_t)()
    subscribe_when_ready(lib.amc_subscribe_encoded, source, slow_desc, slow, lib)
    before_pixels = snapshot(lib, source)
    assert before_pixels["state"] == "idle" and before_pixels["selection"] is None
    assert not before_pixels["decoder_active"] and before_pixels["actual_backend"] is None
    frame_desc = sized(a.am_frame_sub_desc_t)
    frame_desc.pixel_format, frame_desc.sampling_max_fps = a.AM_PF_RGBA8, 4
    frame_desc.mode, frame_desc.queue_max_items, frame_desc.queue_max_bytes = a.AM_SUB_PULL, 4, 2 * 1024 * 1024
    frames = c.POINTER(a.amc_frame_sub_t)()
    subscribe_when_ready(lib.amc_subscribe_frames, source, frame_desc, frames, lib)
    assert lib.amc_source_set_decode_policy(source, 1-policy) == a.AM_ERR_CAMERA_MODE_CONFLICT
    checked(lib.amc_source_set_decode_policy(source, policy))
    timestamps, sequences, identities, keyframes, sizes = [], [], [], 0, []
    deadline = time.monotonic() + 15
    while len(timestamps) < 16:
        assert time.monotonic() < deadline, "RTSP packet deadline"
        packet = c.POINTER(a.am_encoded_packet_t)()
        code = lib.amc_encoded_pull(sub, 500, c.byref(packet))
        if code in (a.AM_ERR_SDK_TRY_AGAIN, a.AM_ERR_SDK_DEADLINE_EXCEEDED):
            continue
        checked(code)
        row = packet.contents
        assert row.codec == args.codec and 0 < row.size <= 16 * 1024 * 1024
        assert (row.time_base_num, row.time_base_den) == tuple(map(int, args.timebase.split("/"))), (
            row.time_base_num, row.time_base_den, args.timebase)
        payload = c.string_at(row.data, row.size)
        if args.codec == a.AM_CODEC_MJPEG:
            assert payload.startswith(b"\xff\xd8") and payload.endswith(b"\xff\xd9"), "MJPEG packet must contain a complete JPEG image"
            assert row.flags & a.AM_PKT_KEYFRAME, "Every MJPEG image must be independently decodable"
            keyframes += 1
        else:
            assert payload.startswith((b"\0\0\1", b"\0\0\0\1"))
            types = nal_types(payload, args.codec)
            if row.flags & a.AM_PKT_KEYFRAME:
                keyframes += 1
                required = {7, 8} if args.codec == a.AM_CODEC_H264 else {32, 33, 34}
                assert required.issubset(types), (required, types)
        timestamps.append(row.pts)
        sequences.append(row.seq)
        identities.append((row.source_id, row.source_generation))
        sizes.append(row.size)
        checked(lib.amc_encoded_packet_retain(packet))
        checked(lib.amc_encoded_packet_release(packet))
        assert c.string_at(packet.contents.data, packet.contents.size) == payload
        checked(lib.amc_encoded_packet_release(packet))
    assert keyframes and sequences == sorted(set(sequences))
    assert len(set(identities)) == 1 and identities[0][0] != 0
    assert len(set(timestamps)) >= 8
    slow_count=0
    deadline=time.monotonic()+10
    while slow_count<3:
        assert time.monotonic()<deadline,"Slow subscriber did not resynchronize"
        packet=c.POINTER(a.am_encoded_packet_t)()
        code=lib.amc_encoded_pull(slow,500,c.byref(packet))
        if code in (a.AM_ERR_SDK_TRY_AGAIN,a.AM_ERR_SDK_DEADLINE_EXCEEDED):
            continue
        checked(code)
        if slow_count==0:
            assert packet.contents.flags&a.AM_PKT_KEYFRAME,"Slow subscriber resumed at a dependent frame"
        assert (packet.contents.source_id,packet.contents.source_generation)==identities[0]
        checked(lib.amc_encoded_packet_release(packet))
        slow_count+=1
    close(lib,slow,lib.amc_encoded_unsubscribe)
    decoded = []
    deadline = time.monotonic() + 10
    while len(decoded) < 6:
        assert time.monotonic() < deadline, "RTSP decoded frame deadline"
        frame = c.POINTER(a.amc_frame_t)()
        code = lib.amc_frame_pull(frames, 500, c.byref(frame))
        if code in (a.AM_ERR_SDK_TRY_AGAIN, a.AM_ERR_SDK_DEADLINE_EXCEEDED):
            continue
        checked(code)
        view, meta = sized(a.am_frame_view_t), sized(a.am_frame_meta_t)
        checked(lib.amc_frame_get_view(frame, c.byref(view)))
        checked(lib.amc_frame_get_meta(frame, c.byref(meta)))
        assert (view.width, view.height, view.format, view.plane_count) == (256, 144, a.AM_PF_RGBA8, 1)
        assert view.strides[0] >= 256 * 4 and view.plane_size_bytes[0] >= view.strides[0] * 144
        assert (meta.source_id, meta.source_generation) == identities[0]
        assert (meta.time_base_num, meta.time_base_den) == tuple(map(int, args.timebase.split("/")))
        pixels = c.string_at(view.planes[0], view.plane_size_bytes[0])
        assert len(set(pixels)) > 8, "Decoded test pattern is not a real color image"
        decoded.append(meta.pts)
        checked(lib.amc_frame_release(frame))
    assert len(set(decoded)) == len(decoded), decoded
    hardware = snapshot(lib, source)
    assert hardware["stream_verified"] and hardware["decoder_active"]
    assert hardware["requested_policy"] == policy and hardware["policy_locked"]
    if args.codec == a.AM_CODEC_MJPEG:
        assert hardware["actual_backend"] == "image-cpu"
    elif policy == a.AM_DHW_SOFTWARE_ONLY:
        assert hardware["actual_backend"] == "ffmpeg-cpu"
        assert hardware["selection"]["attempts"] == []
    elif args.require_hardware:
        assert hardware["actual_backend"] in ("ffmpeg-d3d11va", "ffmpeg-dxva2"), hardware
    close(lib, frames, lib.amc_frame_unsubscribe)
    close(lib, sub, lib.amc_encoded_unsubscribe)
    close(lib, source, lib.amc_source_close)
    assert util.av_log_get_level()==original_log_level,"Last Reader did not restore native diagnostic level"
    close(lib, ctx, lib.amc_shutdown)
    directory.close()
    return {"codec": args.codec, "packets": len(timestamps), "keyframes": keyframes,
            "timeBase": args.timebase, "maxPacketBytes": max(sizes), "retainRelease": True,
            "hardware":hardware, "compressedOnlyDidNotDecode":True, "decodedFrames":len(decoded), "decodedRgbaRealPixels":True,"nativeLogLevelRestored":True,
            "slowSubscriberResynchronized":True,"slowSubscriberPackets":slow_count}


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def stop(process):
    if process and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def main(args):
    from local_paths import local_path
    output = local_path(args.work_dir, "tests")
    output.mkdir(parents=True, exist_ok=True)
    port = free_port()
    config = output / "mediamtx.yml"
    config.write_text(f"""logLevel: warn
rtspAddress: 127.0.0.1:{port}
rtspTransports: [tcp]
rtmp: no
hls: no
webrtc: no
srt: no
moq: no
paths:
  all_others:
""", encoding="utf-8")
    rows = []
    with (output / "mediamtx.log").open("wb") as server_log:
        server = subprocess.Popen([str(Path(args.mediamtx).resolve()), str(config)],
                                  cwd=output, stdout=server_log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 8
            while True:
                assert server.poll() is None, "MediaMTX startup failed; inspect isolated log"
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=.2):
                        break
                except OSError:
                    assert time.monotonic() < deadline, "MediaMTX startup timeout"
                    time.sleep(.05)
            for codec, name, encoder in [(a.AM_CODEC_H264, "h264", "libx264"),
                                          (a.AM_CODEC_H265, "h265", "libx265"),
                                          (a.AM_CODEC_MJPEG, "mjpeg", "mjpeg")]:
                if args.only_codec and name != args.only_codec:
                    continue
                url = f"rtsp://127.0.0.1:{port}/{name}"
                command = ["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-re",
                           "-f", "lavfi", "-i", "testsrc2=size=256x144:rate=12", "-an",
                           "-c:v", encoder, "-threads", "1"]
                if codec == a.AM_CODEC_MJPEG:
                    # RFC2435接收端要求亮度/色度两张量化表，不能省略相同的色度表。
                    command += ["-pix_fmt", "yuvj420p", "-huffman", "default", "-q:v", "3", "-force_duplicated_matrix", "1"]
                else:
                    command += ["-preset", "ultrafast", "-tune", "zerolatency", "-g", "12"]
                if codec == a.AM_CODEC_H265:
                    command += ["-x265-params", "pools=none:frame-threads=1:wpp=0:log-level=error"]
                command += ["-f", "rtsp", "-rtsp_transport", "tcp", url]
                with (output / f"publisher-{name}.log").open("wb") as publisher_log:
                    publisher = subprocess.Popen(command, stdout=publisher_log, stderr=subprocess.STDOUT)
                    try:
                        time.sleep(.6)
                        assert publisher.poll() is None, f"{name} publisher failed"
                        probe = json.loads(subprocess.check_output(["ffprobe", "-v", "error", "-rtsp_transport", "tcp",
                            "-analyzeduration", "1000000", "-show_streams", "-of", "json", url],
                            text=True, timeout=12))
                        stream = next(row for row in probe["streams"] if row["codec_type"] == "video")
                        child_command = [sys.executable, __file__, "--child", "--dll-dir", args.dll_dir,
                            "--runtime-dir", args.runtime_dir, "--url", url, "--codec", str(codec),
                            "--timebase", stream["time_base"]]
                        child_command += ["--decode-policy", args.decode_policy]
                        if args.require_hardware:
                            child_command.append("--require-hardware")
                        if args.native_layout_only:
                            child_command.append("--native-layout-only")
                        result = subprocess.run(child_command, text=True, capture_output=True, timeout=45)
                        (output / f"{name}.stderr.log").write_text(result.stderr, encoding="utf-8")
                        assert "sdk-transport-secret" not in result.stderr and "sdk-transport-user" not in result.stderr
                        assert result.returncode == 0, (result.returncode, result.stderr[-4000:])
                        row=json.loads(result.stdout)
                        (output/f"{name}.result.json").write_text(json.dumps(row,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
                        rows.append(row)
                    finally:
                        stop(publisher)
        finally:
            stop(server)
    result={"passed": True, "isolatedPort": port, "cases": rows}
    (output / "result.json").write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", required=True)
    parser.add_argument("--runtime-dir", required=True)
    parser.add_argument("--work-dir", default=str(ROOT / ".local/tests/sdk-transport"))
    parser.add_argument("--decode-policy", choices=("prefer", "software"), default="prefer")
    parser.add_argument("--require-hardware", action="store_true")
    parser.add_argument("--mediamtx", default=str(Path(os.environ.get("TEMP", ".")) / "p08-mediamtx/mediamtx.exe"))
    parser.add_argument("--child", action="store_true")
    parser.add_argument("--url")
    parser.add_argument("--codec", type=int)
    parser.add_argument("--timebase")
    parser.add_argument("--native-layout-only", action="store_true")
    parser.add_argument("--only-codec", choices=("h264", "h265", "mjpeg"))
    args = parser.parse_args()
    result = (native_child(args) if args.native_layout_only else child(args)) if args.child else main(args)
    print(json.dumps(result, ensure_ascii=False))
