"""实际推理DLL与共享帧所有权验证；无需服务、相机或业务库。"""
import argparse
import ctypes as c
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
sys.dont_write_bytecode = True
import threading
import time
import zlib
import struct
import subprocess
from concurrent.futures import ThreadPoolExecutor

ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdk" if (ROOT / "sdk/bindings").is_dir() else ROOT
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a
spec = importlib.util.spec_from_file_location("shared_test", Path(__file__).with_name("test-shared-frames.py"))
shared = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shared)
size, check = shared.sized, shared.checked


def initialize(vision, directory):
    p = size(a.amv_init_params_t)
    p.abi_version, p.feature_flags = a.AM_ABI_VERSION, a.AM_INIT_SHARED_FRAMES
    p.infer_threads = 2
    dirs = (c.c_char_p * 1)(str(directory.resolve()).encode())
    p.runtime_dirs_utf8, p.runtime_dir_count = dirs, 1
    ctx = c.POINTER(a.amv_context_t)()
    check(vision.amv_init(c.byref(p), c.byref(ctx)))
    return ctx


def load(vision, ctx, path, profile, memory=False):
    handle = c.POINTER(a.amv_model_t)()
    p = size(a.am_model_memory_desc_t if memory else a.am_model_desc_t)
    p.profile_id = profile.encode()
    if memory:
        raw = path.read_bytes()
        data = (c.c_ubyte * len(raw)).from_buffer_copy(raw)
        p.bytes, p.size = data, len(raw)
        check(vision.amv_model_load_memory(ctx, c.byref(p), c.byref(handle)))
        c.memset(data, 0, len(data))
    else:
        p.path_utf8 = str(path.resolve()).encode()
        check(vision.amv_model_load(ctx, c.byref(p), c.byref(handle)))
    info = size(a.am_model_info_t)
    check(vision.amv_model_get_info(handle, c.byref(info)))
    assert info.execution.actual == a.AM_BACKEND_CPU
    assert info.execution.subgraph_cpu_nodes == 0
    assert info.input_count == 1 and info.inputs[0].name == b"images"
    return handle, info.identity.model_uid


def close_model(vision, model):
    state = c.c_uint32()
    deadline = time.monotonic()+5
    while True:
        check(vision.amv_model_close(model, 100, c.byref(state)))
        if state.value == a.AM_LIFECYCLE_CLOSED:
            break
        assert time.monotonic() < deadline, state.value
    check(vision.amv_object_destroy(model))


def png():
    def chunk(kind, content):
        return struct.pack(">I", len(content))+kind+content+struct.pack(">I", zlib.crc32(kind+content))
    return b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR", struct.pack(">IIBBBBB", 4, 4, 8, 2, 0, 0, 0))+chunk(b"IDAT", zlib.compress(b"".join(b"\x00"+bytes(range(y*12,y*12+12)) for y in range(4))))+chunk(b"IEND", b"")


def outputs(pointer):
    result = pointer.contents
    assert result.infer_end_ms >= result.infer_start_ms > 0
    return [(result.raw_outputs[i].name.decode(),
             list(result.raw_outputs[i].shape[:result.raw_outputs[i].shape_len]),
             c.string_at(result.raw_outputs[i].data, result.raw_outputs[i].data_size))
            for i in range(result.raw_output_count)]


def sync(vision, ctx, model, image, deadline=0):
    params = size(a.am_infer_params_t)
    params.confidence_threshold, params.sync_deadline_ms = 0.25, deadline
    result, rid = c.POINTER(a.amv_result_t)(), c.c_uint64()
    code = vision.amv_infer_image_sync(model, c.byref(image), c.byref(params), c.byref(result), c.byref(rid))
    if code == a.AM_ERR_SDK_DEADLINE_EXCEEDED:
        assert rid.value and not result
        wait(vision, ctx, rid.value)
        check(vision.amv_result_fetch(ctx, rid.value, c.byref(result)))
    else:
        check(code)
    return result, rid.value, code


def wait(vision, ctx, rid, expected=a.AM_REQ_DONE):
    deadline = time.monotonic()+10
    while True:
        snapshot = c.POINTER(a.am_request_snapshot_t)()
        check(vision.amv_request_query(ctx, rid, c.byref(snapshot)))
        state, error = snapshot.contents.state, snapshot.contents.error.contents.code
        check(vision.amv_request_snapshot_release(snapshot))
        if state >= a.AM_REQ_DONE:
            assert state == expected, (rid, state, error)
            return
        assert time.monotonic() < deadline, (rid, state)
        time.sleep(0.002)


def basic(vision, ctx, directory):
    profiles = json.loads((ROOT / "crates/vision/resources/profiles.json").read_text(encoding="utf-8"))["profiles"]
    files = ["yolo26n-pose.onnx", "yolo11n.onnx", "yolo11n-cls.onnx", "yolo11n-seg.onnx", "yolo11n-obb.onnx"]
    records = []
    data = (c.c_ubyte * len(png())).from_buffer_copy(png())
    image = size(a.am_image_input_t)
    image.kind, image.encoded, image.encoded_size = a.AM_IMG_PNG, data, len(data)
    for index, (profile, filename) in enumerate(zip(profiles, files)):
        path = directory / filename
        assert hashlib.sha256(path.read_bytes()).hexdigest() == profile["sampleSha256"]
        model, uid = load(vision, ctx, path, profile["id"], memory=index % 2 == 1)
        result, rid, code = sync(vision, ctx, model, image, deadline=1)
        assert result.contents.task == index+1
        assert result.contents.model.model_uid == uid
        raw = outputs(result)
        if index == 0:
            # 同步超时返回即可复用输入，不等后台工作执行完再释放原缓冲。
            p = size(a.am_infer_params_t)
            p.confidence_threshold, p.sync_deadline_ms = 0.25, 1
            later, lid = c.POINTER(a.amv_result_t)(), c.c_uint64()
            code2 = vision.amv_infer_image_sync(model, c.byref(image), c.byref(p), c.byref(later), c.byref(lid))
            assert code2 == a.AM_ERR_SDK_DEADLINE_EXCEEDED
            c.memset(data, 0, len(data))
            wait(vision, ctx, lid.value)
            check(vision.amv_result_fetch(ctx, lid.value, c.byref(later)))
            assert outputs(later) == raw
            check(vision.amv_result_ack(ctx, lid.value))
            check(vision.amv_result_release(later))
            c.memmove(data, png(), len(data))
        again = c.POINTER(a.amv_result_t)()
        check(vision.amv_result_fetch(ctx, rid, c.byref(again)))
        assert c.addressof(result.contents) == c.addressof(again.contents)
        snapshot = c.POINTER(a.am_request_snapshot_t)()
        check(vision.amv_request_query(ctx, rid, c.byref(snapshot)))
        check(vision.amv_result_ack(ctx, rid))
        assert snapshot.contents.state == a.AM_REQ_DONE
        check(vision.amv_result_fetch(ctx, rid, c.byref(again)), a.AM_ERR_SDK_RESULT_EXPIRED)
        assert outputs(result) == raw
        # fetch移交的第二个引用即使ack后也必须显式释放。
        check(vision.amv_result_release(result))
        check(vision.amv_result_release(result))
        check(vision.amv_request_snapshot_release(snapshot))
        close_model(vision, model)
        records.append({"profile": profile["id"], "outputs": len(raw), "syncTimedOutThenFetched": code != 0})
    return records


def pressure(vision, ctx, directory):
    model, _ = load(vision, ctx, directory/"yolo11n-cls.onnx", "classify-yolo11n-prob-v1")
    data, frame, assoc, pixel = shared.sample()
    gate, entered, counter_lock = threading.Event(), threading.Event(), threading.Lock()
    counts = {"retain": 0, "release": 0}
    retain_type, release_type = type(pixel.retain_owner), type(pixel.release_owner)

    @retain_type
    def retain(owner):
        with counter_lock:
            counts["retain"] += 1

    @release_type
    def release(owner):
        with counter_lock:
            first = counts["release"] == 0
            counts["release"] += 1
        if first:
            entered.set()
            gate.wait(10)

    pixel.owner_handle, pixel.retain_owner, pixel.release_owner = 1, retain, release
    image = size(a.am_image_input_t)
    image.kind, image.flags, image.view = a.AM_IMG_PIXEL_VIEW, a.AM_INPUT_BORROW_RETAINED, c.pointer(pixel)
    policy = size(a.am_realtime_policy_t)
    policy.max_inflight, policy.replace_oldest, policy.slot_count, policy.fair_round_robin = 2, 1, 1, 0
    check(vision.amv_set_realtime_policy(model, c.byref(policy)))
    ids, rejected = [], 0
    try:
        rid = c.c_uint64()
        check(vision.amv_infer_image_async(model, c.byref(image), None, c.byref(rid)))
        ids.append(rid.value)
        assert entered.wait(5)
        for _ in range(50):
            rid = c.c_uint64(999)
            code = vision.amv_infer_image_async(model, c.byref(image), None, c.byref(rid))
            if code == a.AM_ERR_SDK_QUEUE_FULL:
                assert rid.value == 0
                rejected += 1
            else:
                check(code)
                ids.append(rid.value)
        assert rejected > 0 and len(ids) > 2
        # 关闭不把正在release屏障中的输入当成已释放；pending替换保留单一终态。
        state = c.c_uint32()
        check(vision.amv_model_close(model, 0, c.byref(state)))
        assert state.value == a.AM_LIFECYCLE_CLOSING
        gate.set()
        for index, rid in enumerate(ids):
            wait(vision, ctx, rid, a.AM_REQ_CANCELED if index in [0, len(ids)-1] else a.AM_REQ_REPLACED)
            check(vision.amv_result_ack(ctx, rid))
        close_model(vision, model)
        assert counts["retain"] == counts["release"]
    finally:
        gate.set()
    return {"accepted": len(ids), "rejected": rejected, "balancedOwners": counts}


def expiry_and_fetch_race(vision, directory, runtime):
    ctx = initialize(vision, runtime)
    model, _ = load(vision, ctx, directory/"yolo11n-cls.onnx", "classify-yolo11n-prob-v1")
    data, frame, assoc, pixel = shared.sample()
    image = size(a.am_image_input_t)
    image.kind, image.view = a.AM_IMG_PIXEL_VIEW, c.pointer(pixel)
    result, rid, _ = sync(vision, ctx, model, image)
    reference = outputs(result)
    check(vision.amv_result_release(result))

    def fetch_many(_):
        for _ in range(100):
            view = c.POINTER(a.amv_result_t)()
            check(vision.amv_result_fetch(ctx, rid, c.byref(view)))
            assert outputs(view) == reference
            check(vision.amv_result_release(view))

    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(fetch_many, range(4)))
    check(vision.amv_result_ack(ctx, rid))
    policy = size(a.am_queue_policy_t)
    check(vision.amv_get_queue_policy(ctx, a.AM_Q_RESULTS, c.byref(policy)))
    policy.ttl_ms = 40
    check(vision.amv_set_queue_policy(ctx, a.AM_Q_RESULTS, c.byref(policy)))
    held, rid, _ = sync(vision, ctx, model, image)
    time.sleep(0.15)
    snapshot = c.POINTER(a.am_request_snapshot_t)()
    check(vision.amv_request_query(ctx, rid, c.byref(snapshot)), a.AM_ERR_SDK_RESULT_EXPIRED)
    assert outputs(held) == reference
    close_model(vision, model)
    state = c.c_uint32()
    check(vision.amv_shutdown(ctx, 0, c.byref(state)))
    assert state.value == a.AM_LIFECYCLE_CLOSING
    check(vision.amv_result_release(held))
    shared.close(vision, ctx, False)
    return {"fetchReleaseRaces": 400, "ttlExternalHold": True}


def rejected_inputs(vision, runtime, directory):
    ctx = initialize(vision, runtime)
    model, _ = load(vision, ctx, directory/"yolo11n-cls.onnx", "classify-yolo11n-prob-v1")
    data, frame, assoc, pixel = shared.sample()
    image = size(a.am_image_input_t)
    image.kind, image.view = a.AM_IMG_PIXEL_VIEW, c.pointer(pixel)
    frame.plane_size_bytes[0] = 1
    rid = c.c_uint64(123)
    check(vision.amv_infer_image_async(model, c.byref(image), None, c.byref(rid)), a.AM_ERR_VISION_IMAGE_VIEW_INVALID)
    assert rid.value == 0
    frame.plane_size_bytes[0] = 48
    limits = size(a.am_limits_t)
    check(vision.amv_get_limits(ctx, c.byref(limits)))
    limits.external_hold_max_bytes = 8000
    check(vision.amv_set_limits(ctx, c.byref(limits)))
    first, first_id, _ = sync(vision, ctx, model, image)
    second, second_id = c.POINTER(a.amv_result_t)(), c.c_uint64()
    check(vision.amv_infer_image_sync(model, c.byref(image), None, c.byref(second), c.byref(second_id)),
          a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
    assert second_id.value and not second
    duplicate = c.POINTER(a.amv_result_t)()
    check(vision.amv_result_fetch(ctx, first_id, c.byref(duplicate)))
    check(vision.amv_result_release(duplicate))
    check(vision.amv_result_release(first))
    check(vision.amv_result_fetch(ctx, second_id.value, c.byref(second)))
    check(vision.amv_result_release(second))
    check(vision.amv_result_ack(ctx, first_id))
    check(vision.amv_result_ack(ctx, second_id.value))
    policy = size(a.am_queue_policy_t)
    check(vision.amv_get_queue_policy(ctx, a.AM_Q_REALTIME_INFER, c.byref(policy)))
    policy.max_items = 0
    check(vision.amv_set_queue_policy(ctx, a.AM_Q_REALTIME_INFER, c.byref(policy)))
    check(vision.amv_infer_image_async(model, c.byref(image), None, c.byref(rid)), a.AM_ERR_SDK_QUEUE_FULL)
    assert rid.value == 0
    snapshot = c.POINTER(a.am_request_snapshot_t)()
    check(vision.amv_request_query(ctx, 123, c.byref(snapshot)), a.AM_ERR_SDK_REQUEST_UNKNOWN)
    close_model(vision, model)
    shared.close(vision, ctx, False)
    return {"badPlaneRejected": True, "zeroQueueRejectsNew": True, "unacceptedIdIsZero": True,
            "externalHoldLimit": True, "duplicateFetchChargedOnce": True}


def child_inference(args):
    message = json.loads(sys.stdin.readline())
    # 子进程只加载推理DLL；共享凭据不传本地句柄或指针。
    vision = c.CDLL(str((args.dll_dir/"am_vision.dll").resolve()))
    a.bind(vision, "amv_")
    ctx = initialize(vision, args.runtime_dir)
    models = [load(vision, ctx, args.model_dir/"yolo26n-pose.onnx", "pose-yolo26-e2e-coco17-v1")[0],
              load(vision, ctx, args.model_dir/"yolo11n-cls.onnx", "classify-yolo11n-prob-v1")[0]]
    data, frame, assoc, pixel = shared.sample()
    direct = size(a.am_image_input_t)
    direct.kind, direct.view = a.AM_IMG_PIXEL_VIEW, c.pointer(pixel)
    reference = []
    for model in models:
        baseline, rid, _ = sync(vision, ctx, model, direct)
        reference.append(outputs(baseline))
        check(vision.amv_result_ack(ctx, rid))
        check(vision.amv_result_release(baseline))
    pool = shared.attach(vision, ctx, shared.desc_from(message["attach"]))
    view = shared.borrow(vision, pool, message["publication"])
    bridge = size(a.am_pixel_view_t)
    check(vision.amv_shared_frame_as_pixel_view(pool, c.byref(view), c.byref(bridge)))
    image = size(a.am_image_input_t)
    image.kind, image.flags, image.view = a.AM_IMG_PIXEL_VIEW, a.AM_INPUT_BORROW_RETAINED, c.pointer(bridge)
    ids = []
    for model in models:
        rid = c.c_uint64()
        check(vision.amv_infer_image_async(model, c.byref(image), None, c.byref(rid)))
        ids.append(rid.value)
    check(vision.amv_shared_pool_return(pool, c.byref(view)))
    c.memset(c.byref(view), 0, c.sizeof(view))
    uids, hashes = [], []
    for index, (model, rid) in enumerate(zip(models, ids)):
        wait(vision, ctx, rid)
        result = c.POINTER(a.amv_result_t)()
        check(vision.amv_result_fetch(ctx, rid, c.byref(result)))
        assert outputs(result) == reference[index]
        assert result.contents.frame.contents.frame_id == assoc.frame_id
        assert result.contents.frame.contents.source_id == assoc.source_id
        uids.append(result.contents.model.model_uid)
        hashes.append(hashlib.sha256(reference[index][0][2]).hexdigest())
        check(vision.amv_result_ack(ctx, rid))
        check(vision.amv_result_release(result))
        close_model(vision, model)
    assert uids[0] != uids[1]
    shared.close(vision, pool, False, True)
    shared.close(vision, ctx, False)
    kernel = c.WinDLL("kernel32")
    kernel.GetModuleHandleW.argtypes, kernel.GetModuleHandleW.restype = [c.c_wchar_p], c.c_void_p
    assert not kernel.GetModuleHandleW("am_camera.dll")
    print(json.dumps({"models": 2, "sameFrame": assoc.frame_id, "rawMatchesOrdinary": True,
                      "onlyVisionDllLoaded": True, "rawSha256": hashes}))


def cross_process(camera, args):
    ctx = shared.init(camera)
    pool, attach = shared.create(camera, ctx)
    data, frame, assoc, pixel = shared.sample()
    publication = shared.publish(camera, pool, pixel)
    command = [sys.executable, str(Path(__file__).resolve()), "--child", "--dll-dir", str(args.dll_dir),
               "--runtime-dir", str(args.runtime_dir), "--model-dir", str(args.model_dir)]
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, encoding="utf-8")
    try:
        stdout, stderr = process.communicate(json.dumps({"attach": shared.desc_json(attach),
                                                        "publication": publication})+"\n", timeout=20)
        assert process.returncode == 0, (stdout, stderr)
        result = json.loads(stdout)
        assert result["sameFrame"] == assoc.frame_id
        assert shared.stats(camera, pool).outstanding_borrows == 0
        shared.publish(camera, pool, pixel)
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
    shared.close(camera, pool, True, True)
    shared.close(camera, ctx, True)
    return result


def shared_inference(camera, vision, ctx, directory, cancel=False, second_pose=False):
    camctx = shared.init(camera)
    pool, desc = shared.create(camera, camctx)
    consumer = shared.attach(vision, ctx, desc)
    data, frame, assoc, pixel = shared.sample()
    publication = shared.publish(camera, pool, pixel)
    view = shared.borrow(vision, consumer, publication)
    bridge = size(a.am_pixel_view_t)
    check(vision.amv_shared_frame_as_pixel_view(consumer, c.byref(view), c.byref(bridge)))
    image = size(a.am_image_input_t)
    image.kind, image.flags, image.view = a.AM_IMG_PIXEL_VIEW, a.AM_INPUT_BORROW_RETAINED, c.pointer(bridge)
    original_release = type(bridge.release_owner)(c.cast(bridge.release_owner, c.c_void_p).value)
    gate, entered = threading.Event(), [threading.Event(), threading.Event()]
    release_count = []
    callback_type = type(bridge.release_owner)

    @callback_type
    def release(owner):
        index = len(release_count)
        release_count.append(owner)
        entered[index].set()
        gate.wait(10)
        original_release(owner)

    bridge.release_owner = release
    second_file = "yolo26n-pose.onnx" if second_pose else "yolo11n-cls.onnx"
    second_profile = "pose-yolo26-e2e-coco17-v1" if second_pose else "classify-yolo11n-prob-v1"
    models = [load(vision, ctx, directory/"yolo26n-pose.onnx", "pose-yolo26-e2e-coco17-v1")[0],
              load(vision, ctx, directory/second_file, second_profile)[0]]
    requests = []
    expected = []
    try:
        direct = size(a.am_image_input_t)
        direct.kind, direct.view = a.AM_IMG_PIXEL_VIEW, c.pointer(pixel)
        for model in models:
            baseline, bid, _ = sync(vision, ctx, model, direct)
            expected.append(outputs(baseline))
            check(vision.amv_result_ack(ctx, bid))
            check(vision.amv_result_release(baseline))
        for model in models:
            rid = c.c_uint64()
            check(vision.amv_infer_image_async(model, c.byref(image), None, c.byref(rid)))
            requests.append(rid.value)
        check(vision.amv_shared_pool_return(consumer, c.byref(view)))
        c.memset(c.byref(view), 0, c.sizeof(view))
        # 接受后立即return；两个真实ORT工作走到release时，释放屏障仍未通过。
        assert entered[0].wait(5) and entered[1].wait(5)
        retained = c.string_at(bridge.view.contents.planes[0], frame.plane_size_bytes[0])
        c.memset(data, 0xA5, len(data))
        shared.publish(camera, pool, pixel, a.AM_ERR_SDK_QUEUE_FULL)
        assert c.string_at(bridge.view.contents.planes[0], frame.plane_size_bytes[0]) == retained
        state = c.c_uint32()
        check(vision.amv_shared_pool_detach(consumer, 0, c.byref(state)))
        assert state.value == a.AM_LIFECYCLE_CLOSING
        if cancel:
            for model in models:
                check(vision.amv_model_close(model, 0, c.byref(state)))
                assert state.value == a.AM_LIFECYCLE_CLOSING
        gate.set()
        for index, (model, rid) in enumerate(zip(models, requests)):
            wait(vision, ctx, rid, a.AM_REQ_CANCELED if cancel else a.AM_REQ_DONE)
            if not cancel:
                result = c.POINTER(a.amv_result_t)()
                check(vision.amv_result_fetch(ctx, rid, c.byref(result)))
                assert result.contents.frame.contents.frame_id == assoc.frame_id
                assert result.contents.frame.contents.source_generation == assoc.source_generation
                assert result.contents.frame.contents.pts == assoc.pts
                assert outputs(result) == expected[index]
                check(vision.amv_result_release(result))
            check(vision.amv_result_ack(ctx, rid))
            close_model(vision, model)
        shared.close(vision, consumer, False, True)
        assert shared.stats(camera, pool).outstanding_borrows == 0
        shared.publish(camera, pool, pixel)
    finally:
        gate.set()
    shared.close(camera, pool, True, True)
    shared.close(camera, camctx, True)
    return {"samePublicationTwoModels": True, "heldBytesUnchanged": True,
            "releaseCalls": len(release_count), "cancel": cancel}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--child", action="store_true")
    parser.add_argument("--shared-pose-only", action="store_true",
                        help="仅验证两个 Pose 模型实例的共享帧正常与关闭竞争")
    args = parser.parse_args()
    if args.child:
        child_inference(args)
        return
    camera = c.CDLL(str((args.dll_dir/"am_camera.dll").resolve()))
    vision = c.CDLL(str((args.dll_dir/"am_vision.dll").resolve()))
    a.bind(camera, "amc_")
    missing = a.bind(vision, "amv_")
    ctx = initialize(vision, args.runtime_dir)
    if args.shared_pose_only:
        try:
            result = [shared_inference(camera, vision, ctx, args.model_dir, cancel, second_pose=True)
                      for cancel in (False, True)]
            print(json.dumps(result, ensure_ascii=False))
        finally:
            shared.close(vision, ctx, False)
        return
    result = {"profiles": basic(vision, ctx, args.model_dir), "shared": []}
    for cancel in [False, True]:
        result["shared"].append(shared_inference(camera, vision, ctx, args.model_dir, cancel))
    result["pressure"] = pressure(vision, ctx, args.model_dir)
    shared.close(vision, ctx, False)
    result["expiry"] = expiry_and_fetch_race(vision, args.model_dir, args.runtime_dir)
    result["rejects"] = rejected_inputs(vision, args.runtime_dir, args.model_dir)
    result["crossProcess"] = cross_process(camera, args)
    result["actualVisionExports"] = sum(n.startswith("amv_") for n in a.FUNCTIONS)-len(missing)
    result["scope"] = "real CPU inference and shared retained inputs; final G0-G3 pending"
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
