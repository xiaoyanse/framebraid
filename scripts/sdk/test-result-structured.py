"""Synthetic ABI fixtures for Python structured-result copy ownership."""
import ctypes as c
import gc
from pathlib import Path
import sys
import threading
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'sdk/bindings/python'))
import amrt_sdk as a
from safe_sdk import Result, sized


def owner(task):
    raw = sized(a.amv_result_t)
    raw.task = task
    released = []
    obj = Result(None, c.pointer(raw), lambda _: released.append(True) or 0, False)
    return raw, obj, released


def close_and_check(obj, released, value, method):
    obj.close(); obj.close(); gc.collect()
    assert released == [True] and value
    try:
        getattr(obj, method)()
        raise AssertionError('closed result accepted')
    except RuntimeError:
        pass


def run():
    raw, obj, released = owner(a.AM_TASK_DETECT)
    rows = (a.am_detect_box_t * 1)(); rows[0].bounds[:] = (0., .25, .5, 1.); rows[0].class_id = 7; rows[0].class_name_utf8 = b'person'
    raw.detect.count, raw.detect.boxes = 1, rows
    detect = obj.copy_detect(); rows[0].bounds[0] = .9; rows[0].class_name_utf8 = b'changed'
    close_and_check(obj, released, detect, 'copy_detect'); assert detect[0][0][0] == 0. and detect[0][3] == 'person'
    raw, obj, released = owner(a.AM_TASK_CLASSIFY)
    rows = (a.am_classify_entry_t * 1)(); rows[0].class_id = 9; rows[0].has_probability = 0; rows[0].logit = 3.5; rows[0].probability = .7
    raw.classify.count, raw.classify.entries = 1, rows
    classified = obj.copy_classify(); rows[0].logit = 8
    close_and_check(obj, released, classified, 'copy_classify'); assert classified[0][1] is None and classified[0][2] == 3.5 and classified[0][4] is False
    raw, obj, released = owner(a.AM_TASK_SEGMENT)
    mask = (c.c_uint8 * 4)(1, 1, 2, 0); rows = (a.am_segment_instance_t * 1)()
    rows[0].mask_width, rows[0].mask_height, rows[0].mask_stride_bytes, rows[0].mask_bits = 9, 2, 2, mask
    rows[0].roi_map.valid, rows[0].roi_map.scale_x = 1, .5
    raw.segment.count, raw.segment.instances = 1, rows
    segment = obj.copy_segment(); mask[0] = 255
    assert segment[0][3:7] == (9, 2, 2, bytes([1, 1, 2, 0])) and segment[0][7][0] == .5
    rows[0].mask_stride_bytes = 1
    try: obj.copy_segment(); raise AssertionError('bad stride accepted')
    except ValueError: pass
    rows[0].mask_stride_bytes = 2; rows[0].mask_bits = c.POINTER(c.c_uint8)()
    try: obj.copy_segment(); raise AssertionError('null mask accepted')
    except ValueError: pass
    rows[0].mask_bits = mask; rows[0].mask_stride_bytes = 64 * 1024 * 1024
    try: obj.copy_segment(); raise AssertionError('overbudget mask accepted')
    except ValueError: pass
    close_and_check(obj, released, segment, 'copy_segment')
    raw, obj, released = owner(a.AM_TASK_OBB)
    rows = (a.am_obb_instance_t * 1)(); rows[0].vertices_xy[:] = (1., 2., 3., 4., 5., 6., 7., 8.); rows[0].angle = .5; rows[0].angle_normalized = 1
    raw.obb.count, raw.obb.instances = 1, rows
    obb = obj.copy_obb(); rows[0].vertices_xy[0] = 99
    close_and_check(obj, released, obb, 'copy_obb'); assert obb[0][0][0] == 1. and obb[0][-1] is True
    raw, obj, released = owner(a.AM_TASK_DETECT); raw.detect.count = 1
    try: obj.copy_detect(); raise AssertionError('null array accepted')
    except ValueError: pass
    raw.detect.count = 301
    try: obj.copy_detect(); raise AssertionError('oversize accepted')
    except ValueError: pass
    try: obj.copy_obb(); raise AssertionError('wrong task accepted')
    except ValueError: pass
    obj.close()
    for method in ('copy_detect', 'copy_classify', 'copy_segment', 'copy_obb'):
        raw, obj, released = owner(a.AM_TASK_DETECT)
        entered, proceed, closed = threading.Event(), threading.Event(), threading.Event()
        def delayed(*_):
            entered.set(); assert proceed.wait(5); return []
        obj._structured = delayed
        obj._close_function = lambda _: closed.set() or 0
        errors = []
        def read():
            try: assert getattr(obj, method)() == ()
            except BaseException as error: errors.append(error)
        reader = threading.Thread(target=read); reader.start(); assert entered.wait(5)
        closer = threading.Thread(target=obj.close); closer.start(); assert not closed.wait(.05)
        proceed.set(); reader.join(5); closer.join(5)
        assert not reader.is_alive() and not closer.is_alive() and closed.is_set() and not errors
    print('detect/classify/segment/obb copies and ownership: passed (synthetic fixtures only)')


if __name__ == '__main__': run()
