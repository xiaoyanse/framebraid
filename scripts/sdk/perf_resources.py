"""MET-2进程CPU/内存和Windows按进程GPU引擎采样；未知不写零。"""
import ctypes as c
import math
import os
import re
import threading
import time

from perf_common import process_resources


def cpu_snapshot():
    # process_time包含当前进程用户态+内核态，不含独立造流器。
    return dict(wall_ns=time.perf_counter_ns(), cpu_ns=time.process_time_ns())


def cpu_delta(before, after, parallelism):
    wall = after["wall_ns"] - before["wall_ns"]
    cpu = after["cpu_ns"] - before["cpu_ns"]
    if wall <= 0 or cpu < 0 or type(parallelism) is not int or parallelism < 1:
        raise ValueError("invalid CPU window or parallelism")
    return dict(wall_s=wall / 1e9, cpu_s=cpu / 1e9,
                core_equivalent=cpu / wall,
                normalized_percent=cpu / wall / parallelism * 100,
                available_logical_parallelism=parallelism,
                scope="whole observer/SDK/native-runtime process, not SDK-only")


def available_parallelism():
    if hasattr(os, "process_cpu_count"):
        return os.process_cpu_count() or 1, "os.process_cpu_count"
    if hasattr(os, "sched_getaffinity"):
        return len(os.sched_getaffinity(0)), "sched_getaffinity"
    return os.cpu_count() or 1, "os.cpu_count; affinity unknown"


class _Value(c.Structure):
    class Data(c.Union):
        _fields_ = [("double", c.c_double), ("large", c.c_int64)]
    _fields_ = [("status", c.c_uint32), ("data", Data)]


class _Item(c.Structure):
    _fields_ = [("name", c.c_wchar_p), ("value", _Value)]


class GpuCounters:
    """PDH英文通配计数器，不依赖本机显示语言，不枚举或创建GPU计算上下文。"""
    PATHS = {"engines": r"\GPU Engine(*)\Utilization Percentage",
             "dedicated_bytes": r"\GPU Process Memory(*)\Dedicated Usage",
             "shared_bytes": r"\GPU Process Memory(*)\Shared Usage"}

    def __init__(self):
        self.query = c.c_void_p()
        self.counters = {}
        self.errors = {}
        self.pdh = None
        if os.name != "nt":
            self.errors["provider"] = "Windows PDH unavailable on this OS"
            return
        self.pdh = c.WinDLL("pdh", use_last_error=True)
        api = self.pdh
        api.PdhOpenQueryW.argtypes = [c.c_wchar_p, c.c_size_t, c.POINTER(c.c_void_p)]
        api.PdhOpenQueryW.restype = c.c_uint32
        api.PdhAddEnglishCounterW.argtypes = [c.c_void_p, c.c_wchar_p, c.c_size_t, c.POINTER(c.c_void_p)]
        api.PdhAddEnglishCounterW.restype = c.c_uint32
        api.PdhCollectQueryData.argtypes = [c.c_void_p]
        api.PdhCollectQueryData.restype = c.c_uint32
        api.PdhGetFormattedCounterArrayW.argtypes = [c.c_void_p, c.c_uint32,
            c.POINTER(c.c_uint32), c.POINTER(c.c_uint32), c.c_void_p]
        api.PdhGetFormattedCounterArrayW.restype = c.c_uint32
        api.PdhCloseQuery.argtypes = [c.c_void_p]
        api.PdhCloseQuery.restype = c.c_uint32
        status = api.PdhOpenQueryW(None, 0, c.byref(self.query))
        if status:
            self.errors["provider"] = f"PdhOpenQueryW:{status:#x}"
            return
        for name, path in self.PATHS.items():
            counter = c.c_void_p()
            status = api.PdhAddEnglishCounterW(self.query, path, 0, c.byref(counter))
            if status:
                self.errors[name] = f"PdhAddEnglishCounterW:{status:#x}"
            else:
                self.counters[name] = counter

    def collect(self):
        if not self.query:
            return dict(values=None, errors=dict(self.errors))
        status = self.pdh.PdhCollectQueryData(self.query)
        if status:
            return dict(values=None, errors=dict(self.errors, collect=f"{status:#x}"))
        values, errors = {}, dict(self.errors)
        for name, counter in self.counters.items():
            size, count = c.c_uint32(), c.c_uint32()
            status = self.pdh.PdhGetFormattedCounterArrayW(counter, 0x200, c.byref(size), c.byref(count), None)
            if status != 0x800007D2 or not 0 < size.value <= 8 * 1024 * 1024:
                errors[name] = f"size/query unavailable:{status:#x}"
                values[name] = None
                continue
            buffer = c.create_string_buffer(size.value)
            status = self.pdh.PdhGetFormattedCounterArrayW(counter, 0x200, c.byref(size), c.byref(count), buffer)
            if status or count.value > 4096 or count.value * c.sizeof(_Item) > len(buffer):
                errors[name] = f"array unavailable or budget exceeded:{status:#x}"
                values[name] = None
                continue
            items = c.cast(buffer, c.POINTER(_Item))
            own = {}
            for index in range(count.value):
                item = items[index]
                if item.name and item.name.startswith(f"pid_{os.getpid()}_"):
                    value = item.value.data.double
                    own[item.name] = value if item.value.status in (0, 1) and math.isfinite(value) and value >= 0 else None
            # 无本进程实例表示未观测到，而不是GPU使用为零。
            values[name] = own or None
        return dict(values=values, errors=errors)

    def close(self):
        if self.query:
            self.pdh.PdhCloseQuery(self.query)
            self.query = c.c_void_p()


def summarize_gpu(samples):
    series = {}
    for row in samples:
        for kind, instances in (row["gpu"].get("values") or {}).items():
            for instance, value in (instances or {}).items():
                if value is not None:
                    series.setdefault((kind, instance), []).append(value)
    result = []
    for (kind, instance), values in sorted(series.items()):
        match = re.search(r"luid_(0x[0-9a-f]+)_(0x[0-9a-f]+)_phys_(\d+)", instance, re.I)
        result.append(dict(kind=kind, instance=instance,
            device_luid_parts=list(match.groups()) if match else None,
            unit="percent" if kind == "engines" else "bytes", samples=len(values),
            mean=sum(values) / len(values), maximum=max(values)))
    return result or None


class ResourceSampler:
    def __init__(self, interval_s=.25, max_samples=4096, enabled=True):
        if not .05 <= interval_s <= 10 or not 1 <= max_samples <= 4096:
            raise ValueError("invalid observation budget")
        self.interval_s, self.max_samples, self.enabled = interval_s, max_samples, enabled
        self.samples, self.errors = [], []
        self.stop_event = threading.Event()
        self.ready_event = threading.Event()
        self.thread = None
        self.sampler_cpu_ns = 0
        self.sampler_wall_ns = 0
        self.initialization_cpu_ns = 0
        self.capacity_reached = False

    def start(self):
        if self.thread is not None:
            raise ValueError("sampler already started")
        self.started_ns = time.perf_counter_ns()
        if self.enabled:
            self.thread = threading.Thread(target=self._run, name="metric-sampler")
            self.thread.start()
            if not self.ready_event.wait(5):
                self.stop()
                raise RuntimeError("resource sampler initialization timed out")
        return self

    def _run(self):
        gpu = None
        start_cpu = time.thread_time_ns()
        start_wall = time.perf_counter_ns()
        try:
            gpu = GpuCounters()
            gpu.collect()  # GPU速率计数首样本仅作为区间起点。
            self.initialization_cpu_ns = time.thread_time_ns() - start_cpu
            self.ready_event.set()
            while not self.stop_event.wait(self.interval_s):
                if len(self.samples) >= self.max_samples:
                    self.capacity_reached = True
                    break
                try:
                    memory = process_resources()
                except OSError as error:
                    memory = None
                    if len(self.errors) < 16:
                        self.errors.append(str(error))
                self.samples.append(dict(offset_ns=time.perf_counter_ns() - self.started_ns,
                                         memory=memory, gpu=gpu.collect()))
        except Exception as error:
            self.errors.append(type(error).__name__ + ": " + str(error))
        finally:
            self.ready_event.set()
            if gpu is not None:
                gpu.close()
            self.sampler_cpu_ns = time.thread_time_ns() - start_cpu
            self.sampler_wall_ns = time.perf_counter_ns() - start_wall

    def stop(self):
        self.stop_event.set()
        if self.thread is not None:
            self.thread.join(timeout=10)
            if self.thread.is_alive():
                raise RuntimeError("resource sampler failed to close")
        return dict(enabled=self.enabled, interval_s=self.interval_s,
            started_at_ns=getattr(self, "started_ns", None),
            sample_limit=self.max_samples, capacity_reached=self.capacity_reached,
            samples=self.samples, errors=self.errors,
            sampler_thread_cpu_s=self.sampler_cpu_ns / 1e9,
            initialization_cpu_s=self.initialization_cpu_ns / 1e9,
            sampler_lifetime_s=self.sampler_wall_ns / 1e9,
            gpu_series=summarize_gpu(self.samples),
            unknown="missing GPU instance/counter is unknown, not zero; memory peaks are sampled")
