"""MET-2 S1单路真实GOP定拍解码A/A及CPU/硬件取舍；不连接现场或自动宣称收益。"""
import argparse
import ctypes as c
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

from perf_common import dll_build_identity, file_sha256, summarize
from perf_met2 import review_stream_rows, validate_stream
from perf_resources import ResourceSampler, available_parallelism, cpu_delta, cpu_snapshot
from perf_stream_input import load_packets

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
import amrt_sdk as a


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def checked(code):
    if code != a.AM_OK:
        raise RuntimeError(f"SDK error {code}")


def close(lib, obj, function):
    state = c.c_uint32()
    checked(function(obj, 1000, c.byref(state)))
    if state.value != a.AM_LIFECYCLE_CLOSED:
        raise RuntimeError("SDK close did not drain")
    checked(lib.amc_object_destroy(obj))


class Decoder:
    def __init__(self, lib, runtime, stream, policy):
        self.lib, self.ctx, self.decoder = lib, c.POINTER(a.amc_context_t)(), c.POINTER(a.amc_decoder_t)()
        directories = (c.c_char_p * 1)(str(runtime.resolve()).encode())
        params = sized(a.amc_init_params_t)
        params.abi_version = a.AM_ABI_VERSION
        params.runtime_dirs_utf8, params.runtime_dir_count = directories, 1
        checked(lib.amc_init(c.byref(params), c.byref(self.ctx)))
        try:
            config = sized(a.am_decoder_config_t)
            config.codec, config.output_pixel_format = a.AM_CODEC_H264, a.AM_PF_RGBA8
            config.hw_policy = a.AM_DHW_SOFTWARE_ONLY if policy == "cpu" else a.AM_DHW_PREFER_HW
            config.queue_max_items, config.queue_max_bytes = 32, 16 * 1024 * 1024
            config.time_base_num, config.time_base_den = map(int, stream["time_base"].split("/"))
            checked(lib.amc_decoder_create(self.ctx, c.byref(config), c.byref(self.decoder)))
        except BaseException:
            close(lib, self.ctx, lib.amc_shutdown)
            raise

    def finish(self):
        try:
            close(self.lib, self.decoder, self.lib.amc_decoder_close)
        finally:
            close(self.lib, self.ctx, self.lib.amc_shutdown)


def run_window(lib, args, stream, packets, policy, observed):
    decoder = Decoder(lib, args.runtime_dir, stream, policy)
    sampler = ResourceSampler(enabled=observed)
    received, pending, samples, late, hashes = set(), {}, [], [], []
    width, height = stream["width"], stream["height"]
    parallelism, parallelism_method = available_parallelism()
    owners = [(c.c_ubyte * len(row["data"])).from_buffer_copy(row["data"]) for row in packets]
    per_frame = []
    first_dts = packets[0]["dts"]
    timebase_num, timebase_den = map(int, stream["time_base"].split("/"))
    start_wall = time.perf_counter_ns()
    measurement_start = measurement_end = None
    phase_start, phase_end = args.warmup_s, args.warmup_s + args.window_s
    phase_input = 0
    opening_inflight = window_outputs = 0
    before = after = None
    observation = None

    def receive_all(draining=False):
        nonlocal window_outputs
        while True:
            frame = c.POINTER(a.amc_frame_t)()
            code = lib.amc_decoder_receive(decoder.decoder, 0, c.byref(frame))
            if code == a.AM_ERR_SDK_TRY_AGAIN and not draining:
                return
            if code == a.AM_ERR_SDK_DRAINED and draining:
                return
            checked(code)
            try:
                meta, view = sized(a.am_frame_meta_t), sized(a.am_frame_view_t)
                checked(lib.amc_frame_get_meta(frame, c.byref(meta)))
                checked(lib.amc_frame_get_view(frame, c.byref(view)))
                now = time.perf_counter_ns()
                if meta.pts not in pending or meta.pts in received:
                    raise RuntimeError("duplicate or uncorrelated output PTS")
                if view.width != width or view.height != height or view.format != a.AM_PF_RGBA8:
                    raise RuntimeError("output contract changed")
                if meta.source_generation != 1:
                    raise RuntimeError("decoder changed source generation during stable window")
                submitted, phase = pending.pop(meta.pts)
                received.add(meta.pts)
                latency = now - submitted
                if before is not None and measurement_end is None:
                    window_outputs += 1
                # 有效性校验不复制整个RGBA面；这不是像素/模型精度验收。
                if not view.planes[0] or view.plane_size_bytes[0] < width * height * 4:
                    raise RuntimeError("invalid pixel view extent")
                if phase:
                    per_frame.append(dict(pts=meta.pts, latency_ns=latency,
                        before_window_end=measurement_end is None))
                    if measurement_end is None:
                        samples.append(latency)
                if len(hashes) < 2:
                    hashes.append(hashlib.sha256(c.string_at(view.planes[0], min(4096, view.plane_size_bytes[0]))).hexdigest())
            finally:
                checked(lib.amc_frame_release(frame))

    try:
        for row, owner in zip(packets, owners):
            offset_s = (row["dts"] - first_dts) * timebase_num / timebase_den
            if offset_s >= phase_end:
                break
            if before is None and offset_s >= phase_start:
                # 先完成观察器初始化，再重新锚定节拍，避免把初始化时间当输入抖动。
                sampler.start()
                start_wall += time.perf_counter_ns() - (start_wall + int(offset_s * 1e9))
                before = cpu_snapshot()
                measurement_start = before["wall_ns"]
                opening_inflight = len(pending)
            target = start_wall + int(offset_s * 1e9)
            remaining = target - time.perf_counter_ns()
            if remaining > 0:
                time.sleep(remaining / 1e9)
            now = time.perf_counter_ns()
            phase = before is not None
            if phase:
                late.append(max(0, now - target))
                phase_input += 1
            pending[row["pts"]] = now, phase
            packet = sized(a.am_decoder_input_t)
            packet.data, packet.size, packet.flags = owner, len(owner), a.AM_PKT_AU_COMPLETE
            packet.pts, packet.dts = row["pts"], row["dts"]
            checked(lib.amc_decoder_submit(decoder.decoder, c.byref(packet)))
            receive_all()
        if before is None:
            raise RuntimeError("fixture too short for warmup")
        # 稳态固定墙钟结束；B帧尾项在窗口外排空，不能冒充窗口内交付。
        target_end = start_wall + int(phase_end * 1e9)
        remaining = target_end - time.perf_counter_ns()
        if remaining > 0:
            time.sleep(remaining / 1e9)
        after = cpu_snapshot()
        measurement_end = after["wall_ns"]
        observation = sampler.stop()
        closing_pending = len(pending)
        if opening_inflight + phase_input != window_outputs + closing_pending:
            raise RuntimeError("measurement window did not conserve input/output/inflight counts")
        checked(lib.amc_decoder_drain(decoder.decoder))
        receive_all(draining=True)
        if pending:
            raise RuntimeError("drained decoder lost submitted frames")
        info = sized(a.am_decoder_info_t)
        checked(lib.amc_decoder_get_info(decoder.decoder, c.byref(info)))
        actual = info.backend_name_utf8.decode()
        if (policy == "cpu" and info.hw_accelerated) or (policy == "hardware" and not info.hw_accelerated):
            raise RuntimeError(f"requested {policy} but observed {actual}; no silent control substitution")
        useful = sum(value <= args.age_budget_ms * 1e6 for value in samples)
        resources = cpu_delta(before, after, parallelism)
        return dict(policy=policy, actual_backend=actual,
            fixture_b_frames=stream.get("has_b_frames"),
            window_start_ns=measurement_start, window_end_ns=measurement_end,
            cpu=dict(resources, parallelism_method=parallelism_method), observation=observation,
            counts=dict(window_inputs=phase_input, measured_input_outputs_before_end=len(samples),
                all_outputs_before_end=window_outputs,
                measured_input_outputs_after_end=sum(not row["before_window_end"] for row in per_frame),
                useful_before_end=useful, expired_before_end=len(samples)-useful,
                all_submitted_drained=len(received), closing_inflight=closing_pending,
                opening_inflight=opening_inflight, rejected=0, dropped=0),
            latency=summarize(samples), submit_schedule_lateness=summarize(late),
            samples_ns=samples, frames=per_frame, useful_fps=useful/resources["wall_s"],
            pixel_prefix_sha256=hashes,
            unknown=dict(pixel_accuracy="not assessed; prefix hashes are diagnostic, not cross-backend equality",
                         exposure_age=None, network_jitter=None, internal_decode_transfer_split=None))
    finally:
        if observation is None:
            sampler.stop()
        decoder.finish()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("dll-dir", "runtime-dir", "input", "output", "target-card"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--warmup-s", type=float, default=2)
    parser.add_argument("--window-s", type=float, default=6)
    parser.add_argument("--age-budget-ms", type=float, default=200)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--policies", nargs="+", choices=("cpu", "hardware"), default=["cpu", "hardware"])
    parser.add_argument("--observation-controls", action="store_true")
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to((ROOT / ".local/evidence").resolve()):
        raise SystemExit("use a new output directory below .local/evidence")
    if not 1 <= args.rounds <= 5 or not .5 <= args.warmup_s <= 10 or not 2 <= args.window_s <= 30 or not 1 <= args.age_budget_ms <= 5000:
        raise SystemExit("experiment time/sample budget exceeded")
    if not args.target_card.is_file():
        raise SystemExit("predeclared target card required")
    stream, packets = load_packets(args.input)
    num, den = map(int, stream["time_base"].split("/"))
    if (packets[-1]["dts"] - packets[0]["dts"]) * num / den < args.warmup_s + args.window_s:
        raise SystemExit("fixture does not cover full fixed window")
    output.mkdir(parents=True)
    directory = os.add_dll_directory(str(args.dll_dir.resolve()))
    try:
        dll = args.dll_dir.resolve() / "am_camera.dll"
        lib = c.CDLL(str(dll))
        a.bind(lib, "amc_", require_all=True)
        version = sized(a.am_version_info_t)
        checked(lib.amc_get_version_info(c.byref(version)))
        build = dll_build_identity(dll, version.package_version_utf8.decode(), version.build_target_utf8.decode(),
            version.runtime_notes_utf8.decode(), subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "dirty" if subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT) else "clean")
        identity = dict(schema="framebraid.performance.met2-stream/1", build=build,
            input_sha256=file_sha256(args.input), stream=stream,
            target_card_sha256=file_sha256(args.target_card),
            runtime_files={p.name: file_sha256(p) for p in sorted(args.runtime_dir.glob("*.dll"))},
            observer_files={name: file_sha256(Path(__file__).with_name(name)) for name in
                            ("benchmark-decode-stream.py", "perf_resources.py", "perf_stream_input.py", "perf_common.py", "perf_met2.py")},
            machine=dict(os=platform.platform(), cpu=platform.processor()),
            protocol=dict(warmup_s=args.warmup_s, window_s=args.window_s, age_budget_ms=args.age_budget_ms,
                          rounds=args.rounds, paced_by="DTS, CPU perf_counter clock; no network or display"),
            acceptance="measurement only; review target card and A/A before claiming gains")
        (output / "identity.json").write_text(json.dumps(identity, indent=2) + "\n", encoding="utf-8")
        sources = output / "observer-sources"
        sources.mkdir()
        for name in identity["observer_files"]:
            (sources / name).write_bytes(Path(__file__).with_name(name).read_bytes())
        result, rows = [], []
        for index in range(args.rounds):
            for policy in args.policies[::1 if index % 2 == 0 else -1]:
                name = f"{policy}-{index+1}.json"
                try:
                    row = run_window(lib, args, stream, packets, policy, True)
                    validate_stream(row)
                except Exception as error:
                    (output / "failure.json").write_text(json.dumps(dict(case=name,
                        error=type(error).__name__ + ": " + str(error))), encoding="utf-8")
                    raise
                (output / name).write_text(json.dumps(row, indent=2) + "\n", encoding="utf-8")
                rows.append(row)
                result.append(dict(file=name, cpu_s=row["cpu"]["cpu_s"], useful_fps=row["useful_fps"],
                                   latency_p95_ms=row["latency"]["p95_ms"], actual_backend=row["actual_backend"]))
        if args.observation_controls:
            for policy in args.policies:
                row = run_window(lib, args, stream, packets, policy, False)
                name = f"{policy}-observation-off.json"
                (output / name).write_text(json.dumps(row, indent=2) + "\n", encoding="utf-8")
        (output / "review.json").write_text(json.dumps(review_stream_rows(rows, args.window_s), indent=2) + "\n", encoding="utf-8")
        (output / "summary.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(dict(output=str(output), rounds=result), indent=2))
    finally:
        directory.close()


if __name__ == "__main__":
    main()
