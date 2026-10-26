"""真实DLL批量提交/逐项回执/原生计算与关闭屏障验证；仅合成输入。"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import struct
import sys
sys.dont_write_bytecode = True
import threading
import time

spec = importlib.util.spec_from_file_location("batch_metadata", Path(__file__).with_name("test-batch-metadata.py"))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
t, base, a, size, check = m.t, m.base, m.a, m.size, m.check


class Item:
    def __init__(self, index, slot=0):
        self.tensors = m.inputs(1)
        c.memmove(c.byref(self.tensors[0].storage, 1), struct.pack("<fff", index, index+1, index+2), 12)
        self.assoc = size(a.am_frame_assoc_t)
        self.assoc.source_id, self.assoc.frame_id, self.assoc.valid = 7, 1000+index, a.AM_ASSOC_SOURCE | a.AM_ASSOC_FRAME
        for v in self.tensors:
            v.view.assoc = c.pointer(self.assoc)
        self.array = (a.am_tensor_in_t*len(self.tensors))(*(v.view for v in self.tensors))
        self.value = size(a.am_batch_item_t)
        self.value.tensors, self.value.tensor_count, self.value.slot_key = self.array, len(self.array), slot

    def clear(self):
        for tensor in self.tensors:
            tensor.clear()
        c.memset(self.array, 0, c.sizeof(self.array))
        c.memset(c.byref(self.assoc), 0, c.sizeof(self.assoc))


def submit(vision, model, items, mode=a.AM_BATCH_AUTO, wait=0):
    array = (a.am_batch_item_t*len(items))(*(i.value for i in items))
    options = size(a.am_batch_options_t)
    options.batch_mode, options.wait_deadline_ms = mode, wait
    receipt = size(a.am_batch_receipt_t)
    check(vision.amv_infer_batch(model, len(array), array, c.byref(options), c.byref(receipt)))
    ids, codes = list(receipt.request_ids[:len(items)]), list(receipt.rejects[:len(items)])
    assert receipt.accepted_count == sum(bool(r) for r in ids)
    assert all(code == 0 for rid, code in zip(ids, codes) if rid)
    assert len({r for r in ids if r}) == receipt.accepted_count
    return receipt, ids, codes


def collect(vision, ctx, ids, indices, mean=None):
    for rid, index in zip(ids, indices):
        if not rid:
            continue
        base.wait(vision, ctx, rid)
        result = c.POINTER(a.amv_result_t)()
        check(vision.amv_result_fetch(ctx, rid, c.byref(result)))
        actual = t.output(result)
        floats = struct.unpack("<3f", actual["out_float"][2])
        expected = 0 if mean is None else index-mean
        assert all(abs(v-expected) < 0.00001 for v in floats), (index, floats, expected)
        assert result.contents.frame.contents.frame_id == 1000+index
        t.release(vision, ctx, result, rid)


def basic(vision, ctx, directory):
    cases = [("dynamic.onnx", a.AM_BATCH_NATIVE, 3), ("dynamic.onnx", a.AM_BATCH_AUTO, 3),
             ("fixed4.onnx", a.AM_BATCH_AUTO, 6), ("unproved.onnx", a.AM_BATCH_AUTO, 4),
             ("dynamic.onnx", a.AM_BATCH_SPLIT, 3)]
    records = []
    for filename, mode, count in cases:
        model = t.load(vision, ctx, directory/filename)
        items = [Item(i) for i in range(count)]
        receipt, ids, codes = submit(vision, model, items, mode)
        for item in items:
            item.clear()
        if filename == "fixed4.onnx":
            assert all(ids[:4]) and ids[4:] == [0, 0]
            assert codes[4:] == [a.AM_ERR_VISION_INSUFFICIENT_BATCH_REJECTED]*2
        else:
            assert all(ids) and codes == [0]*count
        native = filename != "unproved.onnx" and mode != a.AM_BATCH_SPLIT
        collect(vision, ctx, ids, range(count), (sum(i for i, rid in enumerate(ids) if rid)/receipt.accepted_count) if native else None)
        check(vision.amv_receipt_release(c.byref(receipt)))
        assert not receipt.request_ids and not receipt.rejects and receipt.accepted_count == 0
        check(vision.amv_receipt_release(c.byref(receipt)), a.AM_ERR_SDK_HANDLE_INVALID)
        base.close_model(vision, model)
        records.append({"file": filename, "mode": mode, "accepted": sum(bool(r) for r in ids)})
    return records


def partial_and_modes(vision, ctx, directory):
    model = t.load(vision, ctx, directory/"fixed4.onnx")
    items = [Item(i) for i in range(6)]
    items[1].array[0].flags = 0
    receipt, ids, codes = submit(vision, model, items)
    assert receipt.accepted_count == 4 and ids[1] == ids[5] == 0
    assert codes[1] == a.AM_ERR_VISION_IO_UNSUPPORTED and codes[5] == a.AM_ERR_VISION_INSUFFICIENT_BATCH_REJECTED
    collect(vision, ctx, ids, range(6), (0+2+3+4)/4)
    check(vision.amv_receipt_release(c.byref(receipt)))
    receipt, ids, codes = submit(vision, model, [Item(i) for i in range(4)], a.AM_BATCH_SPLIT)
    assert receipt.accepted_count == 0 and codes == [a.AM_ERR_VISION_BATCH_UNSUPPORTED]*4
    check(vision.amv_receipt_release(c.byref(receipt)))
    base.close_model(vision, model)
    model = t.load(vision, ctx, directory/"unproved.onnx")
    receipt, ids, codes = submit(vision, model, [Item(0)], a.AM_BATCH_NATIVE)
    assert ids == [0] and codes == [a.AM_ERR_VISION_BATCH_UNSUPPORTED]
    check(vision.amv_receipt_release(c.byref(receipt)))
    base.close_model(vision, model)
    return {"invalidItemDoesNotRollbackValidBatch": True, "fixedSplitRejected": True, "unprovedNativeRejected": True}


def sixty_four(vision, ctx, directory):
    model = t.load(vision, ctx, directory/"dynamic.onnx")
    refused, rids, rcodes = submit(vision, model, [Item(i) for i in range(64)])
    assert rids == [0]*64 and rcodes == [a.AM_ERR_SDK_QUEUE_FULL]*64
    check(vision.amv_receipt_release(c.byref(refused)))
    policy = size(a.am_realtime_policy_t)
    policy.max_inflight, policy.slot_count, policy.fair_round_robin = 16, 4, 1
    check(vision.amv_set_realtime_policy(model, c.byref(policy)))
    items = [Item(i, i//16) for i in range(64)]
    receipt, ids, codes = submit(vision, model, items)
    assert all(ids) and codes == [0]*64
    collect(vision, ctx, ids, range(64), 31.5)
    check(vision.amv_receipt_release(c.byref(receipt)))
    base.close_model(vision, model)
    return {"count": 64, "slots": 4, "nativeAcrossAllItems": True, "noSilentSplitOnPressure": True}


def receipt_close(vision, runtime, directory):
    ctx = base.initialize(vision, runtime)
    model = t.load(vision, ctx, directory/"dynamic.onnx")
    receipt, ids, codes = submit(vision, model, [Item(i) for i in range(3)])
    collect(vision, ctx, ids, range(3), 1)
    base.close_model(vision, model)
    state = c.c_uint32()
    check(vision.amv_shutdown(ctx, 0, c.byref(state)))
    assert state.value == a.AM_LIFECYCLE_CLOSING
    assert list(receipt.request_ids[:3]) == ids
    check(vision.amv_receipt_release(c.byref(receipt)))
    check(vision.amv_shutdown(ctx, 100, c.byref(state)))
    assert state.value == a.AM_LIFECYCLE_CLOSED
    snapshot = c.POINTER(a.am_request_snapshot_t)()
    check(vision.amv_request_query(ctx, ids[0], c.byref(snapshot)), a.AM_ERR_SDK_HANDLE_CLOSED)
    assert not snapshot
    check(vision.amv_object_destroy(ctx))
    return {"receiptKeepsContextClosing": True, "closedRejectsNewSnapshots": True}


class Gated:
    def __init__(self, retain_delay=0):
        self.data, self.frame, self.assoc, self.pixel = base.shared.sample(0)
        self.gate, self.entered = threading.Event(), threading.Event()
        self.counts = [0, 0]
        self.retain_delay = retain_delay
        retain_type, release_type = type(self.pixel.retain_owner), type(self.pixel.release_owner)
        @retain_type
        def retain(owner):
            self.counts[0] += 1
            if self.retain_delay:
                time.sleep(self.retain_delay)
        @release_type
        def release(owner):
            first = self.counts[1] == 0
            self.counts[1] += 1
            if first:
                self.entered.set()
                self.gate.wait(10)
        self.callbacks = [retain, release]
        self.pixel.owner_handle, self.pixel.retain_owner, self.pixel.release_owner = 1, retain, release
        self.preprocess = size(a.am_raw_preprocess_t)
        self.preprocess.target_w, self.preprocess.target_h, self.preprocess.resize = 2, 2, 1

    def item(self, retained=True, index=0):
        value = type("ImageItem", (), {})()
        value.image = size(a.am_image_input_t)
        value.image.kind, value.image.view = a.AM_IMG_PIXEL_VIEW, c.pointer(self.pixel)
        value.image.flags = a.AM_INPUT_BORROW_RETAINED if retained else 0
        value.assoc = size(a.am_frame_assoc_t)
        value.assoc.frame_id, value.assoc.valid = 700+index, a.AM_ASSOC_FRAME
        value.image.assoc = c.pointer(value.assoc)
        value.value = size(a.am_batch_item_t)
        value.value.image, value.value.raw_preprocess = c.pointer(value.image), c.pointer(self.preprocess)
        return value

    def start(self, vision, model):
        self.initial = self.item()
        params = size(a.am_infer_params_t)
        params.confidence_threshold, params.raw_preprocess = 0.25, c.pointer(self.preprocess)
        rid = c.c_uint64()
        check(vision.amv_infer_image_async(model, c.byref(self.initial.image), c.byref(params), c.byref(rid)))
        assert self.entered.wait(5)
        return rid.value


def native_cancel(vision, runtime, directory):
    ctx = base.initialize(vision, runtime)
    model = t.load(vision, ctx, directory/"image-dynamic.onnx")
    gated = Gated()
    try:
        receipt, ids, codes = submit(vision, model, [gated.item(index=i) for i in range(2)])
        assert all(ids) and codes == [0, 0] and gated.entered.wait(5)
        check(vision.amv_request_cancel(ctx, ids[0]))
        snapshot = c.POINTER(a.am_request_snapshot_t)()
        check(vision.amv_request_query(ctx, ids[0], c.byref(snapshot)))
        assert snapshot.contents.state == a.AM_REQ_RUNNING and not snapshot.contents.terminal
        check(vision.amv_request_snapshot_release(snapshot))
        gated.gate.set()
        base.wait(vision, ctx, ids[0], a.AM_REQ_CANCELED)
        base.wait(vision, ctx, ids[1])
        result = c.POINTER(a.amv_result_t)()
        check(vision.amv_result_fetch(ctx, ids[1], c.byref(result)))
        assert result.contents.transform.contents.valid and result.contents.frame.contents.frame_id == 701
        check(vision.amv_result_release(result))
        for rid in ids:
            check(vision.amv_result_ack(ctx, rid))
        check(vision.amv_receipt_release(c.byref(receipt)))
        base.close_model(vision, model)
        assert gated.counts == [2, 2]
        base.shared.close(vision, ctx, False)
    finally:
        gated.gate.set()
    return {"cancellationIsPerItem": True, "retainedInputs": gated.counts, "terminalAfterActualRelease": True}


def waiting(vision, runtime, directory):
    ctx = base.initialize(vision, runtime)
    model = t.load(vision, ctx, directory/"image-dynamic.onnx")
    policy = size(a.am_realtime_policy_t)
    policy.max_inflight, policy.slot_count, policy.replace_oldest = 2, 1, 1
    check(vision.amv_set_realtime_policy(model, c.byref(policy)))
    gated = Gated()
    try:
        initial = gated.start(vision, model)
        pending, ids, codes = submit(vision, model, [gated.item(False)], a.AM_BATCH_SPLIT)
        assert ids[0] and codes == [0]
        params = size(a.am_infer_params_t)
        params.confidence_threshold, params.raw_preprocess = 0.25, c.pointer(gated.preprocess)
        refused = c.c_uint64(777)
        plain = gated.item(False)
        check(vision.amv_infer_image_async(model, c.byref(plain.image), c.byref(params), c.byref(refused)), a.AM_ERR_SDK_QUEUE_FULL)
        assert refused.value == 0
        start = time.monotonic()
        timed, tids, tcodes = submit(vision, model, [gated.item(False)], a.AM_BATCH_SPLIT, 70)
        elapsed = time.monotonic()-start
        assert tids == [0] and tcodes == [a.AM_ERR_SDK_DEADLINE_EXCEEDED] and 0.05 <= elapsed < 0.8
        check(vision.amv_receipt_release(c.byref(timed)))
        accepted, error = [], []
        def wait_call():
            try:
                accepted.append(submit(vision, model, [gated.item(False)], a.AM_BATCH_SPLIT, 500))
            except BaseException as exc:
                error.append(str(exc))
        worker = threading.Thread(target=wait_call)
        worker.start()
        time.sleep(0.03)
        assert worker.is_alive()
        check(vision.amv_request_cancel(ctx, ids[0]))
        gated.gate.set()
        worker.join(5)
        assert not worker.is_alive() and not error and len(accepted) == 1
        later, lids, lcodes = accepted[0]
        assert lids[0] and lcodes == [0]
        base.wait(vision, ctx, initial)
        base.wait(vision, ctx, ids[0], a.AM_REQ_CANCELED)
        base.wait(vision, ctx, lids[0])
        for rid in [initial, ids[0], lids[0]]:
            check(vision.amv_result_ack(ctx, rid))
        for r in [pending, later]:
            check(vision.amv_receipt_release(c.byref(r)))
        base.close_model(vision, model)
        assert gated.counts == [1, 1]
        base.shared.close(vision, ctx, False)
    finally:
        gated.gate.set()
    return {"offlineNotReplacedByRealtime": True, "timeoutMs": round(elapsed*1000), "capacityResume": True}


def admission_deadline(vision, runtime, directory):
    ctx = base.initialize(vision, runtime)
    model = t.load(vision, ctx, directory/"image-dynamic.onnx")
    gated = Gated(0.03)
    gated.gate.set()
    receipt, ids, codes = submit(vision, model, [gated.item()], wait=1)
    assert ids == [0] and codes == [a.AM_ERR_SDK_DEADLINE_EXCEEDED]
    assert gated.counts == [1, 1]
    check(vision.amv_receipt_release(c.byref(receipt)))
    base.close_model(vision, model)
    base.shared.close(vision, ctx, False)
    return {"noAcceptanceAfterCaptureExceedsDeadline": True, "ownersBalanced": True}


def control_and_errors(vision, runtime, directory):
    ctx = base.initialize(vision, runtime)
    model = t.load(vision, ctx, directory/"unproved.onnx")
    receipts = []
    for _ in range(32):
        receipt, ids, codes = submit(vision, model, [Item(0)], a.AM_BATCH_NATIVE)
        assert ids == [0] and codes == [a.AM_ERR_VISION_BATCH_UNSUPPORTED]
        receipts.append(receipt)
    item = Item(0)
    array = (a.am_batch_item_t*1)(item.value)
    out = size(a.am_batch_receipt_t)
    check(vision.amv_infer_batch(model, 1, array, None, c.byref(out)), a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
    assert out.accepted_count == 0 and not out.request_ids and not out.rejects
    for receipt in receipts:
        check(vision.amv_receipt_release(c.byref(receipt)))
    malformed = Item(0)
    malformed.value.struct_size = 4
    receipt, ids, codes = submit(vision, model, [malformed], a.AM_BATCH_SPLIT)
    assert ids == [0] and codes == [a.AM_ERR_SDK_STRUCT_SIZE_INVALID]
    check(vision.amv_request_snapshot_release(c.cast(receipt.request_ids, c.POINTER(a.am_request_snapshot_t))), a.AM_ERR_SDK_HANDLE_INVALID)
    check(vision.amv_receipt_release(c.byref(receipt)))
    check(vision.amv_infer_batch(model, 65, None, None, c.byref(out)), a.AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
    assert not out.request_ids
    receipt, ids, codes = submit(vision, model, [Item(0)], a.AM_BATCH_SPLIT)
    collect(vision, ctx, ids, [0])
    check(vision.amv_receipt_release(c.byref(receipt)))
    # 控制类字节上限同样约束新回执，不能先接受请求再丢失回执。
    policy = size(a.am_queue_policy_t)
    check(vision.amv_get_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
    policy.max_bytes = 1
    check(vision.amv_set_queue_policy(ctx, a.AM_Q_CONTROL, c.byref(policy)))
    check(vision.amv_infer_batch(model, 1, array, None, c.byref(out)), a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
    assert out.accepted_count == 0 and not out.request_ids
    base.close_model(vision, model)
    base.shared.close(vision, ctx, False)
    return {"receiptItemsCap": 32, "bytesCap": True, "releaseRestoresCapacity": True, "invalidTypesRejected": True}


def profile_images(vision, ctx, directory):
    registry = json.loads((base.ROOT/"crates/vision/resources/profiles.json").read_text(encoding="utf-8"))["profiles"]
    files = ["yolo26n-pose.onnx", "yolo11n.onnx", "yolo11n-cls.onnx", "yolo11n-seg.onnx", "yolo11n-obb.onnx"]
    records = []
    for task, (profile, filename) in enumerate(zip(registry, files), 1):
        model, uid = base.load(vision, ctx, directory/filename, profile["id"])
        png = base.png()
        data = (c.c_ubyte*len(png)).from_buffer_copy(png)
        items = []
        for i in range(2):
            item = type("EncodedItem", (), {})()
            item.assoc = size(a.am_frame_assoc_t)
            item.assoc.frame_id, item.assoc.valid = 2000+i, a.AM_ASSOC_FRAME
            item.image = size(a.am_image_input_t)
            item.image.kind, item.image.encoded, item.image.encoded_size = a.AM_IMG_PNG, data, len(data)
            item.image.assoc = c.pointer(item.assoc)
            item.value = size(a.am_batch_item_t)
            item.value.image = c.pointer(item.image)
            items.append(item)
        receipt, ids, codes = submit(vision, model, items)
        assert all(ids) and codes == [0, 0]
        c.memset(data, 0, len(data))
        raw = []
        for i, rid in enumerate(ids):
            base.wait(vision, ctx, rid)
            result = c.POINTER(a.amv_result_t)()
            check(vision.amv_result_fetch(ctx, rid, c.byref(result)))
            assert result.contents.task == task and result.contents.model.model_uid == uid
            assert result.contents.frame.contents.frame_id == 2000+i
            assert result.contents.transform.contents.valid == 1
            raw.append(base.outputs(result))
            check(vision.amv_result_ack(ctx, rid))
            check(vision.amv_result_release(result))
        assert raw[0] == raw[1]
        check(vision.amv_receipt_release(c.byref(receipt)))
        base.close_model(vision, model)
        records.append({"profile": profile["id"], "images": 2, "copiedBeforeReturn": True})
    return records


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--sample-dir", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path)
    parser.add_argument("--raw-only", action="store_true", help="显式只验原始张量调度；不代表五类档案已验")
    args = parser.parse_args()
    if not args.raw_only and args.model_dir is None:
        parser.error("--model-dir is required unless --raw-only is explicitly selected")
    vision = c.CDLL(str((args.dll_dir/"am_vision.dll").resolve()))
    missing = a.bind(vision, "amv_")
    assert "amv_infer_batch" not in missing and "amv_receipt_release" not in missing
    ctx = base.initialize(vision, args.runtime_dir)
    cap = size(a.amv_capability_t)
    check(vision.amv_query_capability(ctx, c.byref(cap)))
    assert cap.feature_bits & a.AMV_CAP_BATCH
    result = {"modes": basic(vision, ctx, args.sample_dir), "partial": partial_and_modes(vision, ctx, args.sample_dir),
              "native64": sixty_four(vision, ctx, args.sample_dir),
              "profileImages": {"skipped": True, "reason": "explicit --raw-only; profile acceptance remains separate"}
                  if args.raw_only else profile_images(vision, ctx, args.model_dir)}
    base.shared.close(vision, ctx, False)
    result["receipt"] = receipt_close(vision, args.runtime_dir, args.sample_dir)
    result["nativeCancel"] = native_cancel(vision, args.runtime_dir, args.sample_dir)
    result["wait"] = waiting(vision, args.runtime_dir, args.sample_dir)
    result["admissionDeadline"] = admission_deadline(vision, args.runtime_dir, args.sample_dir)
    result["controls"] = control_and_errors(vision, args.runtime_dir, args.sample_dir)
    result["actualVisionExports"] = sum(n.startswith("amv_") for n in a.FUNCTIONS)-len(missing)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
