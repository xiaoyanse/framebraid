using System;
using System.Runtime.InteropServices;

namespace AmrtSdk {
    // 只读操作门面，不导出指针；关闭原拥有者后失效，调用期间持锁保护。
    public sealed class OwnedPixels {
        private readonly object owner;
        private readonly object gate;
        private readonly Func<am_pixel_view_t> getPixel;
        internal OwnedPixels(object lifetime, object ownerGate, Func<am_pixel_view_t> getter) {
            owner = lifetime; gate = ownerGate; getPixel = getter;
        }
        public InferenceResult Infer(Model model, out ulong request, am_raw_preprocess_t? preprocessing = null) {
            lock (gate) {
                try { return model.InferPixel(getPixel(), out request, preprocessing); }
                finally { GC.KeepAlive(owner); }
            }
        }
        // SDK 默认复制接受的异步输入，只需保护到提交返回，不承诺全链零拷贝。
        public ulong Submit(Model model, am_raw_preprocess_t? preprocessing = null) {
            lock (gate) {
                try { return model.SubmitPixel(getPixel(), preprocessing); }
                finally { GC.KeepAlive(owner); }
            }
        }
        public ulong Publish(SharedPublisher publisher) {
            lock (gate) {
                try { return publisher.Publish(getPixel()); }
                finally { GC.KeepAlive(owner); }
            }
        }
        public byte[][] CopyPlanes() {
            lock (gate) {
                try {
                    am_pixel_view_t pixel = getPixel();
                    if (pixel.view == IntPtr.Zero) throw new InvalidOperationException("Pixel view is null");
                    am_frame_view_t frame = Arena.Read<am_frame_view_t>(pixel.view);
                    if (frame.width == 0 || frame.height == 0 || frame.plane_count < 1 || frame.plane_count > 4)
                        throw new InvalidOperationException("Invalid plane dimensions/count");
                    if (frame.format > 3 || frame.plane_count != (frame.format == 3 ? 2U : 1U))
                        throw new InvalidOperationException("Invalid pixel format/plane count");
                    byte[][] planes = new byte[frame.plane_count][];
                    for (int i = 0; i < planes.Length; ++i) {
                        ulong rows = frame.height, rowBytes = frame.width;
                        if (frame.format == 3 && i == 1) { rows = ((ulong)frame.height + 1) / 2; rowBytes = ((ulong)frame.width + 1) / 2 * 2; }
                        else if (frame.format != 3) rowBytes *= frame.format == 0 ? 4UL : 3UL;
                        if (frame.planes[i] == IntPtr.Zero || frame.strides[i] < rowBytes ||
                            frame.plane_size_bytes[i] < (rows - 1) * frame.strides[i] + rowBytes || frame.plane_size_bytes[i] > Int32.MaxValue)
                            throw new InvalidOperationException("Invalid plane pointer/size/stride");
                        planes[i] = new byte[checked((int)frame.plane_size_bytes[i])];
                        Marshal.Copy(frame.planes[i], planes[i], 0, planes[i].Length);
                    }
                    return planes;
                } finally { GC.KeepAlive(owner); }
            }
        }
    }
}
