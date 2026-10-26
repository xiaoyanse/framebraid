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
