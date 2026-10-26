"""MET-2 S1重复性汇总；展示资源取舍，不自动发出性能通过结论。"""
import math
import statistics


def spread(values):
    if not values or any(not math.isfinite(value) or value < 0 for value in values):
        raise ValueError("invalid measurement population")
    midpoint = statistics.median(values)
    width = max(values) - min(values)
    return dict(minimum=min(values), maximum=max(values), median=midpoint,
                absolute_span=width, relative_span=None if midpoint == 0 else width / midpoint)


def validate_stream(row):
    counts = row["counts"]
    if any(type(value) is not int or value < 0 for value in counts.values()):
        raise ValueError("invalid stream counter")
    if counts["opening_inflight"] + counts["window_inputs"] != counts["all_outputs_before_end"] + counts["closing_inflight"]:
        raise ValueError("window counter conservation failed")
    if counts["window_inputs"] != counts["measured_input_outputs_before_end"] + counts["measured_input_outputs_after_end"]:
        raise ValueError("measured inputs did not all drain")
    if counts["useful_before_end"] + counts["expired_before_end"] != counts["measured_input_outputs_before_end"]:
        raise ValueError("useful denominator mismatch")
    if len(row["samples_ns"]) != counts["measured_input_outputs_before_end"]:
        raise ValueError("latency population mismatch")
    if (not math.isfinite(row["cpu"]["wall_s"]) or not math.isfinite(row["cpu"]["cpu_s"])
            or row["cpu"]["wall_s"] <= 0 or row["cpu"]["cpu_s"] < 0):
        raise ValueError("invalid CPU interval")


def review_stream_rows(rows, window_s):
    groups = {}
    for row in rows:
        validate_stream(row)
        groups.setdefault(row["policy"], []).append(row)
    review = {}
    for name, values in groups.items():
        backends = sorted({row["actual_backend"] for row in values})
        if len(backends) != 1:
            raise ValueError("A/A mixed actual backends")
        review[name] = dict(rounds=len(values), actual_backends=backends,
            cpu_s=spread([row["cpu"]["cpu_s"] for row in values]),
            latency_p95_ms=spread([row["latency"]["p95_ms"] for row in values]),
            useful_frames=spread([row["counts"]["useful_before_end"] for row in values]),
            useful_fps=spread([row["useful_fps"] for row in values]),
            window_overrun_s=spread([max(0, row["cpu"]["wall_s"]-window_s) for row in values]),
            gpu_observed=all(row["observation"]["gpu_series"] is not None for row in values),
            verdict="A/A observations only; primary gain and guards require target-card review")
    return review


def review_sampling(row, planned_window_s=None):
    """S2测前实验线，只列不足；配置max_fps不是最低交付承诺。"""
    wall=row["cpu"]["wall_s"]
    if not math.isfinite(wall) or wall<=0:
        raise ValueError("invalid sampling wall interval")
    planned=row.get("planned_window_s",planned_window_s if planned_window_s is not None else wall)
    if not math.isfinite(planned) or planned<=0:
        raise ValueError("invalid planned sampling window")
    findings=[]
    if abs(wall-planned)>.1:
        findings.append("measurement wall window drift exceeds 100 ms")
    if row["compressed_packets"]/wall<27:
        findings.append("source supply below experimental 27 FPS")
    for consumer in row["consumers"]:
        count=consumer["delivered"]
        if count!=len(consumer["frames"]):
            raise ValueError("sampling output denominator mismatch")
        if len({frame["frame_id"] for frame in consumer["frames"]})!=count:
            raise ValueError("duplicate sampling frame identity")
        # 固定4秒窗口的10 FPS×80%=32帧；几百微秒尾差不把32帧误判不足。
        if count < math.ceil(consumer["max_fps"]*.8*planned):
            findings.append(f"{consumer['max_fps']} max FPS below experimental 80 percent delivery")
        p95=consumer["age"]["p95_ms"]
        if p95 is None or p95>250:
            findings.append(f"{consumer['max_fps']} max FPS freshness insufficient")
    closed=row.get("after_source_close")
    if closed is not None and closed["external_held_items"]:
        findings.append("external frame remains after source drained")
    return dict(findings=findings,experimental_guards_met=not findings,
                postclose_count_observed=closed is not None,
                acceptance="not minimum FPS guarantee, optimization gain or whole-package acceptance")
