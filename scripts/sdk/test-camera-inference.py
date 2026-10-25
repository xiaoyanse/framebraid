"""真实双DLL：Digest抓图的同一像素进入异步推理；不启动业务服务。"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import sys
import threading

sys.dont_write_bytecode = True


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


h = module("capture_test", "test-camera-capture.py")
b = module("inference_test", "test-inference.py")
a, size, check = h.a, h.size, h.check


def run(args):
    camera = c.CDLL(str(args.dll_dir.resolve()/"am_camera.dll"))
    vision = c.CDLL(str(args.dll_dir.resolve()/"am_vision.dll"))
    a.bind(camera, "amc_", require_all=True)
    a.bind(vision, "amv_", require_all=True)
    server = h.ThreadingHTTPServer(("127.0.0.1", 0), h.Fixture)
    worker = threading.Thread(target=server.serve_forever)
    worker.start()
    try:
        ctx = h.shared.init(camera, True)
        vctx = b.initialize(vision, args.runtime_dir)
        source = h.open_source(camera, ctx, server.server_port)
        image = h.captured(camera, source, a.AM_CAP_CPU_FRAME)
        assoc = size(a.am_frame_assoc_t)
        for name, _ in assoc._fields_:
            if name != "struct_size" and hasattr(image.contents.meta.contents, name):
                setattr(assoc, name, getattr(image.contents.meta.contents, name))
        assoc.valid = 1
        errors, counts = [], [0, 0]

        @c.CFUNCTYPE(None, c.c_void_p)
        def retain(pointer):
            counts[0] += 1
            if camera.amc_image_retain(c.cast(pointer, c.POINTER(a.am_captured_image_t))):
                errors.append("retain")

        @c.CFUNCTYPE(None, c.c_void_p)
        def release(pointer):
            counts[1] += 1
            if camera.amc_image_release(c.cast(pointer, c.POINTER(a.am_captured_image_t))):
                errors.append("release")

        pixel = size(a.am_pixel_view_t)
        pixel.magic = a.AM_PIXEL_VIEW_MAGIC
        pixel.owner_handle = c.cast(image, c.c_void_p)
        pixel.retain_owner, pixel.release_owner = retain, release
        pixel.view, pixel.assoc = image.contents.view, c.pointer(assoc)
        request = size(a.am_image_input_t)
        request.kind, request.view = a.AM_IMG_PIXEL_VIEW, c.pointer(pixel)
        model, _ = b.load(vision, vctx, args.model_dir/"yolo26n-pose.onnx", "pose-yolo26-e2e-coco17-v1")
        baseline, bid, _ = b.sync(vision, vctx, model, request)
        expected = b.outputs(baseline)
        check(vision.amv_result_ack(vctx, bid))
        check(vision.amv_result_release(baseline))
        request.flags = a.AM_INPUT_BORROW_RETAINED
        rid = c.c_uint64()
        check(vision.amv_infer_image_async(model, c.byref(request), None, c.byref(rid)))
        check(camera.amc_image_release(image))
        b.wait(vision, vctx, rid.value)
        result = c.POINTER(a.amv_result_t)()
        check(vision.amv_result_fetch(vctx, rid.value, c.byref(result)))
        assert b.outputs(result) == expected
        assert result.contents.frame.contents.frame_id == assoc.frame_id
        check(vision.amv_result_ack(vctx, rid.value))
        check(vision.amv_result_release(result))
        b.close_model(vision, model)
        h.close_object(camera, source)
        h.shared.close(camera, ctx, True)
        b.shared.close(vision, vctx, False)
        assert not errors and counts[0] == counts[1] == 1, (errors, counts)
        print(json.dumps({"isapiPixelsToRealPose": True, "samePixelsRawOutputsExact": True,
                          "frameId": assoc.frame_id, "borrowRetainRelease": counts,
                          "scope": "loopback Digest and CPU; not field-camera or clean-machine acceptance"}))
    finally:
        server.shutdown()
        server.server_close()
        worker.join()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    run(parser.parse_args())
