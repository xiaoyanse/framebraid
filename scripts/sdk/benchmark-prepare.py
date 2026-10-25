"""运行实际Rust前处理函数，保存三轮可追溯基线；不代表ONNX或业务吞吐。"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time
from perf_common import SCHEMA, deadline_observation, file_sha256, summarize, validate, workload_budget

ROOT = Path(__file__).resolve().parents[2]


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--samples", type=int, default=200)
    parser.add_argument("--warmup", type=int, default=20)
    args = parser.parse_args()
    output = args.output.resolve()
    allowed = (ROOT / ".local" / "evidence").resolve()
    if not output.is_relative_to(allowed) or output.exists():
        raise SystemExit("output must be a new directory below .local/evidence")
    if not 1 <= args.samples <= 100_000 or not 0 <= args.warmup <= 10_000:
        raise SystemExit("sample/warmup budget exceeded")
    subprocess.run(["cargo", "build", "--release", "--example", "perf_prepare"],
                   cwd=ROOT, check=True)
    executable = ROOT / ".local/build/current/release/examples" / (
        "perf_prepare.exe" if os.name == "nt" else "perf_prepare")
    source_files = ["examples/perf_prepare.rs", "crates/vision/src/sdk/prepare.rs",
                    "crates/vision/src/sdk/pixels.rs", "crates/vision/src/sdk/model.rs", "Cargo.lock"]
    build = dict(source_commit=command("git", "rev-parse", "HEAD"),
                 source_state="dirty" if command("git", "status", "--porcelain") else "clean",
                 artifact_sha256=file_sha256(executable), build_profile="release",
                 source_sha256={p: file_sha256(ROOT / p) for p in source_files},
                 toolchain=command("rustc", "-Vv"))
    hardware = dict(os=platform.platform(), machine=platform.machine(),
                    processor=platform.processor(), logical_cpus=os.cpu_count())
    if os.name == "nt":
        hardware["cpu"] = command("powershell", "-NoProfile", "-Command",
                                  "Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json -Compress")
    output.mkdir(parents=True)
    summaries = []
    for index in range(3):
        start = time.perf_counter_ns()
        raw = json.loads(command(str(executable), str(args.samples), str(args.warmup)))
        wall = time.perf_counter_ns() - start
        samples = raw["samples_ns"]
        report = dict(schema=SCHEMA, build=build,
                      identity=dict(workload=raw["workload"],
                                    input_sha256=hashlib.sha256(bytes((i * 17 + i // 1920) % 251
                                        for i in range(1920 * 1080 * 4))).hexdigest(),
                                    model_sha256="not-applicable:no-model", runtime_sha256="not-applicable:rust-prepare",
                                    hardware=hardware, backend="cpu-rust", threads=1,
                                    budgets=workload_budget("prepare"),
                                    warmup=args.warmup, timing_boundary=raw["boundary"]),
                      counts=dict(attempted=len(samples), delivered=len(samples), rejected=0, dropped=0),
                      elapsed_ns=sum(samples), samples_ns=samples,
                      output_checksum_fnv1a64=raw["checksum_fnv1a64"],
                      rust_allocations_per_prepare=raw["rust_allocations_per_prepare"],
                      rust_allocated_bytes_per_prepare=raw["rust_allocated_bytes_per_prepare"],
                      clock_average_ns=raw["clock_average_ns"],
                      process_wall_ns=wall, checksum_total_ns_outside_timing=raw["checksum_total_ns_outside_timing"],
                      unknown=dict(decode_latency=None, queue_wait=None, infer_latency=None),
                      created_at=datetime.now(timezone.utc).isoformat())
        report["deadline_observation"] = deadline_observation(samples, report["identity"]["budgets"]["latency_budget_ms"])
        validate(report)
        (output / f"round-{index+1}.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        summaries.append(summarize(samples))
    print(json.dumps(dict(output=str(output), rounds=summaries,
                         limitation="local prepare only; excludes model, queue, decode and IPC"), indent=2))


if __name__ == "__main__":
    main()
