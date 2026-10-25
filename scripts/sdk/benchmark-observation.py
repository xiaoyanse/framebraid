"""测量单调钟及实际DLL stats查询/释放开销，不从生产计时中盲减。"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time
from perf_common import file_sha256, summarize

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("tensor_test", Path(__file__).with_name("test-tensors.py"))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
a, size, check = t.a, t.size, t.check
ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dll-dir", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to((ROOT / ".local/evidence").resolve()):
        raise SystemExit("use a new output directory below .local/evidence")
    lib = c.CDLL(str((args.dll_dir / "am_vision.dll").resolve()))
    a.bind(lib, "amv_", require_all=True)
    ctx = t.base.initialize(lib, args.runtime_dir)
    model = None
    try:
        model = t.load(lib, ctx, args.model)
        rounds = []
        for _ in range(3):
            clock, query = [], []
            for i in range(2020):
                start = time.perf_counter_ns()
                timer_ns = time.perf_counter_ns() - start
                start = time.perf_counter_ns()
                pointer = c.POINTER(a.am_stats_t)()
                check(lib.amv_get_stats(ctx, None, c.byref(pointer)))
                check(lib.amv_stats_release(pointer))
                query_ns = time.perf_counter_ns() - start
                if i >= 20:
                    clock.append(timer_ns)
                    query.append(query_ns)
            rounds.append(dict(clock_samples_ns=clock, stats_samples_ns=query,
                               clock=summarize(clock), stats=summarize(query)))
        version = size(a.am_version_info_t)
        check(lib.amv_get_version_info(c.byref(version)))
        report = dict(source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                      artifact_sha256=file_sha256(args.dll_dir/"am_vision.dll"),
                      model_sha256=file_sha256(args.model),
                      runtime_sha256=file_sha256(args.runtime_dir/"onnxruntime.dll"),
                      loaded_notes=version.runtime_notes_utf8.decode(), rounds=rounds,
                      scope="Python timer + actual stats query/release; loaded idle Identity model; not loaded-state observation overhead")
        output.mkdir(parents=True)
        (output/"observation.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
        print(json.dumps(dict(output=str(output), rounds=[dict(clock=r["clock"], stats=r["stats"]) for r in rounds]), indent=2))
    finally:
        if model:
            t.base.close_model(lib, model)
        t.base.shared.close(lib, ctx, False)


if __name__ == "__main__":
    main()
