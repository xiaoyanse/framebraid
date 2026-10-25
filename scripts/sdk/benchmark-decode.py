"""离线解码/转换三轮基准，调用生产Rust原语并计数SwsContext。"""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
from perf_common import SCHEMA, file_sha256, summarize, validate

ROOT = Path(__file__).resolve().parents[2]


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def main():
    import hashlib
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--samples", type=int, default=200)
    parser.add_argument("--warmup", type=int, default=20)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to((ROOT / ".local/evidence").resolve()):
        raise SystemExit("use a new output directory below .local/evidence")
    if not 1 <= args.samples <= 100_000 or not 0 <= args.warmup <= 10_000:
        raise SystemExit("sample/warmup budget exceeded")
    subprocess.run(["cargo", "build", "--release", "--example", "perf_decode"], cwd=ROOT, check=True)
    exe = ROOT / ".local/build/current/release/examples" / ("perf_decode.exe" if os.name == "nt" else "perf_decode")
    runtimes = {p.name: file_sha256(p) for p in sorted(args.runtime_dir.glob("*.dll"))}
    if not runtimes:
        raise SystemExit("matched Windows FFmpeg DLL set required")
    build = dict(source_commit=command("git", "rev-parse", "HEAD"),
                 source_state="dirty" if command("git", "status", "--porcelain") else "clean",
                 artifact_sha256=file_sha256(exe), build_profile="release",
                 source_sha256={p: file_sha256(ROOT / p) for p in (
                     "examples/perf_decode.rs", "crates/am-camera/src/media/codec.rs", "Cargo.lock")},
                 toolchain=command("rustc", "-Vv"))
    hardware = dict(os=platform.platform(), cpu=platform.processor())
    output.mkdir(parents=True)
    summaries = []
    for index in range(3):
        raw = json.loads(command(str(exe), str(args.runtime_dir.resolve()), str(args.input.resolve()),
                                str(args.samples), str(args.warmup)))
        samples = raw.pop("samples_ns")
        report = dict(schema=SCHEMA, build=build,
            identity=dict(workload=dict(codec="h264-single-idr", width=raw["width"], height=raw["height"], output="rgba"),
                input_sha256=file_sha256(args.input), model_sha256="not-applicable:decode",
                runtime_sha256=hashlib.sha256(json.dumps(runtimes, sort_keys=True).encode()).hexdigest(),
                hardware=hardware, backend="ffmpeg-cpu", threads=1,
                budgets=dict(max_pixels=8192*8192, output_bytes=8192*8192*4), warmup=args.warmup,
                timing_boundary="copy_rgba only; peek/decode and hash excluded; measured duration sum throughput"),
            counts=dict(attempted=len(samples), delivered=len(samples), rejected=0, dropped=0),
            elapsed_ns=sum(samples), samples_ns=samples,
            output_checksum_fnv1a64=raw.pop("checksum_fnv1a64"), runtime_files=runtimes, **raw)
        validate(report)
        assert report["contexts_created"] == report["contexts_freed"], "conversion context leak"
        (output / f"round-{index+1}.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
        summaries.append(dict(conversion=summarize(samples),
                              decode=summarize(report["decode_samples_ns"]),
                              contexts_created=report["contexts_created"], contexts_freed=report["contexts_freed"]))
    print(json.dumps(dict(output=str(output), rounds=summaries), indent=2))


if __name__ == "__main__":
    main()
