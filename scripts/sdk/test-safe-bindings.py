"""只测语言侧所有权，不加载 DLL，不重复核心推理回归。"""
import ctypes as c
from pathlib import Path
import sys
import types
import unittest
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdk" if (ROOT / "sdk").is_dir() else ROOT
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a
from safe_sdk import BorrowedFrame, Model, PublisherPool, Resource, Result, _CALLBACKS, sized


class Owner(Resource):
    def __init__(self):
        self.events = []
        self.final_state = a.AM_LIFECYCLE_CLOSED
        self.functions = types.SimpleNamespace(amv_object_destroy=self.destroy,
            amv_result_notify_callback_set=self.notify, amv_shared_pool_return=self.return_borrow)
        self.callback = None
        super().__init__(None, 1, self.close_native)

    @property
    def library(self):
        return self.functions

    @property
    def prefix(self):
        return "amv_"

    def close_native(self, handle, timeout, state):
        c.cast(state, c.POINTER(c.c_uint32))[0] = self.final_state
        self.events.append(("close", handle))
        return 0

    def destroy(self, handle):
        self.events.append(("destroy", handle))
        return 0

    def notify(self, handle, callback, user):
        self.callback = callback
        return 0

    def return_borrow(self, handle, view):
        self.events.append(("return", handle))
        return 0


class OwnershipTests(unittest.TestCase):
    def test_shared_publication_requires_explicit_identity(self):
        owner = Owner()
        publisher = PublisherPool(owner, 5, owner.close_native)
        with self.assertRaisesRegex(ValueError, "explicit source/frame identity"):
            publisher.publish(sized(a.am_pixel_view_t))
        publisher.close()
        owner.close()

    def test_timeout_preserves_retryable_handle(self):
        owner = Owner()
        owner.final_state = a.AM_LIFECYCLE_CLOSING
        with self.assertRaises(TimeoutError):
            owner.close(0)
        self.assertEqual(owner.handle, 1)
        self.assertFalse(any(event[0] == "destroy" for event in owner.events))
        owner.final_state = a.AM_LIFECYCLE_CLOSED
        owner.close()
        owner.close()
        self.assertEqual(owner.events.count(("destroy", 1)), 1)

    def test_parent_cannot_close_before_child(self):
        owner = Owner()
        child = Resource(owner, 2, owner.close_native)
        with self.assertRaises(RuntimeError):
            owner.close()
        self.assertFalse(owner.events)
        child.close()
        owner.close()
        self.assertEqual(owner.events, [("close", 2), ("destroy", 2), ("close", 1), ("destroy", 1)])

    def test_shared_borrow_return_before_pool_close(self):
        owner = Owner()
        pool = Resource(owner, 3, owner.close_native)
        borrow = BorrowedFrame(pool, sized(a.am_shared_frame_view_t))
        with self.assertRaises(RuntimeError):
            pool.close()
        borrow.close()
        borrow.close()
        with self.assertRaises(RuntimeError):
            borrow.pixel_view()
        pool.close()
        owner.close()
        self.assertEqual(owner.events[0], ("return", 3))

    def test_result_copy_survives_release_and_cannot_reborrow(self):
        owner = Owner()
        data = (c.c_float * 2)(1.25, -4.5)
        shape = (c.c_int64 * 1)(2)
        tensor = a.am_tensor_out_t()
        tensor.name, tensor.dtype, tensor.shape, tensor.shape_len = b"output", a.AM_DTYPE_F32, shape, 1
        tensor.data, tensor.data_size = c.addressof(data), c.sizeof(data)
        raw = sized(a.amv_result_t)
        raw.raw_output_count, raw.raw_outputs = 1, c.pointer(tensor)
        released = []
        result = Result(owner, c.pointer(raw), lambda pointer: released.append(pointer) or 0, False)
        copied = result.raw_outputs()
        result.close()
        c.memset(data, 0, c.sizeof(data))
        self.assertEqual(copied[0][:3], ("output", a.AM_DTYPE_F32, (2,)))
        self.assertEqual(len(released), 1)
        self.assertNotEqual(copied[0][3], bytes(c.sizeof(data)))
        with self.assertRaises(RuntimeError):
            result.raw_outputs()
        owner.close()

    def test_callback_stays_pinned_until_unregister(self):
        owner = Owner()
        model = Model(owner, 4, owner.close_native)
        model.enable_notifications()
        self.assertIsNotNone(model._notify)
        self.assertIs(_CALLBACKS[4][0], model)
        owner.callback(None, 123, a.AM_REQ_DONE, None)
        self.assertEqual(model.notifications.get_nowait(), (123, a.AM_REQ_DONE))
        model.enable_notifications(False)
        self.assertIsNone(model._notify)
        self.assertNotIn(4, _CALLBACKS)
        self.assertFalse(owner.callback)
        model.close()
        owner.close()


if __name__ == "__main__":
    unittest.main()
