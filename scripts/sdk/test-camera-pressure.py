"""真实回环源的停读订阅、阻塞回调及健康消费者隔离；不连接现场。"""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
from camera_recovery_fixture import a, sized, checked, close, until, Stream, Session, stats, media_workers, lifecycle, sdk_clock_ms, t
from local_paths import local_path


def observe(session, seconds):
    rows, packets, samples = [], 0, []
    started = time.perf_counter()
    received_after = sdk_clock_ms()
    pre_phase_frames = 0
    while time.perf_counter() - started < seconds:
        frame = session.pull_frame(50)
        if frame and frame["received_ms"] < received_after:
            pre_phase_frames += 1
            frame = None
        if frame:
            frame["elapsed_ms"] = (time.perf_counter() - started) * 1000
            frame["age_ms"] = sdk_clock_ms() - frame["received_ms"]
            rows.append(frame)
        packet = session.pull_packet()
        packets += bool(packet)
        samples.append(stats(session.lib, session.ctx))
    elapsed = time.perf_counter() - started
    assert rows and packets
    assert len({r["generation"] for r in rows}) == 1
    assert [r["frame_id"] for r in rows] == sorted({r["frame_id"] for r in rows})
    gaps = [y["elapsed_ms"] - x["elapsed_ms"] for x, y in zip(rows, rows[1:])]
    summary = {"seconds": elapsed, "frames": len(rows), "packets": packets,
               "pre_phase_frames_excluded": pre_phase_frames,
               "fps": len(rows) / elapsed, "max_age_ms": max(r["age_ms"] for r in rows),
               "max_gap_ms": max(gaps, default=0),
               "queue_items_peak": max(s["queue_items"] for s in samples),
               "queue_bytes_peak": max(s["queue_bytes"] for s in samples),
               "first_stats": samples[0], "last_stats": samples[-1]}
    assert summary["fps"] >= 7.5, summary
    assert summary["max_age_ms"] <= 750 and summary["max_gap_ms"] <= 750, summary
    return {"summary": summary, "frames": rows, "stats": samples}


def run(lib, ctx, stream, work, seconds):
    session = Session(lib, ctx, stream)
    slow_encoded = c.POINTER(a.amc_encoded_sub_t)()
    slow_frames = c.POINTER(a.amc_frame_sub_t)()
    gate, frame_entered, event_entered = threading.Event(), threading.Event(), threading.Event()
    calls, errors = {"frame": [], "event": 0}, []
    try:
        until(lambda: session.pull_frame())
        result = {"baseline": observe(session, seconds)}
        (work / "baseline.json").write_text(json.dumps(result["baseline"], indent=2), encoding="utf-8")
        # 只在一个真实源上增加订阅，排除重复网络连接带来的伪隔离。
        desc = sized(a.am_encoded_sub_desc_t)
        desc.codec_mask, desc.mode, desc.queue_max_items, desc.queue_max_bytes = a.AM_CODEC_H264, a.AM_SUB_PULL, 2, 1024 * 1024
        checked(lib.amc_subscribe_encoded(session.source, c.byref(desc), c.byref(slow_encoded)))
        frames = sized(a.am_frame_sub_desc_t)
        frames.pixel_format, frames.sampling_max_fps = a.AM_PF_RGBA8, 10
        frames.mode, frames.queue_max_items, frames.queue_max_bytes = a.AM_SUB_PUSH, 2, 1024 * 1024
        checked(lib.amc_subscribe_frames(session.source, c.byref(frames), c.byref(slow_frames)))

        @a.am_frame_cb_fn
        def blocked_frame(sub, pointer, user):
            try:
                calls["frame"].append(session.frame(pointer))
                if not frame_entered.is_set():
                    frame_entered.set()
                    assert gate.wait(30), "owned slow frame gate deadline"
            except Exception as error:
                errors.append(str(error))

        @a.am_source_event_fn
        def blocked_event(src, kind, detail, user):
            try:
                assert detail
                calls["event"] += 1
                if not event_entered.is_set():
                    event_entered.set()
                    assert gate.wait(30), "owned slow event gate deadline"
            except Exception as error:
                errors.append(str(error))

        checked(lib.amc_source_set_event_callback(session.source, blocked_event, None))
        checked(lib.amc_frame_set_callback(slow_frames, blocked_frame, None))
        until(lambda: frame_entered.is_set() and event_entered.is_set())
        result["pressure"] = observe(session, seconds)
        (work / "pressure.json").write_text(json.dumps(result["pressure"], indent=2), encoding="utf-8")
        assert len(calls["frame"]) == 1 and calls["event"] == 1
        before = result["baseline"]["summary"]["last_stats"]
        after = result["pressure"]["summary"]["last_stats"]
        assert after["resync_count"] > before["resync_count"]
        assert after["frames_dropped"] > before["frames_dropped"]
        assert after["reconnect_count"] == before["reconnect_count"]
        # 四条订阅各自有界，像素输入最多36包；聚合观察不是进程RSS。
        assert result["pressure"]["summary"]["queue_items_peak"] <= 50
        assert result["pressure"]["summary"]["queue_bytes_peak"] <= 12 * 1024 * 1024
        released_ms = sdk_clock_ms()
        gate.set()
        until(lambda: any(f["received_ms"] >= released_ms for f in calls["frame"]))
        checked(lib.amc_frame_set_callback(slow_frames, a.am_frame_cb_fn(), None))
        count = len(calls["frame"])
        recovered = c.POINTER(a.am_encoded_packet_t)()
        def packet():
            code = lib.amc_encoded_pull(slow_encoded, 100, c.byref(recovered))
            if code in (a.AM_ERR_SDK_TRY_AGAIN, a.AM_ERR_SDK_DEADLINE_EXCEEDED):
                return False
            checked(code)
            return True
        until(packet)
        try:
            value = recovered.contents
            assert value.flags & a.AM_PKT_KEYFRAME
            assert {5, 7, 8}.issubset(t.nal_types(c.string_at(value.data, value.size), a.AM_CODEC_H264))
        finally:
            checked(lib.amc_encoded_packet_release(recovered))
        close(lib, slow_encoded, lib.amc_encoded_unsubscribe)
        slow_encoded = c.POINTER(a.amc_encoded_sub_t)()
        close(lib, slow_frames, lib.amc_frame_unsubscribe)
        slow_frames = c.POINTER(a.amc_frame_sub_t)()
        result["recovery"] = observe(session, seconds)
        assert len(calls["frame"]) == count and not errors, errors
        result["callbacks"] = {"frames": count, "events": calls["event"], "errors": errors}
        result["slow_encoded_resumed_at_keyframe"] = True
        return result
    finally:
        gate.set()
        if slow_frames:
            close(lib, slow_frames, lib.amc_frame_unsubscribe)
        if slow_encoded:
            close(lib, slow_encoded, lib.amc_encoded_unsubscribe)
        session.close()
        (work / "drained.json").write_text(json.dumps(stats(lib, ctx), indent=2), encoding="utf-8")


def main(args):
    work = local_path(args.work_dir.resolve(), "tests", fresh=True)
    work.mkdir(parents=True)
    target = {"source": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
              "metric": "MET-2 M-RECOVER S4/S5", "phase_seconds": args.seconds,
              "input_fps": 10, "healthy_min_fps": 7.5, "healthy_max_age_gap_ms": 750,
              "queue_items_max": 50, "queue_bytes_max": 12 * 1024 * 1024,
              "scope": "one real H264 software source; shared network and independent subscribers; not hardware/soak"}
    (work / "target-card.json").write_text(json.dumps(target, indent=2), encoding="utf-8")
    dll = args.dll_dir.resolve() / "am_camera.dll"
    lib = c.CDLL(str(dll))
    a.bind(lib, "amc_", require_all=True)
    params = sized(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    paths = (c.c_char_p * 1)(str(args.runtime_dir.resolve()).encode())
    params.runtime_dirs_utf8, params.runtime_dir_count = paths, 1
    ctx = c.POINTER(a.amc_context_t)()
    checked(lib.amc_init(c.byref(params), c.byref(ctx)))
    try:
        with Stream(work, args.mediamtx.resolve()) as stream:
            result = run(lib, ctx, stream, work, args.seconds)
            assert not media_workers(lifecycle.windows())
            drained = stats(lib, ctx)
            assert all(drained[k] == 0 for k in ["queue_items", "queue_bytes", "external_held_items", "external_held_bytes"]), drained
    except BaseException as error:
        (work / "failure.json").write_text(json.dumps({"type": type(error).__name__, "message": str(error)}, indent=2), encoding="utf-8")
        raise
    finally:
        close(lib, ctx, lib.amc_shutdown)
    result["identity"] = {"source": target["source"], "dll_sha256": hashlib.sha256(dll.read_bytes()).hexdigest()}
    result["passed"] = True
    (work / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps({"passed": True, "phases": {k: result[k]["summary"] for k in ["baseline", "pressure", "recovery"]}}))


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    for name in ["dll-dir", "runtime-dir", "mediamtx", "work-dir"]:
        p.add_argument("--" + name, type=Path, required=True)
    p.add_argument("--seconds", type=float, default=4)
    args = p.parse_args()
    assert 2 <= args.seconds <= 10
    main(args)
