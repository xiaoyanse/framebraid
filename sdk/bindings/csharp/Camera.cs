using System;
using System.Runtime.InteropServices;

namespace AmrtSdk {
    public static class ImagePreprocessing {
        public static am_raw_preprocess_t StretchRgb(uint width, uint height) {
            am_raw_preprocess_t value = Arena.Sized<am_raw_preprocess_t>();
            value.resize = 1; value.target_w = width; value.target_h = height;
            value.color_order = 0; value.normalize = 0; value.layout = 0;
            return value;
        }
    }
    public sealed class CameraContext : OwnedResource {
        private void RequireWorkerBudget(Arena arena) {
            HardwareReports.RequireApi();
            IntPtr cap = arena.Struct(Arena.Sized<amc_capability_t>());
            Diagnostics.Check(Native.amc_query_capability(Raw, cap), "query camera capability", Raw, false);
            if ((Arena.Read<amc_capability_t>(cap).feature_bits & Native.WorkerBudgetCapability) == 0)
                throw new NotSupportedException("This Media SDK does not expose worker budgets.");
        }
        public uint GetWorkerBudget() {
            lock (Gate) using (Arena arena = new Arena()) {
                RequireWorkerBudget(arena);
                IntPtr value = arena.Struct(0U);
                Diagnostics.Check(Native.amc_get_worker_budget(Raw, value), "get worker budget", Raw, false);
                return unchecked((uint)Marshal.ReadInt32(value));
            }
        }
        public void SetWorkerBudget(uint maximum) {
            lock (Gate) using (Arena arena = new Arena()) {
                RequireWorkerBudget(arena);
                Diagnostics.Check(Native.amc_set_worker_budget(Raw, maximum), "set worker budget", Raw, false);
            }
        }
        public string ProbeDecodeHardware(uint codec, uint budgetMs = 1500) {
            lock (Gate) {
                HardwareReports.RequireApi();
                Diagnostics.Check(Native.amc_probe_decode_hardware(Raw, codec, budgetMs), "probe decode hardware", Raw, false);
                return HardwareReports.Read(Raw);
            }
        }
        public string GetDecodeHardwareReport() { lock (Gate) { return HardwareReports.Read(Raw); } }
        public CameraContext(string[] runtimeDirectories, bool sharedFrames) : base(Create(runtimeDirectories, sharedFrames), null,
            Native.amc_shutdown, Native.amc_object_destroy, null) { }
        private static IntPtr Create(string[] directories, bool shared) {
            using (Arena arena = new Arena()) {
                amc_init_params_t p = Arena.Sized<amc_init_params_t>();
                p.abi_version = 65536; p.feature_flags = shared ? 1UL : 0UL;
                p.runtime_dirs_utf8 = arena.Directories(directories); p.runtime_dir_count = (uint)directories.Length;
                IntPtr raw; SdkException.Check(Diagnostics.Initialize(false, arena.Struct(p), out raw), "initialize camera"); return raw;
            }
        }
        public CameraSource OpenSource(string url, string user, string password, bool rtsp) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_source_desc_t desc = Arena.Sized<am_source_desc_t>();
                desc.kind = rtsp ? 0U : 1U; desc.url_utf8 = arena.Utf8(url); desc.stream_kind_utf8 = arena.Utf8(rtsp ? "main" : "isapi"); desc.reuse = 1;
                if (user != null) {
                    am_credential_data_t credentials = Arena.Sized<am_credential_data_t>();
                    credentials.username_utf8 = arena.Utf8(user); credentials.password_utf8 = arena.Utf8(password ?? ""); desc.creds = arena.Struct(credentials);
                }
                IntPtr source; Diagnostics.Check(Native.amc_source_open(Raw, arena.Struct(desc), out source), "open source", Raw, false); return new CameraSource(this, source);
            }
        }
        public SharedPublisher CreatePool(uint slots, uint maxFrameBytes) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_shared_pool_config_t cfg = Arena.Sized<am_shared_pool_config_t>();
                cfg.slot_count = slots; cfg.max_frame_bytes = maxFrameBytes; cfg.total_bytes = checked((ulong)slots * maxFrameBytes);
                cfg.max_consumers = 4; cfg.max_outstanding_borrows = checked(slots * 4);
                IntPtr attach = arena.Struct(Arena.Sized<am_shared_pool_attach_desc_t>()), pool;
                SdkException.Check(Native.amc_shared_pool_create(Raw, arena.Struct(cfg), attach, out pool), "create shared pool");
                return new SharedPublisher(this, pool, Arena.Read<am_shared_pool_attach_desc_t>(attach));
            }
        }
    }
    public sealed class CameraSource : OwnedResource {
        public string GetDecodeHardwareReport() { lock (Gate) { return HardwareReports.Read(Raw); } }
        public void SetDecodePolicy(bool softwareOnly) {
            lock (Gate) {
                HardwareReports.RequireApi();
                Diagnostics.Check(Native.amc_source_set_decode_policy(Raw, softwareOnly ? 1U : 0U), "set source decode policy", Raw, false);
            }
        }
        private readonly CameraContext context;
        internal CameraSource(CameraContext owner, IntPtr raw) : base(raw, owner, Native.amc_source_close, Native.amc_object_destroy, null) { context = owner; }
        public CapturedImage Capture(bool cpuPixels) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_capture_request_t p = Arena.Sized<am_capture_request_t>(); p.output_kind = cpuPixels ? 1U : 0U; p.deadline_ms = 5000;
                IntPtr image; Diagnostics.Check(Native.amc_capture_sync(Raw, arena.Struct(p), out image), "capture", Raw, false); return new CapturedImage(context, image);
            }
        }
        public FrameSubscription Frames(uint fps, uint pixelFormat) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_frame_sub_desc_t p = Arena.Sized<am_frame_sub_desc_t>();
                p.pixel_format = pixelFormat; p.sampling_max_fps = fps; p.mode = 1; p.queue_max_items = 2; p.queue_max_bytes = 32UL * 1024 * 1024;
                IntPtr sub; SdkException.Check(Native.amc_subscribe_frames(Raw, arena.Struct(p), out sub), "subscribe frames");
                return new FrameSubscription(this, context, sub);
            }
        }
    }
    public sealed class CapturedImage : OwnedResource {
        internal CapturedImage(CameraContext owner, IntPtr raw) : base(raw, owner, null, null, Native.amc_image_release) { }
        public byte[] CopyJpeg() {
            lock (Gate) {
                am_captured_image_t image = Arena.Read<am_captured_image_t>(Raw);
                if (image.output_kind != 0) throw new InvalidOperationException("Capture contains CPU pixels, not JPEG");
                byte[] bytes = new byte[checked((int)image.encoded_size)]; Marshal.Copy(image.encoded, bytes, 0, bytes.Length); return bytes;
            }
        }
        public am_captured_image_t BorrowView() { lock (Gate) return Arena.Read<am_captured_image_t>(Raw); }
    }
    public sealed class FrameSubscription : OwnedResource {
        private readonly CameraContext context;
        internal FrameSubscription(CameraSource owner, CameraContext camera, IntPtr raw) : base(raw, owner, Native.amc_frame_unsubscribe, Native.amc_object_destroy, null) { context = camera; }
        public CameraFrame Pull(uint timeoutMs) {
            lock (Gate) { IntPtr frame; SdkException.Check(Native.amc_frame_pull(Raw, timeoutMs, out frame), "pull frame"); return new CameraFrame(context, frame); }
        }
    }
    public sealed class CameraFrame : OwnedResource {
        public OwnedPixels Pixels() { return new OwnedPixels(this, Gate, BorrowPixel); }
        internal CameraFrame(CameraContext owner, IntPtr raw) : base(raw, owner, null, null, Native.amc_frame_release) { }
        // 低层借用，关闭后失效；安全调用优先用 Pixels()，低层逃逸由调用方负责。
        public am_pixel_view_t BorrowPixel() {
            lock (Gate) using (Arena arena = new Arena()) {
                IntPtr pixel = arena.Struct(Arena.Sized<am_pixel_view_t>());
                SdkException.Check(Native.amc_frame_as_pixel_view(Raw, pixel), "frame pixel view"); return Arena.Read<am_pixel_view_t>(pixel);
            }
        }
        public byte[][] CopyPlanes() {
            lock (Gate) using (Arena arena = new Arena()) {
                IntPtr pointer = arena.Struct(Arena.Sized<am_frame_view_t>());
                SdkException.Check(Native.amc_frame_get_view(Raw, pointer), "frame planes");
                am_frame_view_t view = Arena.Read<am_frame_view_t>(pointer); byte[][] planes = new byte[view.plane_count][];
                for (int i = 0; i < planes.Length; ++i) {
                    planes[i] = new byte[checked((int)view.plane_size_bytes[i])]; Marshal.Copy(view.planes[i], planes[i], 0, planes[i].Length);
                }
                return planes;
            }
        }
    }
    public sealed class SharedPublisher : OwnedResource {
        public readonly am_shared_pool_attach_desc_t Descriptor;
        internal SharedPublisher(CameraContext owner, IntPtr raw, am_shared_pool_attach_desc_t descriptor) : base(raw, owner, Native.amc_shared_pool_close, Native.amc_object_destroy, null) { Descriptor = descriptor; }
        public ulong Publish(am_pixel_view_t pixel) {
            if (pixel.assoc == IntPtr.Zero) throw new ArgumentException("Shared publication requires explicit source/frame identity; pass pixel.assoc");
            lock (Gate) using (Arena arena = new Arena()) {
                ulong publication; SdkException.Check(Native.amc_shared_pool_publish_view(Raw, arena.Struct(pixel), out publication), "publish frame"); return publication;
            }
        }
    }
    public sealed class SharedConsumer : OwnedResource {
        internal SharedConsumer(VisionContext owner, IntPtr raw) : base(raw, owner, Native.amv_shared_pool_detach, Native.amv_object_destroy, null) { }
        public SharedBorrow Borrow(ulong publication, uint timeoutMs) {
            lock (Gate) {
                IntPtr view = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(am_shared_frame_view_t)));
                Marshal.StructureToPtr(Arena.Sized<am_shared_frame_view_t>(), view, false);
                int status = Native.amv_shared_pool_borrow(Raw, publication, timeoutMs, view);
                if (status != 0) { Marshal.FreeHGlobal(view); SdkException.Check(status, "borrow shared frame"); }
                return new SharedBorrow(this, Raw, view);
            }
        }
    }
    public sealed class SharedBorrow : OwnedResource {
        public OwnedPixels Pixels() { return new OwnedPixels(this, Gate, BorrowPixel); }
        private readonly IntPtr pool;
        internal SharedBorrow(SharedConsumer owner, IntPtr poolHandle, IntPtr view) : base(view, owner, null, null,
            delegate(IntPtr p) { int status = Native.amv_shared_pool_return(poolHandle, p); if (status == 0) Marshal.FreeHGlobal(p); return status; }) { pool = poolHandle; }
        // 低层借用，关闭后失效；安全调用优先用 Pixels()，低层逃逸由调用方负责。
        public am_pixel_view_t BorrowPixel() {
            lock (Gate) using (Arena arena = new Arena()) {
                IntPtr pixel = arena.Struct(Arena.Sized<am_pixel_view_t>());
                SdkException.Check(Native.amv_shared_frame_as_pixel_view(pool, Raw, pixel), "shared pixel view"); return Arena.Read<am_pixel_view_t>(pixel);
            }
        }
    }
}
