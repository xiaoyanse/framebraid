"""Validate and compare bounded, build-bound performance evidence."""
import argparse
import hashlib
import json
import math
import re
from pathlib import Path
import statistics

SCHEMA = "framebraid.performance.v1"
MAX_SAMPLES = 100_000
IDENTITY_KEYS = ("workload", "input_sha256", "model_sha256", "runtime_sha256",
                 "hardware", "backend", "threads", "budgets", "warmup", "timing_boundary")
BUDGET_FILE = Path(__file__).with_name("perf-budgets.json")


def workload_budget(name):
    manifest = json.loads(BUDGET_FILE.read_text(encoding="utf-8"))
    if manifest["schema"] != "framebraid.performance-budgets.v1":
        raise ValueError("unsupported budget schema")
    return dict(manifest[name], manifest_sha256=file_sha256(BUDGET_FILE),
                process_private_growth_warning_bytes=manifest["process_private_growth_warning_bytes"])


def deadline_observation(samples, budget_ms):
    return dict(budget_ms=budget_ms, violations=sum(x > budget_ms * 1_000_000 for x in samples),
                measured=len(samples), scope="local function or receipt-to-consumption; no camera exposure clock")


def process_resources():
    import ctypes as c
    import os
    if os.name != "nt":
        return dict(private_bytes=None, working_set_bytes=None, handles=None)
    class Counters(c.Structure):
        _fields_ = [("size", c.c_uint32), ("faults", c.c_uint32)] + [
            (name, c.c_size_t) for name in ("peak_working", "working", "peak_paged", "paged",
                                          "peak_nonpaged", "nonpaged", "pagefile", "peak_pagefile", "private")]
    kernel = c.WinDLL("kernel32", use_last_error=True)
    kernel.GetCurrentProcess.restype = c.c_void_p
    kernel.GetProcessHandleCount.argtypes = [c.c_void_p, c.POINTER(c.c_uint32)]
    kernel.GetProcessHandleCount.restype = c.c_int
    psapi = c.WinDLL("psapi", use_last_error=True)
    psapi.GetProcessMemoryInfo.argtypes = [c.c_void_p, c.POINTER(Counters), c.c_uint32]
    psapi.GetProcessMemoryInfo.restype = c.c_int
    counter = Counters()
    counter.size = c.sizeof(counter)
    process = kernel.GetCurrentProcess()
    handles = c.c_uint32()
    if not psapi.GetProcessMemoryInfo(process, c.byref(counter), counter.size) or not kernel.GetProcessHandleCount(process, c.byref(handles)):
        raise OSError(c.get_last_error(), "process resource query failed")
    return dict(private_bytes=counter.private, working_set_bytes=counter.working, handles=handles.value)


def file_sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def dll_build_identity(path, package, target, notes, observer_commit, observer_state):
    fields = dict(item.split("=", 1) for item in notes.split(";") if "=" in item)
    if fields.get("profile") != "release":
        raise ValueError("benchmark requires a DLL reporting a release build")
    commit = fields.get("source_commit", "")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("loaded DLL does not report a full source commit")
    return dict(source_commit=commit, source_state=fields.get("source_state", "unknown"),
                artifact_sha256=file_sha256(path), build_profile=fields["profile"],
                observer_commit=observer_commit, observer_state=observer_state,
                loaded_sdk=dict(package=package, target=target, notes=notes))


def summarize(samples_ns):
    if len(samples_ns) > MAX_SAMPLES:
        raise ValueError("sample budget exceeded")
    if not samples_ns:
        return dict(count=0, mean_ms=None, p50_ms=None, p95_ms=None,
                    p99_ms=None, p99_representative=False)
    if any(isinstance(x, bool) or not isinstance(x, (int, float))
           or not math.isfinite(x) or x < 0 for x in samples_ns):
        raise ValueError("invalid duration")
    ordered = sorted(samples_ns)
    def percentile(p):
        return ordered[math.ceil(p * len(ordered)) - 1] / 1_000_000
    return dict(count=len(ordered), mean_ms=statistics.fmean(ordered) / 1_000_000,
                p50_ms=percentile(.5), p95_ms=percentile(.95), p99_ms=percentile(.99),
                p99_representative=len(ordered) >= 1000)


def validate(report):
    if report.get("schema") != SCHEMA:
        raise ValueError("unsupported evidence schema")
    identity = report.get("identity", {})
    for key in IDENTITY_KEYS:
        if key not in identity or identity[key] is None or identity[key] == "" or identity[key] == {}:
            raise ValueError("missing identity: " + key)
    for key in ("input_sha256", "model_sha256", "runtime_sha256"):
        value = identity[key]
        if not isinstance(value, str) or not (
                re.fullmatch(r"[0-9a-f]{64}", value) or value.startswith("not-applicable:")):
            raise ValueError("invalid digest identity: " + key)
    for key in ("hardware", "budgets"):
        if not isinstance(identity[key], dict) or not identity[key]:
            raise ValueError("structured identity required: " + key)
    for key, minimum in (("threads", 1), ("warmup", 0)):
        value = identity[key]
        if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
            raise ValueError("invalid execution identity: " + key)
    for key in ("source_commit", "source_state", "artifact_sha256", "build_profile"):
        if not report.get("build", {}).get(key):
            raise ValueError("missing build identity: " + key)
    if report["build"]["source_state"] not in ("clean", "dirty", "unknown"):
        raise ValueError("invalid source state")
    if not re.fullmatch(r"[0-9a-f]{40}", report["build"]["source_commit"]):
        raise ValueError("full source commit required")
    if not re.fullmatch(r"[0-9a-f]{64}", report["build"]["artifact_sha256"]):
        raise ValueError("artifact digest required")
    for key in ("attempted", "delivered", "rejected", "dropped"):
        n = report.get("counts", {}).get(key)
        if isinstance(n, bool) or not isinstance(n, int) or n < 0:
            raise ValueError("invalid counter: " + key)
    counts = report["counts"]
    if counts["attempted"] != sum(counts[k] for k in ("delivered", "rejected", "dropped")):
        raise ValueError("counter conservation failed")
    elapsed = report.get("elapsed_ns")
    if isinstance(elapsed, bool) or not isinstance(elapsed, int) or elapsed <= 0:
        raise ValueError("invalid elapsed time")
    samples = report.get("samples_ns")
    if not isinstance(samples, list) or len(samples) != counts["delivered"]:
        raise ValueError("one latency sample required per delivered result")
    summarize(samples)
    return report


def compare(baseline, candidate):
    validate(baseline)
    validate(candidate)
    if baseline["identity"] != candidate["identity"]:
        raise ValueError("incompatible workload/runtime/budget identity")
    if baseline["build"]["build_profile"] != candidate["build"]["build_profile"]:
        raise ValueError("incompatible build profile")
    if baseline.get("output_checksum_fnv1a64") != candidate.get("output_checksum_fnv1a64"):
        raise ValueError("output changed; correctness review required before timing comparison")
    a, b = summarize(baseline["samples_ns"]), summarize(candidate["samples_ns"])
    if not a["count"] or not b["count"]:
        raise ValueError("cannot compare empty delivered population")
    ta = baseline["counts"]["delivered"] * 1e9 / baseline["elapsed_ns"]
    tb = candidate["counts"]["delivered"] * 1e9 / candidate["elapsed_ns"]
    latency = None if a["p95_ms"] == 0 else b["p95_ms"] / a["p95_ms"] - 1
    rate_a = baseline["counts"]["delivered"] / max(1, baseline["counts"]["attempted"])
    rate_b = candidate["counts"]["delivered"] / max(1, candidate["counts"]["attempted"])
    warnings = []
    if latency is None:
        warnings.append("baseline latency below timer resolution")
    elif latency > .05:
        warnings.append("p95 regression exceeds 5 percent")
    if tb < ta * .95:
        warnings.append("effective throughput regression exceeds 5 percent")
    if rate_b < rate_a:
        warnings.append("delivery fraction decreased; speedup not accepted")
    return dict(baseline=a, candidate=b, p95_relative_change=latency,
                effective_throughput_per_s=dict(baseline=ta, candidate=tb),
                delivery_fraction=dict(baseline=rate_a, candidate=rate_b),
                warnings=warnings, acceptance="requires repeated-run review")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    args = parser.parse_args()
    print(json.dumps(compare(json.loads(args.baseline.read_text(encoding="utf-8")),
                             json.loads(args.candidate.read_text(encoding="utf-8"))), indent=2))


if __name__ == "__main__":
    main()
