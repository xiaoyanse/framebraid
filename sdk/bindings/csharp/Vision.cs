using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

namespace AmrtSdk {
    public sealed partial class VisionContext : OwnedResource {
        public string GetInferenceHardwareReport() { lock (Gate) return HardwareReports.Read(Raw, true); }
        public string ProbeInferenceHardware(uint budgetMs = 1500) {
            lock (Gate) {
                HardwareReports.RequireApi(true);
                Diagnostics.Check(Native.amv_probe_inference_hardware(Raw, budgetMs), "probe inference hardware", Raw, true);
                return HardwareReports.Read(Raw, true);
            }
        }
        public VisionContext(string[] runtimeDirectories, bool sharedFrames) : base(Create(runtimeDirectories, sharedFrames), null,
            Native.amv_shutdown, Native.amv_object_destroy, null) { }
        private static IntPtr Create(string[] directories, bool shared) {
            using (Arena arena = new Arena()) {
                amv_init_params_t p = Arena.Sized<amv_init_params_t>();
                p.abi_version = 65536; p.feature_flags = shared ? 1UL : 0UL; p.infer_threads = 2;
                p.runtime_dirs_utf8 = arena.Directories(directories); p.runtime_dir_count = (uint)directories.Length;
                IntPtr raw; SdkException.Check(Diagnostics.Initialize(true, arena.Struct(p), out raw), "initialize vision"); return raw;
            }
        }
        public Model LoadModel(string path, string profile, uint backend, bool strict) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_model_desc_t p = Arena.Sized<am_model_desc_t>();
                am_backend_policy_t policy = Arena.Sized<am_backend_policy_t>();
                policy.requested = backend; policy.strict = strict ? 1U : 0U;
                p.path_utf8 = arena.Utf8(System.IO.Path.GetFullPath(path)); p.profile_id = arena.Utf8(profile);
                p.load_mode = profile == null ? 1U : 0U; p.backend = arena.Struct(policy); p.intra_threads = 2;
                IntPtr raw; Diagnostics.Check(Native.amv_model_load(Raw, arena.Struct(p), out raw), "load model", Raw, true);
                return new Model(this, raw);
            }
        }
        public InferenceResult Fetch(ulong request) {
            lock (Gate) { IntPtr raw; SdkException.Check(Native.amv_result_fetch(Raw, request, out raw), "fetch result"); return new InferenceResult(this, raw); }
        }
        public void Acknowledge(ulong request) { lock (Gate) SdkException.Check(Native.amv_result_ack(Raw, request), "ack result"); }
        public void Cancel(ulong request) { lock (Gate) SdkException.Check(Native.amv_request_cancel(Raw, request), "cancel request"); }
        public void Wait(ulong request, uint timeoutMs) {
            Stopwatch clock = Stopwatch.StartNew();
            while (true) {
                uint state; SdkException failure = null;
                lock (Gate) {
                    IntPtr raw; SdkException.Check(Native.amv_request_query(Raw, request, out raw), "query request");
                    try {
                        am_request_snapshot_t snapshot = Arena.Read<am_request_snapshot_t>(raw); state = snapshot.state;
                        if (snapshot.error != IntPtr.Zero) {
                            am_error_info_t error = Arena.Read<am_error_info_t>(snapshot.error);
                            failure = Diagnostics.Copy(error.code, "request terminal state " + state, error);
                        }
                    } finally { SdkException.Check(Native.amv_request_snapshot_release(raw), "release snapshot"); }
                }
                if (state >= 2) { if (state != 2) throw failure ?? new SdkException(-1, "request terminal state " + state); return; }
                if (clock.ElapsedMilliseconds >= timeoutMs) throw new TimeoutException("Request remains queryable; wait again or cancel explicitly");
                Thread.Sleep(2);
            }
        }
        public SharedConsumer Attach(am_shared_pool_attach_desc_t descriptor) {
            lock (Gate) using (Arena arena = new Arena()) {
                IntPtr pool; SdkException.Check(Native.amv_shared_pool_attach(Raw, arena.Struct(descriptor), out pool), "attach shared pool");
                return new SharedConsumer(this, pool);
            }
        }
    }

    public sealed class RequestTimeoutException : TimeoutException {
        public ulong RequestId { get; private set; }
        internal RequestTimeoutException(ulong id) : base("Inference continues; query/fetch/ack RequestId") { RequestId = id; }
    }

    public sealed partial class Model : OwnedResource {
        private readonly VisionContext context;
        private static readonly ConcurrentDictionary<IntPtr, Native.Notify> callbacks = new ConcurrentDictionary<IntPtr, Native.Notify>();
        private readonly ConcurrentQueue<Tuple<ulong, uint>> notifications = new ConcurrentQueue<Tuple<ulong, uint>>();
        private long notificationsDropped;
        public long NotificationsDropped { get { return Interlocked.Read(ref notificationsDropped); } }
        internal Model(VisionContext owner, IntPtr raw) : base(raw, owner, Native.amv_model_close, Native.amv_object_destroy, null) { context = owner; }
        public void EnableNotifications(bool enable) {
            lock (Gate) {
                IntPtr key = Raw;
                if (enable) {
                    // 原生线程只入队，不执行业务委托，避免异常或重入越过 C ABI。
                    Native.Notify callback = delegate(IntPtr m, ulong id, uint state, IntPtr user) {
                        try { notifications.Enqueue(Tuple.Create(id, state)); }
                        catch { Interlocked.Increment(ref notificationsDropped); }
                    };
                    Native.Notify previous;
                    callbacks.TryGetValue(key, out previous);
                    callbacks[key] = callback;
                    int status = Native.amv_result_notify_callback_set(key, callback, IntPtr.Zero);
                    GC.KeepAlive(previous);
                    if (status != 0) {
                        if (previous != null) callbacks[key] = previous;
                        else callbacks.TryRemove(key, out callback);
                        SdkException.Check(status, "set notifications");
                    }
                } else {
                    SdkException.Check(Native.amv_result_notify_callback_set(key, null, IntPtr.Zero), "clear notifications");
                    Native.Notify removed; callbacks.TryRemove(key, out removed); GC.KeepAlive(removed);
                }
            }
        }
        public bool TryTakeNotification(out Tuple<ulong, uint> notification) { return notifications.TryDequeue(out notification); }
        internal override void BeforeClose() { EnableNotifications(false); }
        public InferenceResult InferEncoded(byte[] bytes, bool png, out ulong request, am_raw_preprocess_t? preprocessing) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_image_input_t image = Arena.Sized<am_image_input_t>();
                image.kind = png ? 1U : 0U; image.encoded = arena.Bytes(bytes); image.encoded_size = (ulong)bytes.LongLength;
                return Infer(arena, image, preprocessing, out request);
            }
        }
        public ulong SubmitEncoded(byte[] bytes, bool png, am_raw_preprocess_t? preprocessing) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_image_input_t image = Arena.Sized<am_image_input_t>();
                image.kind = png ? 1U : 0U; image.encoded = arena.Bytes(bytes); image.encoded_size = (ulong)bytes.LongLength;
                return Submit(arena, image, preprocessing);
            }
        }
        public InferenceResult InferPixel(am_pixel_view_t pixel, out ulong request, am_raw_preprocess_t? preprocessing) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_image_input_t image = Arena.Sized<am_image_input_t>(); image.kind = 2; image.view = arena.Struct(pixel);
                return Infer(arena, image, preprocessing, out request);
            }
        }
        public ulong SubmitPixel(am_pixel_view_t pixel, am_raw_preprocess_t? preprocessing) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_image_input_t image = Arena.Sized<am_image_input_t>(); image.kind = 2; image.view = arena.Struct(pixel);
                return Submit(arena, image, preprocessing);
            }
        }
        private IntPtr Parameters(Arena arena, am_raw_preprocess_t? preprocessing) {
            am_infer_params_t p = Arena.Sized<am_infer_params_t>(); p.confidence_threshold = 0.25F;
            if (preprocessing.HasValue) p.raw_preprocess = arena.Struct(preprocessing.Value);
            return arena.Struct(p);
        }
        private InferenceResult Infer(Arena arena, am_image_input_t image, am_raw_preprocess_t? preprocessing, out ulong request) {
            IntPtr result;
            int status = Native.amv_infer_image_sync(Raw, arena.Struct(image), Parameters(arena, preprocessing), out result, out request);
            if (status == 9) throw new RequestTimeoutException(request);
            Diagnostics.Check(status, "infer image", Raw, true); return new InferenceResult(context, result);
        }
        private ulong Submit(Arena arena, am_image_input_t image, am_raw_preprocess_t? preprocessing) {
            ulong request;
            Diagnostics.Check(Native.amv_infer_image_async(Raw, arena.Struct(image), Parameters(arena, preprocessing), out request), "submit inference", Raw, true);
            return request;
        }
    }

    public sealed class TensorCopy {
        public readonly uint DataType;
        public readonly long[] Shape;
        public readonly byte[] Data;
        internal TensorCopy(uint dtype, long[] shape, byte[] data) { DataType = dtype; Shape = shape; Data = data; }
    }
    public sealed class InferenceResult : OwnedResource {
        internal InferenceResult(VisionContext owner, IntPtr raw) : base(raw, owner, null, null, Native.amv_result_release) { }
        // 结构化视图含借用指针，只能在本 result 存活期间读取；长期持有使用复制出口。
        public amv_result_t BorrowView() { lock (Gate) return Arena.Read<amv_result_t>(Raw); }
        public Dictionary<string, TensorCopy> CopyRawOutputs() {
            lock (Gate) {
                amv_result_t result = Arena.Read<amv_result_t>(Raw);
                Dictionary<string, TensorCopy> output = new Dictionary<string, TensorCopy>();
                int size = Marshal.SizeOf(typeof(am_tensor_out_t));
                for (int i = 0; i < result.raw_output_count; ++i) {
                    am_tensor_out_t tensor = Arena.Read<am_tensor_out_t>(IntPtr.Add(result.raw_outputs, checked(i * size)));
                    byte[] bytes = new byte[checked((int)tensor.data_size)]; long[] shape = new long[tensor.shape_len];
                    Marshal.Copy(tensor.data, bytes, 0, bytes.Length); Marshal.Copy(tensor.shape, shape, 0, shape.Length);
                    output.Add(Utf8(tensor.name), new TensorCopy(tensor.dtype, shape, bytes));
                }
                return output;
            }
        }
        private static string Utf8(IntPtr pointer) {
            int length = 0; while (Marshal.ReadByte(pointer, length) != 0) ++length;
            byte[] data = new byte[length]; Marshal.Copy(pointer, data, 0, length); return System.Text.Encoding.UTF8.GetString(data);
        }
    }
}
