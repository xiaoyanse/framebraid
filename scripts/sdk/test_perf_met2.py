"""防止在途漏计、少交付和跨后端A/A混淆生成可信测量摘要。"""
import copy
import unittest

from perf_met2 import review_sampling, review_stream_rows, spread, validate_stream


def fixture():
    return dict(policy="cpu", actual_backend="ffmpeg-cpu",
        counts=dict(opening_inflight=2, window_inputs=180, all_outputs_before_end=180,
            closing_inflight=2, measured_input_outputs_before_end=178,
            measured_input_outputs_after_end=2, useful_before_end=177, expired_before_end=1),
        samples_ns=[1] * 178, cpu=dict(wall_s=6, cpu_s=1),
        latency=dict(p95_ms=100), useful_fps=29.5, observation=dict(gpu_series=None))


class ReviewTests(unittest.TestCase):
    def test_sampling_ceiling_not_mistaken_for_achieved_rate(self):
        row=dict(cpu=dict(wall_s=4),compressed_packets=120,
            consumers=[dict(max_fps=30,delivered=60,frames=[dict(frame_id=i) for i in range(60)],age=dict(p95_ms=150))],
            stats=dict(external_held_items=1),after_source_close=dict(external_held_items=0))
        result=review_sampling(row)
        self.assertFalse(result["experimental_guards_met"])
        self.assertEqual(len(result["findings"]),1)
        self.assertTrue(result["postclose_count_observed"])

    def test_sampling_missing_source_and_leases_reported_separately(self):
        row=dict(cpu=dict(wall_s=4),compressed_packets=60,consumers=[],
                 after_source_close=dict(external_held_items=1))
        self.assertEqual(len(review_sampling(row)["findings"]),2)

    def test_sampling_fixed_window_integer_floor_not_clock_tail(self):
        row=dict(cpu=dict(wall_s=4.001),compressed_packets=120,
            consumers=[dict(max_fps=10,delivered=32,frames=[dict(frame_id=i) for i in range(32)],age=dict(p95_ms=100))])
        self.assertTrue(review_sampling(row,4)["experimental_guards_met"])

    def test_inflight_conservation_is_not_simple_attempted_delivered(self):
        validate_stream(fixture())
        row = fixture()
        row["counts"]["closing_inflight"] = 0
        with self.assertRaises(ValueError):
            validate_stream(row)
        row = fixture()
        row["cpu"]["cpu_s"] = float("nan")
        with self.assertRaises(ValueError):
            validate_stream(row)

    def test_missing_tail_and_bad_freshness_denominators_rejected(self):
        for key in ("measured_input_outputs_after_end", "useful_before_end"):
            row = fixture()
            row["counts"][key] += 1
            with self.assertRaises(ValueError):
                validate_stream(row)

    def test_report_keeps_absolute_relative_spread_and_unknown_gpu(self):
        first, second = fixture(), fixture()
        second["cpu"]["cpu_s"] = 1.2
        output = review_stream_rows([first, second], 6)["cpu"]
        self.assertAlmostEqual(output["cpu_s"]["absolute_span"], .2)
        self.assertFalse(output["gpu_observed"])
        self.assertIn("require target-card review", output["verdict"])

    def test_changed_backend_not_same_AA(self):
        first, second = fixture(), fixture()
        second["actual_backend"] = "ffmpeg-d3d11va"
        with self.assertRaises(ValueError):
            review_stream_rows([first, second], 6)

    def test_zero_resolution_and_invalid_values(self):
        self.assertIsNone(spread([0, 0])["relative_span"])
        for values in ([], [-1], [float("nan")]):
            with self.assertRaises(ValueError):
                spread(values)
        row = copy.deepcopy(fixture())
        row["counts"]["useful_before_end"] = True
        with self.assertRaises(ValueError):
            validate_stream(row)


if __name__ == "__main__":
    unittest.main()
