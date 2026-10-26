"""工具解析失败仍必须归还原生结果，不让错误测量污染后续窗口。"""
import ctypes as c
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("inference_resources", Path(__file__).with_name("benchmark-inference-resources.py"))
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)
a = tool.a


class Library:
    def __init__(self):
        self.ack, self.released = [], 0
        self.values = (c.c_float * (300 * 57))()
        self.shape = (c.c_int64 * 3)(1, 300, 57)
        self.tensor = a.am_tensor_out_t()
        self.tensor.data = c.addressof(self.values)
        self.tensor.data_size, self.tensor.dtype = c.sizeof(self.values), a.AM_DTYPE_F32
        self.tensor.shape, self.tensor.shape_len = self.shape, 3
        self.frame = a.am_frame_assoc_t()
        self.frame.frame_id, self.frame.source_id = 3, 51
        self.result = a.amv_result_t()
        self.result.frame, self.result.model.model_uid = c.pointer(self.frame), 7
        self.result.request_id, self.result.task = 42, a.AM_TASK_POSE
        self.result.execution.actual = a.AM_BACKEND_CPU
        self.result.raw_output_count, self.result.raw_outputs = 1, c.pointer(self.tensor)

    def amv_infer_image_async(self, model, image, params, out):
        c.cast(out, c.POINTER(c.c_uint64))[0] = 42
        return 0

    def amv_result_fetch(self, ctx, request_id, out):
        c.cast(out, c.POINTER(c.POINTER(a.amv_result_t)))[0] = c.pointer(self.result)
        return 0

    def amv_result_ack(self, ctx, request_id):
        self.ack.append(request_id)
        return 0

    def amv_result_release(self, result):
        self.released += 1
        return 0


class InferenceObservationTests(unittest.TestCase):
    def test_real_payload_shape_and_owner_are_checked(self):
        lib = Library()
        result = tool.infer_once(lib, None, None, a.am_image_input_t(), 3, 7)
        self.assertEqual(result["request_id"], 42)
        self.assertEqual(lib.ack, [42])
        self.assertEqual(lib.released, 1)

    def test_parse_failures_do_not_leak_result(self):
        def wrong_model(lib): lib.result.model.model_uid = 8
        def missing_data(lib): lib.tensor.data = None
        def wrong_frame(lib): lib.frame.frame_id = 4
        def nan(lib): lib.values[0] = float("nan")
        for modify in (wrong_model, missing_data, wrong_frame, nan):
            with self.subTest(case=modify.__name__):
                lib = Library()
                modify(lib)
                with self.assertRaises(RuntimeError):
                    tool.infer_once(lib, None, None, a.am_image_input_t(), 3, 7)
                self.assertEqual(lib.ack, [42])
                self.assertEqual(lib.released, 1)


if __name__ == "__main__":
    unittest.main()
