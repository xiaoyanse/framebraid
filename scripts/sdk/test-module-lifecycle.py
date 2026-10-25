"""隔离子进程验证工作线程回收与Windows DLL卸载重载；不连接设备。"""
import argparse
import ctypes as c
import gc
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("tensor_test", Path(__file__).with_name("test-tensors.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
a, base, size, check = t.a, t.base, t.size, t.check
spec = importlib.util.spec_from_file_location("decoder_test", Path(__file__).with_name("test-decoder.py"))
decoder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(decoder)


class ThreadEntry(c.Structure):
    _fields_ = [("size", c.c_uint32), ("usage", c.c_uint32), ("tid", c.c_uint32),
                ("pid", c.c_uint32), ("priority", c.c_int32),
                ("delta", c.c_int32), ("flags", c.c_uint32)]


class MemoryCounters(c.Structure):
    _fields_ = [("size", c.c_uint32), ("faults", c.c_uint32)] + [
        (name, c.c_size_t) for name in ("peakWorking", "working", "peakPaged", "paged",
                                      "peakNonPaged", "nonPaged", "pagefile", "peakPagefile", "private")]


def windows():
    k = c.WinDLL("kernel32", use_last_error=True)
    for name, args, result in [
        ("CreateToolhelp32Snapshot", [c.c_uint32, c.c_uint32], c.c_void_p),
        ("Thread32First", [c.c_void_p, c.POINTER(ThreadEntry)], c.c_int),
        ("Thread32Next", [c.c_void_p, c.POINTER(ThreadEntry)], c.c_int),
        ("OpenThread", [c.c_uint32, c.c_int, c.c_uint32], c.c_void_p),
        ("GetThreadDescription", [c.c_void_p, c.POINTER(c.c_void_p)], c.c_int32),
        ("CloseHandle", [c.c_void_p], c.c_int),
        ("LocalFree", [c.c_void_p], c.c_void_p),
        ("FreeLibrary", [c.c_void_p], c.c_int),
        ("GetModuleHandleW", [c.c_wchar_p], c.c_void_p),
        ("GetCurrentProcess", [], c.c_void_p),
        ("GetProcessHandleCount", [c.c_void_p, c.POINTER(c.c_uint32)], c.c_int),
    ]:
        fn = getattr(k, name)
        fn.argtypes, fn.restype = args, result
    return k


def resources(k):
    gc.collect()
    p = c.WinDLL("psapi", use_last_error=True)
    p.GetProcessMemoryInfo.argtypes = [c.c_void_p, c.POINTER(MemoryCounters), c.c_uint32]
    p.GetProcessMemoryInfo.restype = c.c_int
    memory = MemoryCounters()
    memory.size = c.sizeof(memory)
    assert p.GetProcessMemoryInfo(k.GetCurrentProcess(), c.byref(memory), memory.size)
    handles = c.c_uint32()
    assert k.GetProcessHandleCount(k.GetCurrentProcess(), c.byref(handles))
    return {"privateBytes": memory.private, "processHandles": handles.value,
            "sdkWorkers": len(workers(k))}


def workers(k):
    snapshot = k.CreateToolhelp32Snapshot(4, 0)
    assert snapshot != c.c_void_p(-1).value, c.get_last_error()
    names = []
    entry = ThreadEntry()
    entry.size = c.sizeof(entry)
    try:
        more = k.Thread32First(snapshot, c.byref(entry))
        while more:
            if entry.pid == os.getpid():
                handle = k.OpenThread(0x0800, 0, entry.tid)
                if not handle and c.get_last_error() == 87:
                    # 快照之后线程可能已退出；仅跳过这一明确的不存在错误。
                    more = k.Thread32Next(snapshot, c.byref(entry))
                    continue
                assert handle, (entry.tid, c.get_last_error())
                try:
                    label = c.c_void_p()
                    code = k.GetThreadDescription(handle, c.byref(label))
                    assert code >= 0, (entry.tid, hex(code & 0xffffffff))
                    try:
                        name = c.wstring_at(label) if label else ""
                        if name.startswith("am-sdk-infer-"):
                            names.append(name)
                    finally:
                        k.LocalFree(label)
                finally:
                    k.CloseHandle(handle)
            more = k.Thread32Next(snapshot, c.byref(entry))
    finally:
        k.CloseHandle(snapshot)
    return sorted(names)


def expected_workers(k, count):
    deadline = time.monotonic()+2
    while True:
        names = workers(k)
        if len(names) == count:
            return
        assert time.monotonic() < deadline, names
        time.sleep(0.005)


def load(vision, ctx, path):
    desc = size(a.am_model_desc_t)
    desc.path_utf8 = str(path.resolve()).encode()
    desc.load_mode, desc.intra_threads = a.AM_LOAD_RAW, 1
    model = c.POINTER(a.amv_model_t)()
    code = vision.amv_model_load(ctx, c.byref(desc), c.byref(model))
    return code, model


def infer(vision, ctx, model):
    data = struct.pack("<12f", *range(12))
    tensor = t.Tensor("images", a.AM_DTYPE_F32, [1, 3, 2, 2], data)
    result, rid = t.invoke(vision, ctx, model, [tensor])
    assert t.output(result)["pixels"][2] == data
    t.release(vision, ctx, result, rid)


def close_without_destroy(vision, model):
    state = c.c_uint32()
    check(vision.amv_model_close(model, 1000, c.byref(state)))
    assert state.value == a.AM_LIFECYCLE_CLOSED, state.value


def cycle(vision, args, k, stress=True):
    ctx = base.initialize(vision, args.runtime_dir)
    if not stress:
        code, model = load(vision, ctx, args.sample_dir/"image.onnx")
        check(code)
        infer(vision, ctx, model)
        base.close_model(vision, model)
        expected_workers(k, 0)
        base.shared.close(vision, ctx, False)
        return
    code, first = load(vision, ctx, args.sample_dir/"image.onnx")
    check(code)
    code, second = load(vision, ctx, args.sample_dir/"image.onnx")
    check(code)
    expected_workers(k, 2)
    infer(vision, ctx, first)
    close_without_destroy(vision, first)
    expected_workers(k, 2)
    close_without_destroy(vision, second)
    # CLOSED已经回收执行器，不要求调用方先销毁关闭句柄才停止线程。
    expected_workers(k, 0)
    check(vision.amv_object_destroy(first))
    check(vision.amv_object_destroy(second))
    code, failed = load(vision, ctx, args.sample_dir/"absent.onnx")
    assert code != 0 and not failed
    expected_workers(k, 0)

    def concurrent(_):
        for _ in range(10):
            code, model = load(vision, ctx, args.sample_dir/"image.onnx")
            check(code)
            infer(vision, ctx, model)
            base.close_model(vision, model)
    with ThreadPoolExecutor(max_workers=4) as executor:
        list(executor.map(concurrent, range(4)))
    expected_workers(k, 0)
    if args.model_dir:
        model, _ = base.load(vision, ctx, args.model_dir/"yolo11n-cls.onnx", "classify-yolo11n-prob-v1")
        encoded = base.png()
        data = (c.c_ubyte*len(encoded)).from_buffer_copy(encoded)
        image = size(a.am_image_input_t)
        image.kind, image.encoded, image.encoded_size = a.AM_IMG_PNG, data, len(encoded)
        result, rid, _ = base.sync(vision, ctx, model, image)
        check(vision.amv_result_ack(ctx, rid))
        check(vision.amv_result_release(result))
        base.close_model(vision, model)
        expected_workers(k, 0)
    base.shared.close(vision, ctx, False)


def camera_cycle(camera, args):
    p = size(a.amc_init_params_t)
    p.abi_version = a.AM_ABI_VERSION
    dirs = (c.c_char_p * 1)(str(args.ffmpeg_dir.resolve()).encode())
    p.runtime_dirs_utf8, p.runtime_dir_count = dirs, 1
    ctx = c.POINTER(a.amc_context_t)()
    check(camera.amc_init(c.byref(p), c.byref(ctx)))
    cfg = size(a.am_decoder_config_t)
    cfg.codec, cfg.output_pixel_format = a.AM_CODEC_H264, a.AM_PF_RGBA8
    cfg.hw_policy = a.AM_DHW_SOFTWARE_ONLY
    cfg.time_base_num, cfg.time_base_den = 1, 90000
    dec = c.POINTER(a.amc_decoder_t)()
    check(camera.amc_decoder_create(ctx, c.byref(cfg), c.byref(dec)))
    encoded = (args.codec_sample_dir/"one.h264").read_bytes()
    data = (c.c_ubyte * len(encoded)).from_buffer_copy(encoded)
    packet = size(a.am_decoder_input_t)
    packet.data, packet.size, packet.flags = data, len(encoded), a.AM_PKT_AU_COMPLETE
    check(camera.amc_decoder_submit(dec, c.byref(packet)))
    check(camera.amc_decoder_drain(dec))
    frame = c.POINTER(a.amc_frame_t)()
    check(camera.amc_decoder_receive(dec, 0, c.byref(frame)))
    view = size(a.am_frame_view_t)
    check(camera.amc_frame_get_view(frame, c.byref(view)))
    assert (view.width, view.height) == (256, 144)
    check(camera.amc_frame_release(frame))
    state = c.c_uint32()
    check(camera.amc_decoder_close(dec, 1000, c.byref(state)))
    assert state.value == a.AM_LIFECYCLE_CLOSED
    check(camera.amc_object_destroy(dec))
    base.shared.close(camera, ctx, True)


def child(args):
    k = windows()
    path = str((args.dll_dir/"am_vision.dll").resolve())
    # 测量期间不在内存中积累测试记录，避免把测试器自身增长算成SDK泄漏。
    record_file = tempfile.TemporaryFile(mode="w+t", encoding="utf-8")
    for index in range(args.reload_cycles):
        assert not k.GetModuleHandleW("am_vision.dll")
        vision = c.CDLL(path)
        a.bind(vision, "amv_")
        stress = index < 3
        if not args.binding_only:
            cycle(vision, args, k, stress)
        assert not k.GetModuleHandleW("am_camera.dll")
        camera = c.CDLL(str((args.dll_dir/"am_camera.dll").resolve()))
        a.bind(camera, "amc_")
        codec_args = argparse.Namespace(runtime_dir=args.ffmpeg_dir, samples_dir=args.codec_sample_dir,
                                        reference_dir=args.codec_reference_dir)
        codecs = [decoder.bframes(camera, codec_args, codec, name)
                  for codec, name in [(a.AM_CODEC_H264, "multi-h264.mp4"),
                                      (a.AM_CODEC_H265, "multi-h265.mp4")]] if stress and not args.binding_only else []
        if not stress and not args.binding_only:
            camera_cycle(camera, args)
        assert k.FreeLibrary(camera._handle), c.get_last_error()
        assert not k.GetModuleHandleW("am_camera.dll")
        del camera
        handle = vision._handle
        assert k.FreeLibrary(handle), c.get_last_error()
        assert not k.GetModuleHandleW("am_vision.dll"), "SDK module still loaded"
        del vision
        record = {"reload": index+1, **resources(k), "workersAfterClose": 0,
                        "concurrentLoadInferClose": 0 if args.binding_only else (40 if stress else 1), "sdkModuleUnmapped": True,
                        "cameraModuleUnmapped": True, "decoderReferences": codecs,
                  "ortDependencyResident": bool(k.GetModuleHandleW("onnxruntime.dll"))}
        record_file.write(json.dumps(record)+"\n")
    record_file.seek(0)
    records = [json.loads(line) for line in record_file]
    record_file.close()
    tail = records[args.warmup_cycles:]
    growth = tail[-1]["privateBytes"] - tail[0]["privateBytes"]
    block = max(1, len(tail)//4)
    first = sum(r["privateBytes"] for r in tail[:block])/block
    last = sum(r["privateBytes"] for r in tail[-block:])/block
    # 固定上限加头尾分组比较；保留全部原样本，不能仅报告最终工作集。
    assert growth <= 8*1024*1024 and last-first <= 4*1024*1024, (growth, last-first)
    assert max(r["processHandles"] for r in tail)-min(r["processHandles"] for r in tail) <= 8
    assert all(r["sdkWorkers"] == 0 for r in records)
    report = {"bindingOnlyControl": args.binding_only, "windowsCpuModuleLifecycle": records,
              "profileRegistryExercised": bool(args.model_dir and not args.binding_only),
              "plateau": {"warmupCycles": args.warmup_cycles, "measuredCycles": len(tail),
                          "privateGrowthBytes": growth, "lastMinusFirstBlockBytes": last-first,
                          "privateRangeBytes": max(r["privateBytes"] for r in tail)-min(r["privateBytes"] for r in tail),
                          "handleRange": max(r["processHandles"] for r in tail)-min(r["processHandles"] for r in tail)},
              "scope": "Windows CPU ORT; native dependencies may remain process-resident; finite reload test is not 7x24 or acceleration acceptance"}
    print(json.dumps(report))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--sample-dir", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path)
    parser.add_argument("--ffmpeg-dir", type=Path, required=True)
    parser.add_argument("--codec-sample-dir", type=Path, required=True)
    parser.add_argument("--codec-reference-dir", type=Path, help="可选的预生成解码参考，避免测试机依赖FFmpeg CLI")
    parser.add_argument("--reload-cycles", type=int, default=100)
    parser.add_argument("--warmup-cycles", type=int, default=10)
    parser.add_argument("--binding-only", action="store_true", help="仅装载绑定的对照组，不作为功能验收")
    parser.add_argument("--child", action="store_true")
    args = parser.parse_args()
    assert 3 <= args.warmup_cycles < args.reload_cycles <= 1000
    if args.child:
        child(args)
    else:
        command = [sys.executable, str(Path(__file__).resolve()), *sys.argv[1:], "--child"]
        result = subprocess.run(command, capture_output=True, text=True, timeout=180)
        if result.returncode:
            print(result.stdout)
            print(result.stderr, file=sys.stderr)
            raise SystemExit(f"module lifecycle child failed: {result.returncode}")
        print(result.stdout.strip())
