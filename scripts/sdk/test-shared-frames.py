"""真实Windows DLL/跨进程合成帧验证；不访问相机、服务或业务数据库。"""
import argparse
import ctypes as c
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdk" if (ROOT / "sdk/bindings").is_dir() else ROOT
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a


def checked(code, expected=0):
    assert code == expected, (code, expected)


def init(lib, camera=True):
    p = a.amc_init_params_t() if camera else a.amv_init_params_t()
    p.struct_size = c.sizeof(p)
    p.abi_version = a.AM_ABI_VERSION
    p.feature_flags = a.AM_INIT_SHARED_FRAMES
    ctx = c.POINTER(a.amc_context_t if camera else a.amv_context_t)()
    checked((lib.amc_init if camera else lib.amv_init)(c.byref(p), c.byref(ctx)))
    return ctx


def close(lib, obj, camera, pool=False):
    state = c.c_uint32()
    func = (lib.amc_shared_pool_close if pool else lib.amc_shutdown) if camera else (
        lib.amv_shared_pool_detach if pool else lib.amv_shutdown)
    checked(func(obj, 100, c.byref(state)))
    assert state.value == a.AM_LIFECYCLE_CLOSED, state.value
    checked((lib.amc_object_destroy if camera else lib.amv_object_destroy)(obj))


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def create(lib, ctx):
    cfg = sized(a.am_shared_pool_config_t)
    cfg.slot_count, cfg.max_frame_bytes, cfg.total_bytes = 1, 128, 128
    cfg.max_consumers, cfg.max_outstanding_borrows = 4, 8
    desc = sized(a.am_shared_pool_attach_desc_t)
    pool = c.POINTER(a.amc_shared_pool_t)()
    checked(lib.amc_shared_pool_create(ctx, c.byref(cfg), c.byref(desc), c.byref(pool)))
    return pool, desc


def attach(lib, ctx, desc):
    pool = c.POINTER(a.amv_shared_pool_t)()
    checked(lib.amv_shared_pool_attach(ctx, c.byref(desc), c.byref(pool)))
    return pool


def sample(format_id=1):
    data = (c.c_ubyte * 96)(*range(96))
    frame = sized(a.am_frame_view_t)
    frame.width, frame.height, frame.format = 4, 4, format_id
    frame.plane_count = 2 if format_id == 3 else 1
    frame.planes[0] = c.cast(data, c.POINTER(c.c_ubyte))
    frame.strides[0] = 16 if format_id == 0 else (4 if format_id == 3 else 12)
    frame.plane_size_bytes[0] = frame.strides[0] * 4
    if format_id == 3:
        frame.planes[1] = c.cast(c.byref(data, 16), c.POINTER(c.c_ubyte))
        frame.strides[1], frame.plane_size_bytes[1] = 4, 8
        frame.color_matrix, frame.color_range = 1, 1
    assoc = sized(a.am_frame_assoc_t)
    assoc.source_id, assoc.frame_id = 100, 999999999999999999
    assoc.source_generation, assoc.config_generation = 8, 12
    assoc.valid, assoc.clock_mask, assoc.clock_quality = 31, 7, 2
    assoc.pts, assoc.time_base_num, assoc.time_base_den = 91919, 1, 90000
    assoc.captured_at_ms, assoc.received_at_ms = 12345, 12350
    pixel = sized(a.am_pixel_view_t)
    pixel.magic, pixel.view, pixel.assoc = a.AM_PIXEL_VIEW_MAGIC, c.pointer(frame), c.pointer(assoc)
    return data, frame, assoc, pixel


def publish(lib, pool, pixel, expected=0):
    value = c.c_uint64(777)
    checked(lib.amc_shared_pool_publish_view(pool, c.byref(pixel), c.byref(value)), expected)
    assert bool(value.value) == (expected == 0), value.value
    return value.value


def borrow(lib, pool, publication=0):
    view = sized(a.am_shared_frame_view_t)
    checked(lib.amv_shared_pool_borrow(pool, publication, 0, c.byref(view)))
    return view


def stats(lib, pool):
    value = sized(a.am_shared_pool_stats_t)
    checked(lib.amc_shared_pool_get_stats(pool, c.byref(value)))
    return value


def desc_json(desc):
    return {key: (list(value) if key == "identity_tag" else value)
            for key, _ in desc._fields_ for value in [getattr(desc, key)]}


def desc_from(data):
    desc = sized(a.am_shared_pool_attach_desc_t)
    for key, value in data.items():
        if key == "identity_tag":
            desc.identity_tag[:] = value
        else:
            setattr(desc, key, value)
    return desc


def child(script_args, payload):
    process = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), *script_args],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               text=True, encoding="utf-8")
    process.stdin.write(json.dumps(payload) + "\n")
    process.stdin.flush()
    return process


def child_body(args, camera, vision):
    message = json.loads(sys.stdin.readline())
    if args.child == "consumer":
        ctx = init(vision, False)
        pool = attach(vision, ctx, desc_from(message))
        view = borrow(vision, pool)
        print(json.dumps({"borrow": view.borrow_id, "byte": c.string_at(view.plane_data[0], 1)[0]}), flush=True)
        sys.stdin.readline()
        checked(vision.amv_shared_pool_return(pool, c.byref(view)))
        close(vision, pool, False, True)
        close(vision, ctx, False)
    elif args.child == "producer":
        ctx = init(camera)
        pool, desc = create(camera, ctx)
        data, frame, assoc, pixel = sample()
        publish(camera, pool, pixel)
        print(json.dumps(desc_json(desc)), flush=True)
        sys.stdin.readline()
        # 模拟已发布并有活消费者时拥有者异常退出。
        os._exit(0)
    elif args.child == "abandoned":
        kernel = c.WinDLL("kernel32", use_last_error=True)
        kernel.OpenMutexW.argtypes = [c.c_uint32, c.c_int, c.c_wchar_p]
        kernel.OpenMutexW.restype = c.c_void_p
        kernel.WaitForSingleObject.argtypes = [c.c_void_p, c.c_uint32]
        name = "Local\\amrt-sfp-v1-" + bytes(message["identity_tag"]).hex() + "-lock"
        handle = kernel.OpenMutexW(0x100001, 0, name)
        assert handle and kernel.WaitForSingleObject(handle, 1000) == 0
        print("locked", flush=True)
        os._exit(0)
    elif args.child == "fault-test":
        ctx = init(camera)
        pool, desc = create(camera, ctx)
        process = child(["--dll-dir", args.dll_dir, "--child", "abandoned"], desc_json(desc))
        try:
            assert process.stdout.readline().strip() == "locked"
            process.wait(timeout=5)
            data, frame, assoc, pixel = sample()
            publish(camera, pool, pixel, a.AM_ERR_SDK_INTERNAL_ERROR)
            publish(camera, pool, pixel, a.AM_ERR_SDK_INTERNAL_ERROR)
            state = c.c_uint32()
            checked(camera.amc_shared_pool_close(pool, 0, c.byref(state)))
            assert state.value == a.AM_LIFECYCLE_CLOSING
            print(json.dumps({"abandonedRejectedReuse": True, "faultClose": "CLOSING"}), flush=True)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)


def readonly_protection(pointer):
    class MBI(c.Structure):
        _fields_ = [("base", c.c_void_p), ("allocation", c.c_void_p), ("allocationProtect", c.c_uint32),
                    ("partition", c.c_uint16), ("size", c.c_size_t), ("state", c.c_uint32),
                    ("protect", c.c_uint32), ("type", c.c_uint32)]
    kernel = c.WinDLL("kernel32", use_last_error=True)
    kernel.VirtualQuery.argtypes = [c.c_void_p, c.POINTER(MBI), c.c_size_t]
    kernel.VirtualQuery.restype = c.c_size_t
    value = MBI()
    assert kernel.VirtualQuery(pointer, c.byref(value), c.sizeof(value)) == c.sizeof(value)
    assert value.protect == 2, value.protect  # PAGE_READONLY，未用破坏性写入验证。


def handle_count():
    kernel = c.WinDLL("kernel32")
    kernel.GetCurrentProcess.restype = c.c_void_p
    kernel.GetProcessHandleCount.argtypes = [c.c_void_p, c.POINTER(c.c_uint32)]
    value = c.c_uint32()
    assert kernel.GetProcessHandleCount(kernel.GetCurrentProcess(), c.byref(value))
    return value.value


def run(args, camera, vision):
    result = {}
    for fmt in range(4):
        ctx, vctx = init(camera), init(vision, False)
        pool, desc = create(camera, ctx)
        consumer = attach(vision, vctx, desc)
        data, frame, assoc, pixel = sample(fmt)
        publication = publish(camera, pool, pixel)
        view = borrow(vision, consumer, publication)
        readonly_protection(view.plane_data[0])
        for plane in range(frame.plane_count):
            assert c.string_at(view.plane_data[plane], frame.plane_size_bytes[plane]) == c.string_at(
                frame.planes[plane], frame.plane_size_bytes[plane])
        bridge = sized(a.am_pixel_view_t)
        checked(vision.amv_shared_frame_as_pixel_view(consumer, c.byref(view), c.byref(bridge)))
        assert bridge.assoc.contents.pts == assoc.pts
        assert bridge.assoc.contents.frame_id == assoc.frame_id
        assert bridge.view.contents.color_matrix == frame.color_matrix
        bridge.retain_owner(bridge.owner_handle)
        checked(vision.amv_shared_pool_return(consumer, c.byref(view)))
        publish(camera, pool, pixel, a.AM_ERR_SDK_QUEUE_FULL)
        bridge.release_owner(bridge.owner_handle)
        publish(camera, pool, pixel)
        close(vision, consumer, False, True)
        close(camera, pool, True, True)
        close(vision, vctx, False)
        close(camera, ctx, True)
    result["pixelFormatsByteExact"] = ["RGBA8", "RGB8", "BGR8", "NV12"]
    result["consumerPageProtection"] = "PAGE_READONLY"
    baseline = handle_count()
    for _ in range(20):
        ctx, vctx = init(camera), init(vision, False)
        pool, desc = create(camera, ctx)
        con = attach(vision, vctx, desc)
        close(vision, con, False, True)
        close(camera, pool, True, True)
        close(vision, vctx, False)
        close(camera, ctx, True)
    assert handle_count() <= baseline + 1, (baseline, handle_count())
    result["handleRecovery"] = {"before": baseline, "after20Cycles": handle_count()}

    ctx = init(camera)
    pool, desc = create(camera, ctx)
    data, frame, assoc, pixel = sample()
    publish(camera, pool, pixel)
    process = child(["--dll-dir", args.dll_dir, "--child", "consumer"], desc_json(desc))
    try:
        assert json.loads(process.stdout.readline())["byte"] == 0
        assert stats(camera, pool).outstanding_borrows == 1  # 此时缓存已核验的真实进程对象。
        time.sleep(.03)
        publish(camera, pool, pixel, a.AM_ERR_SDK_QUEUE_FULL)  # 活读者停顿不能强制回收。
        process.kill()
        process.wait(timeout=5)
        assert stats(camera, pool).outstanding_borrows == 0
        assert publish(camera, pool, pixel) == 2
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=5)
    close(camera, pool, True, True)
    close(camera, ctx, True)
    result["verifiedConsumerExitReclaimed"] = True
    result["aliveSlowConsumerPreserved"] = True

    process = child(["--dll-dir", args.dll_dir, "--child", "producer"], {})
    try:
        desc = desc_from(json.loads(process.stdout.readline()))
        vctx = init(vision, False)
        con = attach(vision, vctx, desc)
        view = borrow(vision, con)
        process.stdin.write("exit\n")
        process.stdin.flush()
        process.wait(timeout=5)
        assert c.string_at(view.plane_data[0], 4) == bytes(range(4))
        next_view = sized(a.am_shared_frame_view_t)
        checked(vision.amv_shared_pool_borrow(con, 0, 0, c.byref(next_view)), a.AM_ERR_SDK_HANDLE_CLOSED)
        checked(vision.amv_shared_pool_return(con, c.byref(view)))
        close(vision, con, False, True)
        close(vision, vctx, False)
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=5)
    result["ownerExitKeepsExistingView"] = True

    # 故障池保持CLOSING属于安全退化；单独子进程承载本用例，退出后OS回收。
    process = child(["--dll-dir", args.dll_dir, "--child", "fault-test"], {})
    try:
        result.update(json.loads(process.stdout.readline()))
        process.wait(timeout=5)
        assert process.returncode == 0, process.stderr.read()
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=5)
    result["scope"] = "synthetic CPU shared-frame substrate; real decoding/inference tested by separate scripts"
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", required=True)
    parser.add_argument("--child", choices=["consumer", "producer", "abandoned", "fault-test"])
    args = parser.parse_args()
    directory = Path(args.dll_dir).resolve()
    dll_directory = os.add_dll_directory(str(directory))
    camera = c.CDLL(str(directory / "am_camera.dll"))
    vision = c.CDLL(str(directory / "am_vision.dll"))
    a.bind(camera, "amc_")
    a.bind(vision, "amv_")
    if args.child:
        child_body(args, camera, vision)
    else:
        run(args, camera, vision)
