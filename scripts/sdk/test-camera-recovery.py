"""隔离回环C视频源：断流/认证恢复、真实启停、回调排空与资源记录。"""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from camera_recovery_fixture import a, sized, checked, close, until, Stream, Session, stats, media_workers, resources, lifecycle, sdk_clock_ms, t
from local_paths import local_path


def faults(lib, ctx, stream, count, work):
    session = Session(lib, ctx, stream)
    result = []
    try:
        previous = until(lambda: session.pull_frame())
        for i in range(count):
            old_generation = previous["generation"]
            if i % 2 == 0:
                stream.disconnect()
                until(lambda: session.status().state == a.AM_SRC_RECONNECT_WAIT)
                kind = "disconnect"
            else:
                bad = sized(a.am_credential_data_t)
                bad.username_utf8, bad.password_utf8 = stream.user, b"invalid-synthetic-password"
                checked(lib.amc_credentials_set(session.source, c.byref(bad)))
                until(lambda: session.status().state == a.AM_SRC_AUTH_WAITING)
                assert session.status().fault_class == a.AM_FAULT_AUTH
                attempts = session.status().reconnect_attempts
                time.sleep(0.2)
                assert session.status().state == a.AM_SRC_AUTH_WAITING
                assert session.status().reconnect_attempts == attempts
                kind = "authentication"
            assert session.pull_frame(0) is None, "fault did not clear old frame queue"
            started = time.perf_counter()
            restored_ms = sdk_clock_ms()
            if kind == "disconnect":
                stream.publish()
            else:
                checked(lib.amc_credentials_set(session.source, c.byref(session.creds)))
            recovered = session.recovered(old_generation, started, restored_ms)
            assert recovered["source_id"] == previous["source_id"]
            recovered["fault"] = kind
            result.append(recovered)
            (work / "fault-progress.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
            previous = {"generation": recovered["generation"], "source_id": recovered["source_id"]}
            print(json.dumps({"progress": "fault", "completed": i + 1, "first_ms": round(recovered["first_ms"], 2)}), flush=True)
        return result
    finally:
        session.close()


def cycles(lib, ctx, stream, count, warmup, k, work):
    result, before = [], None
    for i in range(warmup + count):
        if i == warmup:
            before = resources(k)
        session = Session(lib, ctx, stream, push=True, block=i % 10 == 0)
        try:
            until(lambda: session.counts[0] > 0 and session.counts[1] > 0)
            if session.block:
                until(lambda: session.entered.is_set())
            live_workers = media_workers(k)
            assert "amc-shared-source" in live_workers and "amc-source-pixels" in live_workers, live_workers
            assert live_workers.count("amc-subscription") == 2, live_workers
        finally:
            close_ms = session.close()
        snapshot = tuple(session.counts)
        time.sleep(0.03)
        assert tuple(session.counts) == snapshot, "callback after completed close"
        assert not media_workers(k), "SDK media worker survived close"
        value = stats(lib, ctx)
        assert [value[n] for n in ("queue_items", "queue_bytes", "external_held_items", "external_held_bytes")] == [0, 0, 0, 0], value
        if i >= warmup:
            result.append({"cycle": i - warmup + 1, "close_ms": close_ms, "counts": snapshot,
                           "live_workers": live_workers, "drained_stats": value, "resources": resources(k)})
            if (i - warmup + 1) % 10 == 0:
                (work / "cycle-progress.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
                print(json.dumps({"progress": "cycle", "completed": i - warmup + 1}), flush=True)
    return {"cycles": result, "warmup": warmup, "before": before, "after": resources(k)}


def retry_policy(lib, ctx, work):
    port = t.free_port()
    source = c.POINTER(a.amc_source_t)()
    creds = sized(a.am_credential_data_t)
    creds.username_utf8, creds.password_utf8 = b"synthetic", b"synthetic"
    desc = sized(a.am_source_desc_t)
    desc.kind, desc.url_utf8, desc.creds = a.AM_SRC_RTSP_URL, f"rtsp://127.0.0.1:{port}/absent".encode(), c.pointer(creds)
    checked(lib.amc_source_open(ctx, c.byref(desc), c.byref(source)))
    events, history = [], []
    @a.am_source_event_fn
    def event(src, kind, detail, user):
        reason = detail.contents.reason_utf8
        if reason in (b"camera.connecting", b"camera.streamInterrupted"):
            events.append((time.perf_counter(), reason.decode()))
    try:
        policy = sized(a.am_reconnect_policy_t)
        policy.initial_backoff_ms, policy.max_backoff_ms, policy.max_attempts = 100, 400, 5
        checked(lib.amc_source_set_reconnect_policy(source, c.byref(policy)))
        checked(lib.amc_source_set_event_callback(source, event, None))
        def done():
            status = sized(a.am_source_status_t)
            checked(lib.amc_source_get_status(source, c.byref(status)))
            row = (status.state, status.reconnect_attempts, status.fault_class)
            if not history or history[-1][1:] != row:
                history.append((time.perf_counter(), *row))
            return status.reconnect_attempts == 5
        # 包含五次最多5s原生打开；退避只从上次失败结束量到下次开始。
        until(done, 30)
        until(lambda: len([e for e in events if e[1] == "camera.streamInterrupted"]) >= 4)
        gaps = [(y[0] - x[0]) * 1000 for x, y in zip(events, events[1:])
                if x[1] == "camera.streamInterrupted" and y[1] == "camera.connecting"][-3:]
        assert len(gaps) == 3 and all(expected - 15 <= actual <= expected + 1000
                                     for expected, actual in zip([200, 400, 400], gaps)), gaps
        old = len(events)
        time.sleep(0.3)
        assert done() and len(events) == old, "max_attempts did not stop reconnect attempts"
        return {"initial_backoff_ms": 100, "max_backoff_ms": 400, "max_attempts": 5,
                "last_three_connect_gaps_ms": gaps, "stopped_after_max_attempts": True}
    finally:
        (work / "retry-policy-trace.json").write_text(json.dumps({"events": events, "status_changes": history}, indent=2), encoding="utf-8")
        checked(lib.amc_source_set_event_callback(source, a.am_source_event_fn(), None))
        close(lib, source, lib.amc_source_close)


def main(args):
    root = Path(__file__).resolve().parents[2]
    work = local_path(args.work_dir.resolve(), "tests", fresh=True)
    work.mkdir(parents=True)
    target = {"source": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
              "metric": "MET-2 M-RECOVER S4/S5", "fault_rounds": args.fault_rounds, "cycles": args.cycles,
              "warmup": args.warmup, "recovery_first_budget_ms": 10000, "recovery_five_frames_after_first_ms": 2000,
              "close_budget_ms": 8000, "sdk_workers_after_close": 0, "callbacks_after_close": 0,
              "retry_policy_probe_budget_ms": 30000, "retry_wait_boundary": "streamInterrupted to next connecting; excludes native open duration",
              "held_or_queued_after_close": 0, "scope": "local H264 256x144 10FPS software; not hardware/soak or a general SLA",
              "resource_review": "post-warmup resource series recorded; no universal RSS zero-growth gate"}
    (work / "target-card.json").write_text(json.dumps(target, indent=2), encoding="utf-8")
    lib = c.CDLL(str(args.dll_dir.resolve() / "am_camera.dll"))
    a.bind(lib, "amc_", require_all=True)
    params = sized(a.amc_init_params_t)
    params.abi_version = a.AM_ABI_VERSION
    paths = (c.c_char_p * 1)(str(args.runtime_dir.resolve()).encode())
    params.runtime_dirs_utf8, params.runtime_dir_count = paths, 1
    ctx = c.POINTER(a.amc_context_t)()
    checked(lib.amc_init(c.byref(params), c.byref(ctx)))
    k = lifecycle.windows()
    rows = {}
    try:
        with Stream(work, args.mediamtx.resolve()) as stream:
            rows["retry_policy"] = retry_policy(lib, ctx, work)
            if args.fault_rounds:
                rows["faults"] = faults(lib, ctx, stream, args.fault_rounds, work)
            if args.cycles:
                rows["lifecycle"] = cycles(lib, ctx, stream, args.cycles, args.warmup, k, work)
            assert not media_workers(k)
    except BaseException as error:
        (work / "failure.json").write_text(json.dumps({"type": type(error).__name__, "message": str(error)}, indent=2), encoding="utf-8")
        raise
    finally:
        close(lib, ctx, lib.amc_shutdown)
    rows["identity"] = {"source": target["source"], "dll_sha256": hashlib.sha256((args.dll_dir / "am_camera.dll").read_bytes()).hexdigest(),
                        "ffmpeg_files": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in args.runtime_dir.glob("*.dll")},
                        "mediamtx_sha256": hashlib.sha256(args.mediamtx.read_bytes()).hexdigest()}
    if rows.get("faults"):
        values = sorted(row["first_ms"] for row in rows["faults"])
        rows["recovery_summary"] = {"count": len(values), "median_ms": statistics.median(values), "max_ms": max(values)}
    rows["passed"] = True
    (work / "result.json").write_text(json.dumps(rows, indent=2), encoding="utf-8")
    print(json.dumps({"passed": True, "result": str(work / "result.json")}), flush=True)


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--dll-dir", type=Path, required=True)
    p.add_argument("--runtime-dir", type=Path, required=True)
    p.add_argument("--mediamtx", type=Path, required=True)
    p.add_argument("--work-dir", type=Path, required=True)
    p.add_argument("--fault-rounds", type=int, default=20)
    p.add_argument("--cycles", type=int, default=100)
    p.add_argument("--warmup", type=int, default=5)
    args = p.parse_args()
    assert 0 <= args.fault_rounds <= 100 and 0 <= args.cycles <= 1000 and 0 <= args.warmup <= 20
    main(args)
