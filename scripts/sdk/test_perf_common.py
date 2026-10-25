import copy
import unittest
import tempfile
from pathlib import Path
from perf_common import SCHEMA, IDENTITY_KEYS, MAX_SAMPLES, compare, dll_build_identity, summarize, validate


def report():
    identity = {k: "fixture" for k in IDENTITY_KEYS}
    identity.update(input_sha256="1" * 64, model_sha256="2" * 64,
                    runtime_sha256="3" * 64, hardware=dict(cpu="fixture"),
                    budgets=dict(bytes=1024), threads=1, warmup=20)
    return dict(schema=SCHEMA, identity=identity,
                build=dict(source_commit="a" * 40, source_state="clean",
                           artifact_sha256="b" * 64, build_profile="release"),
                counts=dict(attempted=4, delivered=4, rejected=0, dropped=0),
                elapsed_ns=10_000_000, samples_ns=[1_000_000] * 4)


class EvidenceTests(unittest.TestCase):
    def test_loaded_dll_identity_not_observer_checkout(self):
        test_root = Path(__file__).resolve().parents[2] / ".local/tests"
        test_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=test_root) as directory:
            path = Path(directory) / "fixture.dll"
            path.write_bytes(b"fixture")
            item = dll_build_identity(path, "0.1.0", "x64", "source_commit=" + "a"*40 + ";source_state=clean;profile=release", "b"*40, "dirty")
            self.assertEqual(item["source_commit"], "a"*40)
            self.assertEqual(item["observer_commit"], "b"*40)
            with self.assertRaises(ValueError):
                dll_build_identity(path, "0.1.0", "x64", "profile=debug", "b"*40, "dirty")

    def test_percentiles_and_unknown(self):
        self.assertIsNone(summarize([])["p95_ms"])
        self.assertEqual(summarize([1e6, 2e6, 3e6, 4e6])["p95_ms"], 4)
        self.assertFalse(summarize([1])["p99_representative"])

    def test_invalid_duration_and_budget(self):
        for values in ([-1], [float("nan")], [True], [0] * (MAX_SAMPLES + 1)):
            with self.assertRaises(ValueError):
                summarize(values)

    def test_missing_identity(self):
        for key in IDENTITY_KEYS:
            value = report()
            del value["identity"][key]
            with self.assertRaises(ValueError):
                validate(value)

    def test_incompatible_comparison(self):
        for key in IDENTITY_KEYS:
            value = report()
            value["identity"][key] = "different"
            with self.assertRaises(ValueError):
                compare(report(), value)

    def test_conservation_and_missing_samples(self):
        for key, value in (("counts", dict(attempted=5, delivered=4, rejected=0, dropped=0)),
                           ("samples_ns", []), ("elapsed_ns", 0)):
            item = report()
            item[key] = value
            with self.assertRaises(ValueError):
                validate(item)

    def test_changed_artifact_allowed_but_not_profile(self):
        value = report()
        value["build"]["artifact_sha256"] = "c" * 64
        self.assertFalse(compare(report(), value)["warnings"])
        value["build"]["build_profile"] = "debug"
        with self.assertRaises(ValueError):
            compare(report(), value)

    def test_fewer_frames_is_not_speedup(self):
        value = copy.deepcopy(report())
        value["counts"].update(delivered=2, dropped=2)
        value["samples_ns"] = [100, 100]
        value["elapsed_ns"] = 1000
        self.assertIn("delivery fraction decreased; speedup not accepted",
                      compare(report(), value)["warnings"])

    def test_zero_resolution_does_not_divide_by_zero(self):
        value = report()
        value["samples_ns"] = [0] * 4
        self.assertIsNone(compare(value, value)["p95_relative_change"])

    def test_reject_placeholder_or_empty_identity(self):
        for key, invalid in (("input_sha256", "unknown"), ("hardware", {}),
                             ("budgets", {}), ("threads", True), ("warmup", -1)):
            item = report()
            item["identity"][key] = invalid
            with self.assertRaises(ValueError):
                validate(item)

    def test_changed_pixels_not_speedup(self):
        baseline, candidate = report(), report()
        baseline["output_checksum_fnv1a64"] = "abc"
        candidate["output_checksum_fnv1a64"] = "def"
        with self.assertRaises(ValueError):
            compare(baseline, candidate)


if __name__ == "__main__":
    unittest.main()
