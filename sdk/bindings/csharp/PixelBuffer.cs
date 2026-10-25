using System;
using System.Runtime.InteropServices;

namespace AmrtSdk {
    public static class FrameAssociation {
        public static am_frame_assoc_t Identity(ulong sourceId, ulong frameId, uint generation, uint configGeneration) {
            am_frame_assoc_t value = Arena.Sized<am_frame_assoc_t>();
            value.source_id = sourceId; value.frame_id = frameId; value.source_generation = generation;
            value.config_generation = configGeneration; value.valid = 1 | 2 | 4;
            value.pts = value.captured_at_ms = value.received_at_ms = Int64.MinValue;
            return value;
        }
    }
    // 调用方自有 RGB8 数据；异步默认复制，Submit 返回后即可 Dispose。
    public sealed class RgbBuffer : IDisposable {
        private GCHandle pin;
        private IntPtr view;
        private IntPtr association;
        public RgbBuffer(uint width, uint height, byte[] bytes, am_frame_assoc_t? frameAssociation = null) {
            if (width == 0 || height == 0 || bytes.LongLength != checked((long)width * height * 3)) throw new ArgumentException("RGB8 byte count must equal width * height * 3");
            pin = GCHandle.Alloc(bytes, GCHandleType.Pinned);
            try {
                am_frame_view_t frame = Arena.Sized<am_frame_view_t>();
                frame.width = width; frame.height = height; frame.format = 1; frame.plane_count = 1;
                frame.planes[0] = pin.AddrOfPinnedObject(); frame.strides[0] = checked(width * 3); frame.plane_size_bytes[0] = (ulong)bytes.LongLength;
                view = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(am_frame_view_t))); Marshal.StructureToPtr(frame, view, false);
                if (frameAssociation.HasValue) {
                    association = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(am_frame_assoc_t)));
                    Marshal.StructureToPtr(frameAssociation.Value, association, false);
                }
            } catch { Dispose(); throw; }
        }
        public am_pixel_view_t BorrowPixel() {
            if (view == IntPtr.Zero) throw new ObjectDisposedException("RgbBuffer");
            am_pixel_view_t pixel = Arena.Sized<am_pixel_view_t>(); pixel.magic = 1095585880; pixel.view = view; pixel.assoc = association; return pixel;
        }
        public void Dispose() {
            if (association != IntPtr.Zero) { Marshal.FreeHGlobal(association); association = IntPtr.Zero; }
            if (view != IntPtr.Zero) { Marshal.FreeHGlobal(view); view = IntPtr.Zero; }
            if (pin.IsAllocated) pin.Free();
        }
    }
}
