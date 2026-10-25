using System;
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;

namespace AmrtSdk {
    internal static class Native {
        private const CallingConvention Convention = CallingConvention.Cdecl;
        [UnmanagedFunctionPointer(Convention)] internal delegate void Notify(IntPtr model, ulong request, uint state, IntPtr user);
        internal delegate int CloseFunction(IntPtr handle, uint timeout, out uint state);
        internal delegate int DestroyFunction(IntPtr handle);
        internal delegate int ReleaseFunction(IntPtr handle);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_get_init_error(IntPtr info);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_get_init_error(IntPtr info);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_get_last_error(IntPtr handle, IntPtr info);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_get_last_error(IntPtr handle, IntPtr info);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_init(IntPtr p, out IntPtr h);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_shutdown(IntPtr h, uint timeout, out uint state);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_object_destroy(IntPtr h);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_source_open(IntPtr h, IntPtr p, out IntPtr source);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_source_close(IntPtr h, uint timeout, out uint state);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_capture_sync(IntPtr h, IntPtr p, out IntPtr image);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_image_release(IntPtr h);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_subscribe_frames(IntPtr h, IntPtr p, out IntPtr sub);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_frame_unsubscribe(IntPtr h, uint timeout, out uint state);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_frame_pull(IntPtr h, uint timeout, out IntPtr frame);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_frame_release(IntPtr h);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_frame_get_view(IntPtr h, IntPtr p);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_frame_as_pixel_view(IntPtr h, IntPtr p);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_shared_pool_create(IntPtr h, IntPtr cfg, IntPtr attach, out IntPtr pool);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_shared_pool_publish_view(IntPtr h, IntPtr pixel, out ulong publication);
        [DllImport("am_camera.dll", CallingConvention=Convention)] internal static extern int amc_shared_pool_close(IntPtr h, uint timeout, out uint state);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_init(IntPtr p, out IntPtr h);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_shutdown(IntPtr h, uint timeout, out uint state);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_object_destroy(IntPtr h);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_model_load(IntPtr h, IntPtr p, out IntPtr model);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_model_close(IntPtr h, uint timeout, out uint state);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_infer_image_sync(IntPtr h, IntPtr image, IntPtr p, out IntPtr result, out ulong request);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_infer_image_async(IntPtr h, IntPtr image, IntPtr p, out ulong request);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_result_fetch(IntPtr h, ulong id, out IntPtr result);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_result_ack(IntPtr h, ulong id);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_result_release(IntPtr h);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_request_query(IntPtr h, ulong id, out IntPtr snapshot);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_request_snapshot_release(IntPtr h);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_request_cancel(IntPtr h, ulong id);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_result_notify_callback_set(IntPtr h, Notify callback, IntPtr user);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_shared_pool_attach(IntPtr h, IntPtr attach, out IntPtr pool);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_shared_pool_borrow(IntPtr h, ulong publication, uint timeout, IntPtr view);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_shared_pool_return(IntPtr h, IntPtr view);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_shared_frame_as_pixel_view(IntPtr h, IntPtr view, IntPtr pixel);
        [DllImport("am_vision.dll", CallingConvention=Convention)] internal static extern int amv_shared_pool_detach(IntPtr h, uint timeout, out uint state);
    }

    public sealed class SdkException : Exception {
        public int Status { get; private set; }
        public string SourceKey { get; private set; }
        public string Detail { get; private set; }
        public SdkException(int code, string operation) : base(operation + " failed (SDK status " + code + ")") { Status = code; }
        internal SdkException(int code, string operation, string detail, string source)
            : base(operation + " failed (SDK status " + code + "): " + detail + " [source=" + source + "]") {
            Status = code; Detail = detail; SourceKey = source;
        }
        internal static void Check(int code, string operation) { if (code != 0) throw new SdkException(code, operation); }
    }

    // UTF-8 和结构缓冲仅存活至调用返回；异步入口在接受时已复制输入。
    internal sealed class Arena : IDisposable {
        private readonly List<KeyValuePair<IntPtr, int>> blocks = new List<KeyValuePair<IntPtr, int>>();
        internal IntPtr Allocate(int bytes) {
            IntPtr p = Marshal.AllocHGlobal(bytes);
            for (int i = 0; i < bytes; ++i) Marshal.WriteByte(p, i, 0);
            blocks.Add(new KeyValuePair<IntPtr, int>(p, bytes)); return p;
        }
        internal IntPtr Bytes(byte[] value) { IntPtr p = Allocate(value.Length); Marshal.Copy(value, 0, p, value.Length); return p; }
        internal IntPtr Utf8(string value) { return value == null ? IntPtr.Zero : Bytes(Encoding.UTF8.GetBytes(value + "\0")); }
        internal IntPtr Directories(string[] values) {
            if (values.Length == 0) return IntPtr.Zero;
            IntPtr array = Allocate(IntPtr.Size * values.Length);
            for (int i = 0; i < values.Length; ++i) Marshal.WriteIntPtr(array, i * IntPtr.Size, Utf8(System.IO.Path.GetFullPath(values[i])));
            return array;
        }
        internal IntPtr Struct<T>(T value) where T : struct { IntPtr p = Allocate(Marshal.SizeOf(typeof(T))); Marshal.StructureToPtr(value, p, false); return p; }
        internal static T Read<T>(IntPtr p) where T : struct { return (T)Marshal.PtrToStructure(p, typeof(T)); }
        internal static T Sized<T>() where T : struct { return (T)Initialized(typeof(T)); }
        private static object Initialized(Type type) {
            object value = Activator.CreateInstance(type);
            foreach (FieldInfo field in type.GetFields()) {
                if (field.FieldType.IsArray) {
                    MarshalAsAttribute array = (MarshalAsAttribute)Attribute.GetCustomAttribute(field, typeof(MarshalAsAttribute));
                    field.SetValue(value, Array.CreateInstance(field.FieldType.GetElementType(), array.SizeConst));
                } else if (field.FieldType.IsValueType && !field.FieldType.IsPrimitive && field.FieldType != typeof(IntPtr)) {
                    field.SetValue(value, Initialized(field.FieldType));
                }
            }
            FieldInfo size = type.GetField("struct_size");
            if (size != null) size.SetValue(value, (uint)Marshal.SizeOf(type));
            return value;
        }
        public void Dispose() {
            foreach (KeyValuePair<IntPtr, int> block in blocks) {
                for (int i = 0; i < block.Value; ++i) Marshal.WriteByte(block.Key, i, 0);
                Marshal.FreeHGlobal(block.Key);
            }
            blocks.Clear();
        }
    }
}
