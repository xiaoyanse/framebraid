"""M1 收尾 MET-2：真实 Pose 模型 CPU vs CUDA 同步推理对照；每轮独立进程加载防缓存污染。"""
import argparse
import ctypes as c
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parent))
import perf_common  # noqa: F401  # 建立本地工具路径环境

spec = importlib.util.spec_from_file_location("tb", Path(__file__).with_name("test-batch.py"))
tb = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tb)
t, base, a, size, check = tb.t, tb.base, tb.a, tb.size, tb.check


def run_round(dll_dir, ort_dir, model_path, image_path, backend, samples):
    """backend: 0=CPU 2=CUDA（经子进程隔离执行，返回 JSON 行）。"""
    if os.environ.get("AM_GPU_CHILD") != "1":
        env = dict(os.environ, AM_GPU_CHILD="1")
        cmd = [sys.executable, "-B", str(Path(__file__).resolve()), "--child",
               "--dll-dir", str(dll_dir), "--runtime-dir", str(ort_dir),
               "--model", str(model_path), "--image", str(image_path),
               "--backend", str(backend), "--samples", str(samples)]
        done = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=300)
        for line in done.stdout.splitlines():
            if line.startswith("RESULT "):
                return json.loads(line[7:])
        raise RuntimeError("child failed: " + done.stdout[-200:] + done.stderr[-300:])

    search = os.add_dll_directory(str(Path(dll_dir).resolve()))
    try:
        vis = c.CDLL(str(Path(dll_dir).resolve() / "am_vision.dll"))
        a.bind(vis, "amv_", require_all=True)
        ctx = c.POINTER(a.amv_context_t)()
        dirs = (c.c_char_p * 1)(str(Path(ort_dir).resolve()).encode())
        init = size(a.amv_init_params_t)
        init.abi_version, init.runtime_dirs_utf8, init.runtime_dir_count = a.AM_ABI_VERSION, dirs, 1
        check(vis.amv_init(c.byref(init), c.byref(ctx)))

        policy = size(a.am_backend_policy_t)
        policy.requested, policy.strict = backend, 0
        md = size(a.am_model_desc_t)
        md.path_utf8 = str(Path(model_path).resolve()).encode()
        md.profile_id = b"pose-yolo26-e2e-coco17-v1"
        md.load_mode, md.backend, md.intra_threads = a.AM_LOAD_PROFILE, c.pointer(policy), 2
        model = c.POINTER(a.amv_model_t)()
        load_start = time.perf_counter()
        check(vis.amv_model_load(ctx, c.byref(md), c.byref(model)))
        load_ms = (time.perf_counter() - load_start) * 1000

        # 图像输入（inference.py 的输入构造）——真实 PNG 图像走 amv_infer_image_sync
        from test_inference_helper import load_image_input  # 本地 helper（见文件尾）
        image_input, params = load_image_input(vis, a, size, image_path)

        latencies, out_hash = [], None
        for i in range(samples):
            result = c.POINTER(a.amv_result_t)()
            rid = c.c_uint64()
            start = time.perf_counter()
            check(vis.amv_infer_image_sync(model, c.byref(image_input), c.byref(params), c.byref(result), c.byref(rid)))
            latencies.append((time.perf_counter() - start) * 1000)
            if i == samples - 1:
                out_hash = hash_result(a, result)
            check(vis.amv_result_release(result))
            check(vis.amv_result_ack(ctx, rid.value))

        info = size(a.am_model_info_t)
        check(vis.amv_model_get_info(model, c.byref(info)))
        matrix = size(a.am_backend_matrix_t)
        check(vis.amv_query_backends(ctx, c.byref(matrix)))
        cuda_row = matrix.rows[2]
        actual = {0: "cpu", 2: "cuda"}[backend]
        row = dict(backend=actual, load_ms=round(load_ms, 1),
                   p50=round(sorted(latencies)[len(latencies) // 2], 3),
                   p95=round(sorted(latencies)[int(len(latencies) * .95) - 1], 3),
                   min_ms=round(min(latencies), 3), max_ms=round(max(latencies), 3),
                   samples=len(latencies), out_hash=out_hash,
                   matrix_verified=[cuda_row.verified if backend == 2 else matrix.rows[0].verified][0])
        print("RESULT " + json.dumps(row), flush=True)

        base.close_model(vis, model)
        state = c.c_uint32()
        vis.amv_shutdown(ctx, 100, c.byref(state))
    finally:
        search.close()


def hash_result(a, result):
    """简化输出指纹：raw 输出字节数 + 首尾值哈希（对拍足够）。"""
    import hashlib
    value = result.contents
    if value.raw_output_count == 0:
        return "no-raw"
    raw = value.raw_outputs[0]
    blob = c.string_at(raw.data, min(64, raw.data_size))
    return hashlib.sha256(bytes(blob) + str(raw.data_size).encode()).hexdigest()[:16]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("dll-dir", "model", "image"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path)
    parser.add_argument("--samples", type=int, default=20)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--child", action="store_true")
    parser.add_argument("--backend", type=int)
    args = parser.parse_args()

    if args.child:
        run_round(args.dll_dir, args.runtime_dir, args.model, args.image,
                  int(os.environ["AM_GPU_BACKEND"]), args.samples)
        return

    from perf_common import file_sha256
    rows = []
    cpu_ort = r"E:/projects/framebraid/.local/releases/candidates/v0.1.0/windows-x64/runtime/onnxruntime-cpu"
    gpu_ort = r"E:/projects/framebraid/.local/runtimes/ort-gpu-131-final"
    identity = dict(model_sha256=file_sha256(args.model), input_sha256=file_sha256(args.image),
                    scope="CPU vs CUDA real Pose sync inference, alternating rounds, child-process isolated")
    print(json.dumps(identity))
    order = [(0, cpu_ort), (2, gpu_ort)]
    for r in range(args.rounds):
        for backend, ort in order if r % 2 == 0 else reversed(order):
            os.environ["AM_GPU_BACKEND"] = str(backend)
            row = run_round(args.dll_dir, ort, args.model, args.image, backend, args.samples)
            row["round"] = r + 1
            rows.append(row)
            print(json.dumps(row))
    # 输出对拍：CPU 与 CUDA 的 out_hash 是否一致（可能因 EP 数值差异不同——如实记录）
    cpu_hashes = {r["out_hash"] for r in rows if r["backend"] == "cpu"}
    cuda_hashes = {r["out_hash"] for r in rows if r["backend"] == "cuda"}
    summary = dict(rows=rows, output_match=(cpu_hashes == cuda_hashes and bool(cpu_hashes)),
                   cpu_hashes=sorted(cpu_hashes), cuda_hashes=sorted(cuda_hashes))
    out_dir = Path(r"E:/projects/framebraid/.local/evidence/gpu-m1-met2-20261028/run1")
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "rounds.json").write_text(json.dumps(summary, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(summary["rows"], indent=1))


if __name__ == "__main__":
    main()
