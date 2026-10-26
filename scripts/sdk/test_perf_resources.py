"""量化工具边界：窗口CPU分母、未知GPU、采样上限及输入完整性。"""
import time
import unittest
from unittest.mock import patch

from perf_resources import ResourceSampler, cpu_delta, cpu_snapshot, summarize_gpu
from perf_stream_input import annexb, has_random_access_headers


class MetricsTests(unittest.TestCase):
    def test_cpu_denominators_not_aggregate_percent(self):
        result = cpu_delta(dict(wall_ns=0, cpu_ns=100), dict(wall_ns=2_000_000_000, cpu_ns=1_000_000_100), 4)
        self.assertEqual(result["cpu_s"], 1)
        self.assertEqual(result["core_equivalent"], .5)
        self.assertEqual(result["normalized_percent"], 12.5)
        for parallel in (0, -1, True):
            with self.assertRaises(ValueError):
                cpu_delta(dict(wall_ns=0, cpu_ns=0), dict(wall_ns=1, cpu_ns=1), parallel)

    def test_cpu_window_rejects_reversal(self):
        for end in (dict(wall_ns=0, cpu_ns=2), dict(wall_ns=1, cpu_ns=-1)):
            with self.assertRaises(ValueError):
                cpu_delta(dict(wall_ns=0, cpu_ns=0), end, 1)

    def test_cpu_snapshot_is_cumulative(self):
        before = cpu_snapshot()
        sum(index * index for index in range(10000))
        after = cpu_snapshot()
        self.assertGreater(after["wall_ns"], before["wall_ns"])
        self.assertGreaterEqual(after["cpu_ns"], before["cpu_ns"])

    def test_gpu_unknown_not_zero_and_engines_not_summed(self):
        self.assertIsNone(summarize_gpu([dict(gpu=dict(values=None))]))
        rows = [dict(gpu=dict(values=dict(engines={
            "pid_1_luid_0x0_0xff_phys_0_eng_0_engtype_VideoDecode": 10,
            "pid_1_luid_0x0_0xff_phys_0_eng_1_engtype_Copy": 7,
            "invalid": None}))) for _ in range(2)]
        result = summarize_gpu(rows)
        self.assertEqual(len(result), 2)
        self.assertEqual(sorted(row["mean"] for row in result), [7, 10])
        self.assertEqual(result[0]["device_luid_parts"], ["0x0", "0xff", "0"])

    def test_disabled_observer_does_not_probe_gpu(self):
        with patch("perf_resources.GpuCounters") as gpu:
            result = ResourceSampler(enabled=False).start().stop()
        gpu.assert_not_called()
        self.assertEqual(result["samples"], [])
        self.assertIsNone(result["gpu_series"])

    def test_sampler_is_bounded_and_closes_query(self):
        with patch("perf_resources.GpuCounters") as gpu:
            gpu.return_value.collect.return_value = dict(values=None, errors={})
            observer = ResourceSampler(interval_s=.05, max_samples=1).start()
            time.sleep(.16)
            result = observer.stop()
            gpu.return_value.close.assert_called_once()
        self.assertTrue(result["capacity_reached"])
        self.assertEqual(len(result["samples"]), 1)
        self.assertFalse(observer.thread.is_alive())

    def test_sampler_failure_is_not_resource_zero(self):
        with patch("perf_resources.GpuCounters", side_effect=OSError("unavailable")):
            observer = ResourceSampler().start()
            result = observer.stop()
        self.assertTrue(result["errors"])
        self.assertIsNone(result["gpu_series"])

    def test_annex_extent_validation(self):
        self.assertEqual(annexb(b"\0\0\0\2ab\0\0\0\1c"), b"\0\0\0\1ab\0\0\0\1c")
        for data in (b"\0\0", b"\0\0\0\0", b"\0\0\0\3ab"):
            with self.assertRaises(ValueError):
                annexb(data)

    def test_three_nals_do_not_imply_random_access_headers(self):
        self.assertFalse(has_random_access_headers(b"\0\0\0\1\x01a" * 3))
        self.assertTrue(has_random_access_headers(b"\0\0\0\1\x67a\0\0\0\1\x68b\0\0\0\1\x65c"))


if __name__ == "__main__":
    unittest.main()
