"""轻量拥有权封装；底层结构和完整 API 仍由 amrt_sdk 自动生成。

资源必须使用 with 或 close。关闭超时保留句柄，可释放借用后重试；不强销毁
在用资源，不主动卸载 DLL。原始视图只在其拥有者存活期间可读。
"""
import ctypes as c
import json
from pathlib import Path
import queue
import threading
import sys
import time
import weakref

import amrt_sdk as a
from load_diagnostics import SdkLoadError, load_library

# 原生持有函数指针不构成 Python GC 根；注销屏障前必须全局固定委托和拥有者。
_CALLBACKS = {}


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


class SdkError(RuntimeError):
    def __init__(self, code, operation, message=None, source=None, retryable=None, fault_class=None):
        self.code = code
        self.message, self.source = message, source
        self.retryable, self.fault_class = retryable, fault_class
        detail = (": " + message) if message else ""
        if source:
            detail += " [source=" + source + "]"
        super().__init__("%s failed (SDK status %d)%s" % (operation, code, detail))


def copied_error(code, operation, info):
    message = info.message if isinstance(info, a.am_init_error_t) else info.message_utf8
    decode = lambda value: value.decode("utf-8", "replace") if value else None
    return SdkError(code, operation, decode(message), decode(info.source_utf8),
                    getattr(info, "retryable", None), getattr(info, "fault_class", None))


def check(code, operation):
    if code:
        raise SdkError(code, operation)


class Resource:
    def __init__(self, context, handle, close_function, destroy=True):
        self.context = context
        self.handle = handle
        self._close_function = close_function
        self._destroy = destroy
        self._children = weakref.WeakSet()
        self._lock = threading.RLock()
        if context is not None:
            context._children.add(self)

    def _require(self):
        if not self.handle:
            raise RuntimeError("SDK resource is closed")
        return self.handle

    def _check(self, code, operation):
        if code:
            info = sized(a.am_error_info_t)
            getter = getattr(self.library, self.prefix + "get_last_error", None)
            if getter is not None and getter(self.handle, c.byref(info)) == 0 and info.code == code:
                raise copied_error(code, operation, info)
            raise SdkError(code, operation)

    def close(self, timeout_ms=5000):
        with self._lock:
            if not self.handle:
                return
            if any(child.handle for child in self._children):
                raise RuntimeError("Close child resources before their parent")
            if self._destroy:
                state = c.c_uint32()
                self._check(self._close_function(self.handle, timeout_ms, c.byref(state)), "close")
                if state.value != a.AM_LIFECYCLE_CLOSED:
                    raise TimeoutError("SDK is still closing; release borrowed resources and retry")
                self._check(self._library_function("object_destroy")(self.handle), "destroy")
            else:
                self._check(self._close_function(self.handle), "release")
            self.handle = None

    def _library_function(self, suffix):
        return getattr(self.library, self.prefix + suffix)

    @property
    def library(self):
        return self.context.library

    @property
    def prefix(self):
        return self.context.prefix

    def __enter__(self):
        self._require()
        return self

    def __exit__(self, *_):
        self.close()


class Context(Resource):
    def __init__(self, dll_path, runtime_dirs=(), shared_frames=False, vision=False):
        self._library = load_library(dll_path)
        self._prefix = "amv_" if vision else "amc_"
        try:
            a.bind(self._library, self._prefix, require_all=True)
        except (AttributeError, RuntimeError) as error:
            raise RuntimeError("SDK exports do not match these bindings; deploy matching SDK and bindings: " +
                               str(Path(dll_path).resolve())) from error
        p = sized(a.amv_init_params_t if vision else a.amc_init_params_t)
        p.abi_version = a.AM_ABI_VERSION
        p.feature_flags = a.AM_INIT_SHARED_FRAMES if shared_frames else 0
        directories = (c.c_char_p * len(runtime_dirs))(
            *(str(Path(path).resolve()).encode("utf-8") for path in runtime_dirs))
        p.runtime_dirs_utf8, p.runtime_dir_count = directories, len(directories)
        if vision:
            p.infer_threads = 2
        handle = c.POINTER(a.amv_context_t if vision else a.amc_context_t)()
        code = self._library_function("init")(c.byref(p), c.byref(handle))
        if code:
            info = sized(a.am_init_error_t)
            if self._library_function("get_init_error")(c.byref(info)) == 0 and info.code == code:
                raise copied_error(code, "initialize", info)
            check(code, "initialize")
        super().__init__(None, handle, self._library_function("shutdown"))

    @property
    def library(self):
        return self._library

    @property
    def prefix(self):
        return self._prefix


def _selection_report(owner, function, *arguments):
    output, count = c.create_string_buffer(8192), c.c_uint64()
    owner._check(function(*arguments, output, len(output), c.byref(count)), "read selection report")
    if not 1 <= count.value <= len(output):
        raise RuntimeError("Invalid selection report length")
    return json.loads(output.raw[:count.value - 1])


class VisionContext(Context):
    def get_selection_attempt_report(self, attempt_id):
        with self._lock:
            return _selection_report(self, self.library.amv_get_selection_attempt_report,
                                     self._require(), attempt_id)

    def load_model_selected(self, path, candidates, profile=None, allow_fallback=True, budget_ms=1500):
        """candidates为(backend, device_id)序列；不会自动添加CPU。"""
        return self._load_selected(path, candidates, profile, allow_fallback, budget_ms, False)

    def load_model_memory_selected(self, data, candidates, profile=None, allow_fallback=True, budget_ms=1500):
        return self._load_selected(data, candidates, profile, allow_fallback, budget_ms, True)

    def _load_selected(self, source, candidates, profile, fallback, budget, memory):
        with self._lock:
            handle = self._require()
            capability = sized(a.amv_capability_t)
            self._check(self.library.amv_query_capability(handle, c.byref(capability)), "query selection capability")
            if not capability.feature_bits & a.AMV_CAP_ORDERED_SELECTION:
                raise NotImplementedError("Ordered selection requires matching FrameBraid ABI 1.1 bindings and library")
            candidates = tuple(candidates)
            if not 1 <= len(candidates) <= 3:
                raise ValueError("Provide one to three backend/device candidates")
            values = (a.am_backend_candidate_t * len(candidates))(
                *(a.am_backend_candidate_t(backend, device) for backend, device in candidates))
            policy = sized(a.am_backend_selection_t)
            policy.candidate_count, policy.candidates = len(values), values
            policy.allow_fallback, policy.budget_ms = int(fallback), budget
            desc = sized(a.am_model_memory_desc_t if memory else a.am_model_desc_t)
            desc.load_mode = a.AM_LOAD_PROFILE if profile else a.AM_LOAD_RAW
            desc.profile_id = profile.encode("utf-8") if profile else None
            model, attempt = c.POINTER(a.amv_model_t)(), c.c_uint64()
            if memory:
                storage = (c.c_ubyte * len(source)).from_buffer_copy(source)
                desc.bytes, desc.size = storage, len(storage)
                code = self.library.amv_model_load_memory_selected(handle, c.byref(desc), c.byref(policy),
                                                                  2, c.byref(model), c.byref(attempt))
            else:
                desc.path_utf8, desc.intra_threads = str(Path(source).resolve()).encode("utf-8"), 2
                code = self.library.amv_model_load_selected(handle, c.byref(desc), c.byref(policy),
                                                           c.byref(model), c.byref(attempt))
            try:
                self._check(code, "load selected model")
            except SdkError as error:
                error.attempt_id = attempt.value
                raise
            result = Model(self, model, self.library.amv_model_close)
            result.selection_attempt_id = attempt.value
            return result

    def get_inference_hardware_report(self):
        with self._lock:
            version = self.library.amv_get_abi_version()
            if version >> 16 != 1 or version < 0x10001:
                raise NotImplementedError("Inference hardware reports require C ABI 1.1")
            output, count = c.create_string_buffer(65536), c.c_uint64()
            self._check(self.library.amv_get_inference_hardware_report(self._require(), output, len(output), c.byref(count)), "read inference hardware report")
            if not 1 <= count.value <= len(output):
                raise RuntimeError("Invalid inference hardware report length")
            return json.loads(output.raw[:count.value - 1])

    def probe_inference_hardware(self, budget_ms=1500):
        with self._lock:
            version = self.library.amv_get_abi_version()
            if version >> 16 != 1 or version < 0x10001:
                raise NotImplementedError("Inference hardware reports require C ABI 1.1")
            self._check(self.library.amv_probe_inference_hardware(self._require(), budget_ms), "probe inference hardware")
            return self.get_inference_hardware_report()

    def __init__(self, dll_path, runtime_dirs=(), shared_frames=False):
        super().__init__(dll_path, runtime_dirs, shared_frames, True)

    def load_model(self, path, profile=None, backend=a.AM_BACKEND_CPU, strict=True):
        with self._lock:
            self._require()
            desc, policy = sized(a.am_model_desc_t), sized(a.am_backend_policy_t)
            desc.path_utf8 = str(Path(path).resolve()).encode("utf-8")
            desc.load_mode = a.AM_LOAD_PROFILE if profile else a.AM_LOAD_RAW
            desc.profile_id = profile.encode("utf-8") if profile else None
            policy.requested, policy.strict = backend, int(strict)
            desc.backend, desc.intra_threads = c.pointer(policy), 2
            handle = c.POINTER(a.amv_model_t)()
            self._check(self.library.amv_model_load(self.handle, c.byref(desc), c.byref(handle)), "load model")
            return Model(self, handle, self.library.amv_model_close)

    def attach_pool(self, descriptor):
        handle = c.POINTER(a.amv_shared_pool_t)()
        with self._lock:
            self._check(self.library.amv_shared_pool_attach(self._require(), c.byref(descriptor), c.byref(handle)), "attach pool")
            return ConsumerPool(self, handle, self.library.amv_shared_pool_detach)

    def fetch(self, request_id):
        with self._lock:
            pointer = c.POINTER(a.amv_result_t)()
            self._check(self.library.amv_result_fetch(self._require(), request_id, c.byref(pointer)), "fetch result")
            return Result(self, pointer, self.library.amv_result_release, False)

    def ack(self, request_id):
        with self._lock:
            self._check(self.library.amv_result_ack(self._require(), request_id), "ack result")

    def cancel(self, request_id):
        with self._lock:
            self._check(self.library.amv_request_cancel(self._require(), request_id), "cancel request")

    def wait(self, request_id, timeout=30):
        deadline = time.monotonic() + timeout
        while True:
            snapshot = c.POINTER(a.am_request_snapshot_t)()
            with self._lock:
                self._check(self.library.amv_request_query(self._require(), request_id, c.byref(snapshot)), "query request")
                try:
                    state = snapshot.contents.state
                    error = copied_error(snapshot.contents.error.contents.code, "request terminal state %d" % state,
                                         snapshot.contents.error.contents) if snapshot.contents.error else None
                finally:
                    self._check(self.library.amv_request_snapshot_release(snapshot), "release snapshot")
            if state >= a.AM_REQ_DONE:
                if state != a.AM_REQ_DONE:
                    raise error or SdkError(-1, "request terminal state %d" % state)
                return
            if time.monotonic() >= deadline:
                raise TimeoutError("Request remains queryable; wait again or cancel explicitly")
            time.sleep(0.002)


class Result(Resource):
    def raw_outputs(self):
        with self._lock:
            result = self._require().contents
            return [(tensor.name.decode("utf-8"), tensor.dtype,
                     tuple(tensor.shape[:tensor.shape_len]), c.string_at(tensor.data, tensor.data_size))
                    for tensor in result.raw_outputs[:result.raw_output_count]]

    def copy_pose(self):
        """复制 Pose 检测框与关键点；结果关闭后独立值仍可使用。"""
        with self._lock:
            result = self._require().contents
            if result.task != a.AM_TASK_POSE:
                raise ValueError("Result is not a Pose result")
            pose = result.pose
            if pose.count > 300 or (pose.count and not pose.detections):
                raise ValueError("Invalid Pose detections")
            copied = []
            for detection in pose.detections[:pose.count]:
                if detection.kpt_count > 1024 or (detection.kpt_count and not detection.keypoints):
                    raise ValueError("Invalid Pose keypoints")
                keypoints = tuple((point.x, point.y, point.confidence)
                                  for point in detection.keypoints[:detection.kpt_count])
                copied.append((tuple(detection.bounds), detection.confidence, keypoints))
            return tuple(copied)

    @property
    def view(self):
        """低层借用视图；调用方须保证结果始终存活，关闭后不可读取。"""
        return self._require().contents



class OwnedPixels:
    """只读操作门面；强持拥有者，调用期间禁止 close，不导出裸指针。

    拥有者显式关闭后本门面失效。异步输入由 SDK 默认复制，仅持有到提交返回；
    不承诺全链路零拷贝。底层 ctypes 入口仍由调用者自行保护。
    """
    def __init__(self, owner, lock, getter):
        self._owner, self._lock, self._getter = owner, lock, getter

    def infer(self, model, asynchronous=False, confidence=0.25, preprocess=None):
        with self._lock:
            return model.infer_pixel(self._getter(), asynchronous, confidence, preprocess)

    def publish(self, publisher):
        with self._lock:
            return publisher.publish(self._getter())

    def copy_planes(self):
        with self._lock:
            pixel = self._getter()
            if not pixel.view:
                raise ValueError("Pixel view is null")
            view = pixel.view.contents
            if view.width == 0 or view.height == 0 or not 1 <= view.plane_count <= 4:
                raise ValueError("Invalid plane dimensions/count")
            expected = 2 if view.format == a.AM_PF_NV12 else 1
            if view.format not in (a.AM_PF_RGBA8, a.AM_PF_RGB8, a.AM_PF_BGR8, a.AM_PF_NV12) or view.plane_count != expected:
                raise ValueError("Invalid pixel format/plane count")
            for i in range(view.plane_count):
                rows, row_bytes = view.height, view.width
                if view.format == a.AM_PF_NV12 and i == 1:
                    rows, row_bytes = (view.height + 1) // 2, ((view.width + 1) // 2) * 2
                elif view.format != a.AM_PF_NV12:
                    row_bytes *= 4 if view.format == a.AM_PF_RGBA8 else 3
                size, stride = view.plane_size_bytes[i], view.strides[i]
                if not view.planes[i] or stride < row_bytes or not stride * (rows - 1) + row_bytes <= size <= sys.maxsize:
                    raise ValueError("Invalid plane pointer/size/stride")
            return tuple(c.string_at(view.planes[i], view.plane_size_bytes[i])
                         for i in range(view.plane_count))


class RgbBuffer:
    """复制构造 RGB8 输入；调用方修改原 bytearray 不会改写待用像素。"""
    def __init__(self, width, height, data, association=None):
        if width <= 0 or height <= 0 or len(data) != width * height * 3:
            raise ValueError("RGB8 byte count must equal width * height * 3")
        self._lock, self.handle = threading.RLock(), True
        self._data = (c.c_ubyte * len(data)).from_buffer_copy(data)
        self._frame = sized(a.am_frame_view_t)
        self._frame.width, self._frame.height = width, height
        self._frame.format, self._frame.plane_count = a.AM_PF_RGB8, 1
        self._frame.planes[0] = c.cast(self._data, c.POINTER(c.c_ubyte))
        self._frame.strides[0], self._frame.plane_size_bytes[0] = width * 3, len(data)
        self._association = (a.am_frame_assoc_t.from_buffer_copy(association)
                             if association is not None else None)

    def pixel_view(self):
        """低层借用，调用方须保证拥有者和缓冲在原生读取结束前有效且不修改。"""
        with self._lock:
            if not self.handle:
                raise RuntimeError("RGB buffer is closed")
            pixel = sized(a.am_pixel_view_t)
            pixel.magic, pixel.view = 1095585880, c.pointer(self._frame)
            pixel.assoc = c.pointer(self._association) if self._association is not None else None
            return pixel

    def pixels(self):
        return OwnedPixels(self, self._lock, self.pixel_view)

    def close(self):
        with self._lock:
            self.handle = False
            self._data = self._frame = self._association = None

    def __enter__(self):
        with self._lock:
            self.pixel_view()
            return self

    def __exit__(self, *_):
        self.close()


class Model(Resource):
    NOTIFICATION_CAPACITY = 256

    def get_selection_report(self):
        with self._lock:
            return _selection_report(self, self.library.amv_get_model_selection_report, self._require())

    def __init__(self, *args):
        super().__init__(*args)
        self._notify = None
        self.notifications = queue.Queue(maxsize=self.NOTIFICATION_CAPACITY)
        self.notifications_dropped = 0
        self._notification_lock = threading.Lock()
        self.notification_error = None

    def _enqueue_notification(self, rid, state):
        # 仅统计语言缓冲丢弃；不把原生通知统计重复计入。
        with self._notification_lock:
            try:
                try:
                    self.notifications.put_nowait((rid, state))
                except queue.Full:
                    try:
                        self.notifications.get_nowait()
                        self.notifications_dropped += 1
                    except queue.Empty:
                        pass
                    try:
                        self.notifications.put_nowait((rid, state))
                    except queue.Full:
                        self.notifications_dropped += 1
            except BaseException as error:
                self.notifications_dropped += 1
                self.notification_error = error

    def enable_notifications(self, enabled=True):
        with self._lock:
            self._require()
            # 只排队 ID，不在原生回调线程执行调用方代码或跨语言异常。
            def enqueue(_model, rid, state, _user):
                try:
                    self._enqueue_notification(rid, state)
                except BaseException as error:
                    self.notifications_dropped += 1
                    self.notification_error = error
            callback = a.am_result_notify_fn(enqueue) if enabled else a.am_result_notify_fn()
            key = c.cast(self.handle, c.c_void_p).value
            previous = _CALLBACKS.get(key)
            if enabled:
                _CALLBACKS[key] = (self, callback)
            try:
                self._check(self.library.amv_result_notify_callback_set(self.handle, callback, None), "set notifications")
            except BaseException:
                if previous is not None:
                    _CALLBACKS[key] = previous
                else:
                    _CALLBACKS.pop(key, None)
                raise
            if not enabled:
                _CALLBACKS.pop(key, None)
            self._notify = callback if enabled else None

    def infer_encoded(self, data, png=False, asynchronous=False, confidence=0.25, preprocess=None):
        payload = (c.c_ubyte * len(data)).from_buffer_copy(data)
        image = sized(a.am_image_input_t)
        image.kind = a.AM_IMG_PNG if png else a.AM_IMG_JPEG
        image.encoded, image.encoded_size = payload, len(payload)
        return self._infer(image, asynchronous, confidence, preprocess)

    def infer_pixel(self, pixel, asynchronous=False, confidence=0.25, preprocess=None):
        """低层入口：调用方须持有像素拥有者直到本调用返回；优先用 owner.pixels().infer。"""
        image = sized(a.am_image_input_t)
        image.kind, image.view = a.AM_IMG_PIXEL_VIEW, c.pointer(pixel)
        # 默认复制接受的异步输入，调用返回后即可归还共享借用。
        return self._infer(image, asynchronous, confidence, preprocess)

    def _infer(self, image, asynchronous, confidence, preprocess):
        with self._lock:
            params = sized(a.am_infer_params_t)
            params.confidence_threshold = confidence
            params.raw_preprocess = c.pointer(preprocess) if preprocess is not None else None
            rid = c.c_uint64()
            if asynchronous:
                self._check(self.library.amv_infer_image_async(self._require(), c.byref(image), c.byref(params), c.byref(rid)), "submit inference")
                return rid.value
            pointer = c.POINTER(a.amv_result_t)()
            code = self.library.amv_infer_image_sync(self._require(), c.byref(image), c.byref(params), c.byref(pointer), c.byref(rid))
            if code == a.AM_ERR_SDK_DEADLINE_EXCEEDED:
                error = TimeoutError("Inference continues; query/fetch/ack request_id=%d" % rid.value)
                error.request_id = rid.value
                raise error
            self._check(code, "infer image")
            return rid.value, Result(self.context, pointer, self.library.amv_result_release, False)

    def close(self, timeout_ms=5000):
        with self._lock:
            if self.handle and self._notify is not None:
                self.enable_notifications(False)
            super().close(timeout_ms)


def _read_decode_hardware_report(resource):
    with resource._lock:
        version = resource.library.amc_get_abi_version()
        if version >> 16 != 1 or version < 0x10001:
            raise RuntimeError("Decode hardware reports require C ABI 1.1 in major version 1")
        output, count = c.create_string_buffer(65536), c.c_uint64()
        resource._check(resource.library.amc_get_decode_hardware_report(resource._require(), output, len(output), c.byref(count)), "read decode hardware report")
        if not 1 <= count.value <= len(output):
            raise RuntimeError("Invalid hardware report length")
        return json.loads(output.raw[:count.value - 1])


class CameraContext(Context):
    def _worker_budget_api(self):
        version = self.library.amc_get_abi_version()
        if version >> 16 != 1 or version < 0x10001:
            raise NotImplementedError("Media worker budgets require ABI 1.1")
        capability = sized(a.amc_capability_t)
        self._check(self.library.amc_query_capability(self._require(), c.byref(capability)), "query camera capability")
        if not capability.feature_bits & a.AMC_CAP_WORKER_BUDGET:
            raise NotImplementedError("This Media SDK does not expose worker budgets")

    def get_worker_budget(self):
        with self._lock:
            self._worker_budget_api()
            value = c.c_uint32()
            self._check(self.library.amc_get_worker_budget(self._require(), c.byref(value)), "get worker budget")
            return value.value

    def set_worker_budget(self, maximum):
        if not isinstance(maximum, int) or not 0 <= maximum <= a.AM_MEDIA_WORKER_LIMIT:
            raise ValueError("worker budget must be an integer in 0..%d" % a.AM_MEDIA_WORKER_LIMIT)
        with self._lock:
            self._worker_budget_api()
            self._check(self.library.amc_set_worker_budget(self._require(), maximum), "set worker budget")

    def get_decode_hardware_report(self):
        return _read_decode_hardware_report(self)

    def probe_decode_hardware(self, codec=a.AM_CODEC_H264, budget_ms=1500):
        with self._lock:
            self._check(self.library.amc_probe_decode_hardware(self._require(), codec, budget_ms), "probe decode hardware")
            return self.get_decode_hardware_report()

    def open_source(self, url, username=None, password=None, rtsp=False, reuse=True):
        desc, credentials = sized(a.am_source_desc_t), sized(a.am_credential_data_t)
        desc.kind, desc.url_utf8 = a.AM_SRC_RTSP_URL if rtsp else a.AM_SRC_ISAPI_URL, url.encode("utf-8")
        desc.stream_kind_utf8, desc.reuse = b"main" if rtsp else b"isapi", int(reuse)
        if username is not None:
            credentials.username_utf8 = username.encode("utf-8")
            credentials.password_utf8 = (password or "").encode("utf-8")
            desc.creds = c.pointer(credentials)
        handle = c.POINTER(a.amc_source_t)()
        with self._lock:
            self._check(self.library.amc_source_open(self._require(), c.byref(desc), c.byref(handle)), "open source")
            return Source(self, handle, self.library.amc_source_close)

    def create_pool(self, slots=3, max_frame_bytes=1920*1080*4, consumers=4):
        config, descriptor = sized(a.am_shared_pool_config_t), sized(a.am_shared_pool_attach_desc_t)
        config.slot_count, config.max_frame_bytes, config.total_bytes = slots, max_frame_bytes, slots*max_frame_bytes
        config.max_consumers, config.max_outstanding_borrows = consumers, consumers*slots
        handle = c.POINTER(a.amc_shared_pool_t)()
        with self._lock:
            self._check(self.library.amc_shared_pool_create(self._require(), c.byref(config), c.byref(descriptor), c.byref(handle)), "create pool")
            pool = PublisherPool(self, handle, self.library.amc_shared_pool_close)
            pool.descriptor = descriptor
            return pool


class Source(Resource):
    def get_decode_hardware_report(self):
        return _read_decode_hardware_report(self)

    def set_decode_policy(self, software_only=False):
        with self._lock:
            self._check(self.library.amc_source_set_decode_policy(self._require(),
                a.AM_DHW_SOFTWARE_ONLY if software_only else a.AM_DHW_PREFER_HW), "set source decode policy")

    def capture(self, cpu_frame=False):
        request = sized(a.am_capture_request_t)
        request.output_kind = a.AM_CAP_CPU_FRAME if cpu_frame else a.AM_CAP_ENCODED_JPEG
        request.deadline_ms = 5000
        pointer = c.POINTER(a.am_captured_image_t)()
        with self._lock:
            self._check(self.library.amc_capture_sync(self._require(), c.byref(request), c.byref(pointer)), "capture")
            return CapturedImage(self.context, pointer, self.library.amc_image_release, False)

    def frames(self, sampling_fps=5, pixel_format=a.AM_PF_RGB8):
        desc = sized(a.am_frame_sub_desc_t)
        desc.pixel_format, desc.sampling_max_fps, desc.mode = pixel_format, sampling_fps, a.AM_SUB_PULL
        desc.queue_max_items, desc.queue_max_bytes = 2, 32*1024*1024
        handle = c.POINTER(a.amc_frame_sub_t)()
        with self._lock:
            self._check(self.library.amc_subscribe_frames(self._require(), c.byref(desc), c.byref(handle)), "subscribe frames")
            return FrameSubscription(self, handle, self.library.amc_frame_unsubscribe)


class CapturedImage(Resource):
    def jpeg(self):
        with self._lock:
            image = self._require().contents
            if image.output_kind != a.AM_CAP_ENCODED_JPEG:
                raise ValueError("Capture contains CPU pixels, not JPEG")
            return c.string_at(image.encoded, image.encoded_size)


class FrameSubscription(Resource):
    def pull(self, timeout_ms=5000):
        pointer = c.POINTER(a.amc_frame_t)()
        with self._lock:
            self._check(self.library.amc_frame_pull(self._require(), timeout_ms, c.byref(pointer)), "pull frame")
            return Frame(self.context.context, pointer, self.library.amc_frame_release, False)


class Frame(Resource):
    def pixels(self):
        return OwnedPixels(self, self._lock, self.pixel_view)

    def pixel_view(self):
        """低层借用；本帧 close 后失效。安全推理使用 pixels().infer。"""
        with self._lock:
            pixel = sized(a.am_pixel_view_t)
            self._check(self.library.amc_frame_as_pixel_view(self._require(), c.byref(pixel)), "frame view")
            return pixel

    def copy_planes(self):
        with self._lock:
            view = sized(a.am_frame_view_t)
            self._check(self.library.amc_frame_get_view(self._require(), c.byref(view)), "frame planes")
            return [c.string_at(view.planes[i], view.plane_size_bytes[i]) for i in range(view.plane_count)]


class PublisherPool(Resource):
    def publish(self, pixel):
        if not pixel.assoc:
            raise ValueError("Shared publication requires explicit source/frame identity; pass pixel.assoc")
        publication = c.c_uint64()
        with self._lock:
            self._check(self.library.amc_shared_pool_publish_view(self._require(), c.byref(pixel), c.byref(publication)), "publish shared frame")
        return publication.value


class ConsumerPool(Resource):
    def borrow(self, publication_id=0, wait_ms=0):
        view = sized(a.am_shared_frame_view_t)
        with self._lock:
            self._check(self.library.amv_shared_pool_borrow(self._require(), publication_id, wait_ms, c.byref(view)), "borrow shared frame")
            return BorrowedFrame(self, view)


class BorrowedFrame:
    def __init__(self, pool, view):
        self.pool, self.view, self.handle = pool, view, True
        pool._children.add(self)

    def pixels(self):
        return OwnedPixels(self, self.pool._lock, self.pixel_view)

    def pixel_view(self):
        """低层借用；归还后失效。安全推理使用 pixels().infer。"""
        with self.pool._lock:
            if not self.handle:
                raise RuntimeError("Borrow already returned")
            pixel = sized(a.am_pixel_view_t)
            self.pool._check(self.pool.library.amv_shared_frame_as_pixel_view(self.pool._require(), c.byref(self.view), c.byref(pixel)), "shared pixel view")
            return pixel

    def close(self):
        with self.pool._lock:
            if self.handle:
                self.pool._check(self.pool.library.amv_shared_pool_return(self.pool._require(), c.byref(self.view)), "return shared frame")
                self.handle = False

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
