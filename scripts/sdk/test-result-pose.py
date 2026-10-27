"""Python Pose result copy lifetime checks with synthetic data and optional CPU model."""
import argparse
import ctypes as c
import gc
from pathlib import Path
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
import amrt_sdk as a
from safe_sdk import Result, VisionContext, RgbBuffer, sized


def synthetic():
    points = (a.am_keypoint_t * 2)(a.am_keypoint_t(.1, .2, .3), a.am_keypoint_t(.4, .5, .6))
    rows = (a.am_pose_detection_t * 1)()
    rows[0].bounds[:] = (0., .25, .5, 1.)
    rows[0].confidence = .75
    rows[0].kpt_count, rows[0].keypoints = 2, points
    raw = sized(a.amv_result_t)
    raw.task, raw.pose.count, raw.pose.detections = a.AM_TASK_POSE, 1, rows
    released = []
    result = Result(None, c.pointer(raw), lambda _: released.append(True) or 0, False)
    copy = result.copy_pose()
    assert len(copy) == 1 and len(copy[0][2]) == 2 and copy[0][0] == (0., .25, .5, 1.)
    assert abs(copy[0][2][0][0] - .1) < 1e-6
    rows[0].bounds[0] = .9
    points[0].x = .9
    assert copy[0][0][0] == 0. and abs(copy[0][2][0][0] - .1) < 1e-6
    raw.pose.count = 301
    try:
        result.copy_pose(); raise AssertionError("oversize accepted")
    except ValueError:
        pass
    raw.pose.count = 1
    raw.task = a.AM_TASK_GENERIC
    try:
        result.copy_pose(); raise AssertionError("non-Pose accepted")
    except ValueError:
        pass
    raw.task = a.AM_TASK_POSE
    rows[0].kpt_count = 1025
    try:
        result.copy_pose(); raise AssertionError("oversize keypoints accepted")
    except ValueError:
        pass
    rows[0].kpt_count = 2
    rows[0].keypoints = c.POINTER(a.am_keypoint_t)()
    try:
        result.copy_pose(); raise AssertionError("null keypoints accepted")
    except ValueError:
        pass
    result.close()
    assert released == [True]
    try:
        result.copy_pose(); raise AssertionError("closed result accepted")
    except RuntimeError:
        pass
    gc.collect()
    assert copy[0][0][0] == 0.

    # 控制读取过程，证明 close 不能在持锁的复制中释放原生结果。
    entered, resume, closed = threading.Event(), threading.Event(), threading.Event()
    class Delayed:
        @property
        def task(self):
            entered.set(); assert resume.wait(5); return a.AM_TASK_POSE
        pose = a.am_pose_result_t()
    class Pointer:
        @property
        def contents(self): return Delayed()
    controlled = Result(None, Pointer(), lambda _: closed.set() or 0, False)
    errors = []
    def read():
        try: assert controlled.copy_pose() == ()
        except BaseException as error: errors.append(error)
    reader = threading.Thread(target=read); reader.start(); assert entered.wait(5)
    closer = threading.Thread(target=controlled.close); closer.start()
    assert not closed.wait(.05)
    resume.set(); reader.join(5); closer.join(5)
    assert not reader.is_alive() and not closer.is_alive() and closed.is_set() and not errors


def real(args):
    with VisionContext(args.dll, [args.runtime]) as vision, \
         vision.load_model(args.model, "pose-yolo26-e2e-coco17-v1") as model, \
         RgbBuffer(640, 640, bytes(640*640*3)) as image:
        request, result = image.pixels().infer(model)
        with result:
            snapshot = result.copy_pose()
            raw = result.view
            assert len(snapshot) == raw.pose.count
            vision.ack(request)
        gc.collect()
        assert isinstance(snapshot, tuple)
        try:
            result.copy_pose(); raise AssertionError("closed result accepted")
        except RuntimeError:
            pass
        return len(snapshot)


def real_image(args):
    with VisionContext(args.dll, [args.runtime]) as vision, \
         vision.load_model(args.model, "pose-yolo26-e2e-coco17-v1") as model:
        request, result = model.infer_encoded(args.image.read_bytes())
        with result:
            snapshot = result.copy_pose()
            raw = result.view
            assert raw.pose.count == len(snapshot) and len(snapshot) > 0
            for index, copied in enumerate(snapshot):
                native = raw.pose.detections[index]
                assert copied[0] == tuple(native.bounds)
                assert copied[1] == native.confidence
                assert copied[2] == tuple((p.x, p.y, p.confidence) for p in native.keypoints[:native.kpt_count])
            vision.ack(request)
        gc.collect()
        assert len(snapshot) > 0 and len(snapshot[0][2]) == 17
        return len(snapshot)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--image", type=Path)
    args = parser.parse_args()
    synthetic()
    summary = {"synthetic_nonempty_copy": True, "concurrent_close": True, "real_cpu_pose_count": real(args)}
    if args.image:
        summary["real_image_nonempty_pose_count"] = real_image(args)
    print(summary)
