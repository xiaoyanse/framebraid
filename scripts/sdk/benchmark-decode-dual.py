"""MET-2 同进程双路定拍解码 CPU/硬件取舍；单路对照见 benchmark-decode-stream。不连接现场。"""
import argparse
import ctypes as c
import hashlib
import importlib.util
import json
import subprocess
import os
from pathlib import Path
import time

from perf_common import dll_build_identity, file_sha256, summarize
from perf_met2 import validate_stream
from perf_resources import ResourceSampler, available_parallelism, cpu_delta, cpu_snapshot
from perf_stream_input import load_packets

spec = importlib.util.spec_from_file_location("single", Path(__file__).with_name("benchmark-decode-stream.py"))
single = importlib.util.module_from_spec(spec)
spec.loader.exec_module(single)
a = single.a
sized, checked, close, Decoder = single.sized, single.checked, single.close, single.Decoder


def run_dual_window(lib, args, stream, packets, policies):
    """多路独立解码器同相位定拍；每路独立延迟分账，CPU/观察器为全进程合并。"""
    lanes_total = len(policies)
    assert all(len(lane_packets) for lane_packets in packets)
    parallelism, parallelism_method = available_parallelism()
    sampler = ResourceSampler(enabled=args.observation_controls)
    decoders, pendings, received, samples, lateness = [], [], [], [], []
    window_counts = [dict(outputs=0, inputs=0, opening=0) for _ in range(lanes_total)]
    hashes = []
    state = dict(before=None, measurement_end=None)
    observation = None
    timebase_num, timebase_den = map(int, stream["time_base"].split("/"))
    owners = [[(c.c_ubyte * len(row["data"])).from_buffer_copy(row["data"])
               for row in lane_packets] for lane_packets in packets]
    for policy in policies:
        decoders.append(Decoder(lib, args.runtime_dir, stream, policy))
        pendings.append({})
        received.append(set())
        samples.append([])
        lateness.append([])
        hashes.append([])
    try:
        first_dts = min(row["dts"] for lane_packets in packets for row in lane_packets)
        timeline = []
        for lane, lane_packets in enumerate(packets):
            for lane_index, row in enumerate(lane_packets):
                timeline.append((row["dts"] - first_dts, lane, lane_index, row))
        timeline.sort(key=lambda item: item[:3])
        warmup, window = args.warmup_s, args.window_s
        start_wall = time.perf_counter_ns()

        def receive_all(draining=False):
            for index, decoder in enumerate(decoders):
                while True:
                    frame = c.POINTER(a.amc_frame_t)()
                    code = lib.amc_decoder_receive(decoder.decoder, 0, c.byref(frame))
                    if code == a.AM_ERR_SDK_TRY_AGAIN and not draining:
                        break
                    if code == a.AM_ERR_SDK_DRAINED and draining:
                        break
                    checked(code)
                    try:
                        meta, view = sized(a.am_frame_meta_t), sized(a.am_frame_view_t)
                        checked(lib.amc_frame_get_meta(frame, c.byref(meta)))
                        checked(lib.amc_frame_get_view(frame, c.byref(view)))
                        now = time.perf_counter_ns()
                        if meta.pts not in pendings[index] or meta.pts in received[index]:
                            raise RuntimeError("duplicate or uncorrelated output PTS")
                        if meta.source_generation != 1:
                            raise RuntimeError("decoder changed source generation during stable window")
                        submitted, phase = pendings[index].pop(meta.pts)
                        received[index].add(meta.pts)
                        if phase and state["measurement_end"] is None:
                            window_counts[index]["outputs"] += 1
                        if phase:
                            latency = now - submitted
                            samples[index].append(latency)
                            if len(hashes[index]) < 2:
                                hashes[index].append(hashlib.sha256(c.string_at(
                                    view.planes[0], min(4096, view.plane_size_bytes[0]))).hexdigest())
                    finally:
                        checked(lib.amc_frame_release(frame))

        for offset, lane, lane_index, row in timeline:
            offset_s = offset * timebase_num / timebase_den
            if offset_s >= warmup + window:
                break
            if state["before"] is None and offset_s >= warmup:
                sampler.start()
                state["before"] = cpu_snapshot()
                for lane_state in window_counts:
                    lane_state["opening"] = len(pendings[window_counts.index(lane_state)])
            target = start_wall + int(offset_s * 1e9)
            remaining = target - time.perf_counter_ns()
            if remaining > 0:
                time.sleep(remaining / 1e9)
            now = time.perf_counter_ns()
            phase = state["before"] is not None
            if phase:
                lateness[lane].append(max(0, now - target))
                window_counts[lane]["inputs"] += 1
            pendings[lane][row["pts"]] = now, phase
            packet, owner = sized(a.am_decoder_input_t), owners[lane][lane_index]
            packet.data, packet.size = owner, len(owner)
            packet.flags = a.AM_PKT_AU_COMPLETE
            packet.pts, packet.dts = row["pts"], row["dts"]
            checked(lib.amc_decoder_submit(decoders[lane].decoder, c.byref(packet)))
            receive_all()
        if state["before"] is None:
            raise RuntimeError("fixture too short for warmup")
        remaining = start_wall + int((warmup + window) * 1e9) - time.perf_counter_ns()
        if remaining > 0:
            time.sleep(remaining / 1e9)
        after = cpu_snapshot()
        state["measurement_end"] = after["wall_ns"]
        observation = sampler.stop()
        for index, decoder in enumerate(decoders):
            expected = window_counts[index]["inputs"] - window_counts[index]["outputs"]
            if len(pendings[index]) < expected:
                raise RuntimeError("measurement window did not conserve counts")
        lanes = []
        for index, decoder in enumerate(decoders):
            checked(lib.amc_decoder_drain(decoder.decoder))
        receive_all(draining=True)
        for index, decoder in enumerate(decoders):
            if pendings[index]:
                raise RuntimeError("drained decoder lost submitted frames")
        for index, decoder in enumerate(decoders):
            info = sized(a.am_decoder_info_t)
            checked(lib.amc_decoder_get_info(decoder.decoder, c.byref(info)))
            actual = info.backend_name_utf8.decode()
            if (policies[index] == "cpu" and info.hw_accelerated) or \
               (policies[index] == "hardware" and not info.hw_accelerated):
                raise RuntimeError(f"lane{index} requested {policies[index]} but observed {actual}")
            useful = sum(value <= args.age_budget_ms * 1e6 for value in samples[index])
            lanes.append(dict(lane=index, policy=policies[index], actual_backend=actual,
                              useful=summarize([v / 1e6 for v in samples[index]]),
                              useful_before_end=useful, expired=len(samples[index]) - useful,
                              inputs=window_counts[index]["inputs"],
                              outputs_before_end=window_counts[index]["outputs"],
                              pixel_prefix_sha256=hashes[index]))
        resources = cpu_delta(state["before"], after, parallelism)
        both_useful = all(lane["useful_before_end"] >= lane["inputs"] * 0.9 for lane in lanes)
        return dict(policies=policies, lanes=lanes,
                    cpu=dict(resources, parallelism_method=parallelism_method),
                    observation=observation,
                    qualified_lanes=sum(lane["useful_before_end"] >= lane["inputs"] * 0.9 for lane in lanes),
                    both_lanes_qualified=bool(both_useful),
                    submit_schedule_lateness_ms=[summarize([v / 1e6 for v in lane]) for lane in lateness])
    finally:
        if observation is None:
            sampler.stop()
        for decoder in decoders:
            decoder.finish()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("dll-dir", "runtime-dir", "input", "output", "target-card"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--warmup-s", type=float, default=2)
    parser.add_argument("--window-s", type=float, default=6)
    parser.add_argument("--age-budget-ms", type=float, default=200)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--policies", nargs="+", choices=("cpu", "hardware"), default=None,
                        help="各路策略（路数由此决定）；默认跑全CPU与全硬件两组")
    parser.add_argument("--lanes", type=int, default=2,
                        help="默认组的路数（--policies 未给时生效）")
    parser.add_argument("--observation-controls", action="store_true")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    stream, packets = load_packets(args.input)
    search = os.add_dll_directory(str(args.dll_dir.resolve()))
    results = []
    try:
        lib = c.CDLL(str(args.dll_dir.resolve() / "am_camera.dll"))
        a.bind(lib, "amc_", require_all=True)
        groups = [args.policies] if args.policies else             [["cpu"] * args.lanes, ["hardware"] * args.lanes]
        for policies in groups:
            for round_index in range(1, args.rounds + 1):
                row = run_dual_window(lib, args, stream, [packets] * len(policies), policies)
                name = f"dual-{'-'.join(policies)}-{round_index}.json"
                (output / name).write_text(json.dumps(row, indent=1) + "\n", encoding="utf-8")
                results.append(dict(file=name,
                                    cpu_s=row["cpu"]["cpu_s"], wall_s=row["cpu"]["wall_s"],
                                    qualified_lanes=row["qualified_lanes"],
                                    lane_useful_fps=[round(lane["useful_before_end"] / row["cpu"]["wall_s"], 3)
                                                     for lane in row["lanes"]],
                                    backends=[lane["actual_backend"] for lane in row["lanes"]]))
        dll = args.dll_dir.resolve() / "am_camera.dll"
        version = sized(a.am_version_info_t)
        checked(lib.amc_get_version_info(c.byref(version)))
        build = dll_build_identity(dll, version.package_version_utf8.decode(), version.build_target_utf8.decode(),
            version.runtime_notes_utf8.decode(), subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=Path(__file__).resolve().parents[2], text=True).strip(),
            "dirty" if subprocess.check_output(["git", "status", "--porcelain"], cwd=Path(__file__).resolve().parents[2]) else "clean")
        identity = dict(schema="framebraid.performance.met2-dual-stream/1", build=build,
                        input_sha256=file_sha256(args.input), stream=stream,
                        target_card_sha256=file_sha256(args.target_card),
                        runtime_files={p.name: file_sha256(p) for p in sorted(args.runtime_dir.glob("*.dll"))},
                        scope="same-process dual decoder stream cadence; MET-2 observation; not field acceptance")
        (output / "identity.json").write_text(json.dumps(identity, indent=1) + "\n", encoding="utf-8")
        print(json.dumps(results, indent=1))
    finally:
        search.close()


if __name__ == "__main__":
    main()
