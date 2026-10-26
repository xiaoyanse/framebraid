"""只读拥有者门面：竞争/GC/异常与可选真实 CPU 共享帧验证。"""
import argparse
import ctypes as c
import gc
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import threading
import weakref
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
from safe_sdk import CameraContext, VisionContext, RgbBuffer, OwnedPixels, Frame, sized
import amrt_sdk as a
from local_paths import local_path


def unit_checks():
    original = bytearray([17] * 12)
    buffer = RgbBuffer(2, 2, original)
    pixels = buffer.pixels()
    original[:] = bytes(12)
    assert pixels.copy_planes() == (bytes([17] * 12),)
    ref = weakref.ref(buffer)
    del buffer
    gc.collect()
    assert ref() is not None
    entered, release, closing, closed = [threading.Event() for _ in range(4)]
    errors = []
    class SlowModel:
        def infer_pixel(self, pixel, asynchronous, confidence, preprocess):
            entered.set()
            assert release.wait(5)
            assert c.string_at(pixel.view.contents.planes[0], 12) == bytes([17]*12)
            return 42
    def call():
        try:
            assert pixels.infer(SlowModel(), asynchronous=True) == 42
        except BaseException as error:
            errors.append(error)
    def close():
        closing.set()
        ref().close()
        closed.set()
    worker = threading.Thread(target=call)
    worker.start()
    assert entered.wait(5)
    closer = threading.Thread(target=close)
    closer.start()
    assert closing.wait(5) and not closed.wait(.05)
    release.set()
    worker.join(5)
    closer.join(5)
    assert not worker.is_alive() and not closer.is_alive() and closed.is_set() and not errors
    for action in [pixels.copy_planes, lambda: pixels.infer(SlowModel())]:
        try:
            action()
            raise AssertionError("closed owner accepted")
        except RuntimeError:
            pass
    with RgbBuffer(2, 2, bytes(12)) as buffer:
        class Failure:
            def infer_pixel(self, *args):
                raise ValueError("injected submit failure")
        try:
            buffer.pixels().infer(Failure())
            raise AssertionError("exception hidden")
        except ValueError:
            pass
        assert buffer.pixels().copy_planes() == (bytes(12),)
        buffer._frame.plane_size_bytes[0] = 1
        try:
            buffer.pixels().copy_planes()
            raise AssertionError("undersized plane accepted")
        except ValueError:
            pass
        buffer._frame.plane_size_bytes[0] = 12
    # Frame 使用同一拥有者锁；不需解码或接入相机来重现裸指针逃逸差异。
    with RgbBuffer(2, 2, bytes([9]*12)) as buffer:
        class TestFrame(Frame):
            def pixel_view(self):
                self._require()
                return buffer.pixel_view()
        frame = TestFrame(None, True, lambda _: 0, False)
        view = frame.pixels()
        frame.close()
        try:
            view.copy_planes()
            raise AssertionError("closed Frame accepted")
        except RuntimeError:
            pass
    return {"immutableInputCopy": True, "ownerGcRoot": True,
            "concurrentCloseBlockedDuringCall": True, "closedOwnerRejected": True,
            "exceptionUnlock": True, "frameInvalidation": True, "planeBoundsRejected": True}


def validate(result):
    output = result.raw_outputs()
    assert len(output) == 1 and output[0][2] == (1, 3, 2, 2)
    assert struct.unpack("<12f", output[0][3]) == (17.,)*12


def real_checks(args):
    association = sized(a.am_frame_assoc_t)
    association.source_id, association.frame_id, association.source_generation = 1, 1, 1
    association.valid = a.AM_ASSOC_SOURCE | a.AM_ASSOC_FRAME | a.AM_ASSOC_GENERATION
    association.pts = association.captured_at_ms = association.received_at_ms = -(1 << 63)
    prep = sized(a.am_raw_preprocess_t)
    prep.resize, prep.target_w, prep.target_h = a.AM_RESIZE_STRETCH, 2, 2
    with CameraContext(args.dll_dir/'am_camera.dll', shared_frames=True) as camera, \
         VisionContext(args.dll_dir/'am_vision.dll', [args.runtime_dir], True) as vision, \
         vision.load_model(args.model) as first, vision.load_model(args.model) as second:
        with RgbBuffer(2, 2, bytes([17]*12), association) as buffer:
            pixels = buffer.pixels()
            for model in (first, second):
                rid, result = pixels.infer(model, preprocess=prep)
                with result:
                    vision.ack(rid)
                    validate(result)
            with camera.create_pool(1, 64) as publisher, vision.attach_pool(publisher.descriptor) as consumer:
                publication = pixels.publish(publisher)
                borrowed = consumer.borrow(publication)
                shared = borrowed.pixels()
                try:
                    consumer.close()
                    raise AssertionError("consumer closed with live borrow")
                except RuntimeError:
                    pass
                assert shared.copy_planes() == (bytes([17]*12),)
                rids = [shared.infer(model, asynchronous=True, preprocess=prep) for model in (first, second)]
                borrowed.close()
                borrowed.close()
                try:
                    shared.infer(first, asynchronous=True, preprocess=prep)
                    raise AssertionError("returned shared lease accepted")
                except RuntimeError:
                    pass
                buffer.close()
                gc.collect()
                for rid in rids:
                    vision.wait(rid, 10)
                    with vision.fetch(rid) as result:
                        vision.ack(rid)
                        validate(result)
    return {"syncModels": 2, "asyncModels": 2, "returnedLeaseRejected": True,
            "asyncDefaultCopySurvivesReturnAndGc": True, "consumerCloseGuard": True, "rawIdentityOnly": True}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--dll-dir', type=Path)
    parser.add_argument('--runtime-dir', type=Path)
    parser.add_argument('--model', type=Path)
    args = parser.parse_args()
    output = local_path(args.output, 'evidence')
    output.mkdir(parents=True, exist_ok=True)
    summary = {"python": unit_checks()}
    if any((args.dll_dir, args.runtime_dir, args.model)):
        if not all((args.dll_dir, args.runtime_dir, args.model)):
            parser.error('all runtime inputs required')
        summary['pythonRuntime'] = real_checks(args)
    sources = sorted((ROOT/'sdk/bindings/csharp').glob('*.cs'))
    executable = output/'OwnedPixels.exe'
    command = ['C:/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe', '/nologo', '/warnaserror',
               '/platform:x64', '/out:'+str(executable), *map(str, sources),
               str(Path(__file__).with_name('owned-pixels-check.cs'))]
    compiled = subprocess.run(command, capture_output=True, text=True)
    assert compiled.returncode == 0, compiled.stdout+compiled.stderr
    env = os.environ.copy()
    if args.dll_dir:
        env['PATH'] = str(args.dll_dir.resolve())+os.pathsep+env.get('PATH', '')
    command = [str(executable)] + ([str(args.runtime_dir.resolve()), str(args.model.resolve())] if args.dll_dir else [])
    checked = subprocess.run(command, env=env, capture_output=True, text=True)
    assert checked.returncode == 0, checked.stdout+checked.stderr
    summary['csharp'] = checked.stdout.strip()
    (output/'results.json').write_text(json.dumps(summary, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(summary))


if __name__ == '__main__':
    main()
