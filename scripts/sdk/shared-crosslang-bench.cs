// R2 1.4 跨语言搬运量化：C# 发布/借用/托管复制/归还，与 Python 基线同构。
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using AmrtSdk;

internal static class SharedCrosslangBench {
    private const uint PixelViewMagic = 1095585880; // AM_PIXEL_VIEW_MAGIC
    private const uint PfRgba8 = 0;                 // AM_PF_RGBA8

    private static double[] Measure(SharedPublisher publisher, SharedConsumer consumer,
                                    IntPtr pixelPtr, int warmup, int samples) {
        double[] rows = new double[4 * samples];
        for (int i = 0; i < warmup + samples; ++i) {
            long t0 = Stopwatch.GetTimestamp();
            ulong publication = publisher.Publish((am_pixel_view_t)Marshal.PtrToStructure(pixelPtr, typeof(am_pixel_view_t)));
            long t1 = Stopwatch.GetTimestamp();
            using (SharedBorrow borrowed = consumer.Borrow(publication, 0)) {
                long t2 = Stopwatch.GetTimestamp();
                am_shared_frame_view_t view = (am_shared_frame_view_t)Marshal.PtrToStructure(
                    borrowed.Raw, typeof(am_shared_frame_view_t));
                byte[] copied = new byte[(int)view.desc.plane_valid_bytes[0]];
                Marshal.Copy(view.plane_data[0], copied, 0, copied.Length);
                long t3 = Stopwatch.GetTimestamp();
                if (copied[0] != 17 || copied[copied.Length - 1] != 23) throw new Exception("borrowed bytes mismatch");
                borrowed.Close(5000);
                long t4 = Stopwatch.GetTimestamp();
                if (i >= warmup) {
                    int at = 4 * (i - warmup);
                    double hz = Stopwatch.Frequency;
                    rows[at] = (t1 - t0) * 1e6 / hz;
                    rows[at + 1] = (t2 - t1) * 1e6 / hz;
                    rows[at + 2] = (t3 - t2) * 1e6 / hz;
                    rows[at + 3] = (t4 - t3) * 1e6 / hz;
                }
            }
        }
        return rows;
    }

    private static string RowJson(int round, double[] rows) {
        string[] names = new string[] { "publish_us", "borrow_us", "copy_us", "return_us" };
        System.Text.StringBuilder builder = new System.Text.StringBuilder();
        builder.Append("{\"round\":").Append(round);
        for (int k = 0; k < 4; ++k) {
            double[] values = new double[rows.Length / 4];
            for (int i = 0; i < values.Length; ++i) values[i] = rows[4 * i + k];
            Array.Sort(values);
            builder.Append(",\"").Append(names[k]).Append("_p50_us\":").Append(values[values.Length / 2].ToString("F1", System.Globalization.CultureInfo.InvariantCulture));
            builder.Append(",\"").Append(names[k]).Append("_p95_us\":").Append(values[(int)(values.Length * 0.95) - 1].ToString("F1", System.Globalization.CultureInfo.InvariantCulture));
        }
        builder.Append("}");
        return builder.ToString();
    }

    private static void Main() {
        int width = 1920, height = 1080;
        byte[] data = new byte[width * height * 4];
        data[0] = 17; data[data.Length - 1] = 23;
        GCHandle pin = GCHandle.Alloc(data, GCHandleType.Pinned);
        IntPtr framePtr = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(am_frame_view_t)));
        IntPtr assocPtr = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(am_frame_assoc_t)));
        IntPtr pixelPtr = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(am_pixel_view_t)));
        try {
            am_frame_view_t frame = new am_frame_view_t();
            frame.struct_size = (uint)Marshal.SizeOf(typeof(am_frame_view_t));
            frame.width = (uint)width; frame.height = (uint)height; frame.format = PfRgba8;
            frame.plane_count = 1;
            frame.planes = new IntPtr[4]; frame.planes[0] = pin.AddrOfPinnedObject();
            frame.strides = new uint[4]; frame.strides[0] = (uint)(width * 4);
            frame.plane_size_bytes = new ulong[4]; frame.plane_size_bytes[0] = (ulong)data.Length;
            Marshal.StructureToPtr(frame, framePtr, false);
            am_frame_assoc_t assoc = new am_frame_assoc_t();
            assoc.struct_size = (uint)Marshal.SizeOf(typeof(am_frame_assoc_t));
            assoc.source_id = 100; assoc.frame_id = 99; assoc.source_generation = 8; assoc.config_generation = 12;
            assoc.pts = 91919; assoc.time_base_num = 1; assoc.time_base_den = 90000;
            assoc.captured_at_ms = 12345; assoc.received_at_ms = 12350;
            assoc.clock_mask = 7; assoc.clock_quality = 2; assoc.valid = 31;
            Marshal.StructureToPtr(assoc, assocPtr, false);
            am_pixel_view_t pixel = new am_pixel_view_t();
            pixel.struct_size = (uint)Marshal.SizeOf(typeof(am_pixel_view_t));
            pixel.magic = PixelViewMagic; pixel.view = framePtr; pixel.assoc = assocPtr;
            Marshal.StructureToPtr(pixel, pixelPtr, false);
            using (CameraContext camera = new CameraContext(new string[0], true))
            using (VisionContext vision = new VisionContext(new string[0], true))
            using (SharedPublisher publisher = camera.CreatePool(1, (uint)data.Length))
            using (SharedConsumer consumer = vision.Attach(publisher.Descriptor)) {
                Console.Out.WriteLine("{\"frame_bytes\":" + data.Length + ",\"warmup\":10,\"samples\":60," +
                    "\"scope\":\"same-process synthetic C# marshal copy; no decode/inference\"}");
                for (int round = 1; round <= 3; ++round)
                    Console.Out.WriteLine(RowJson(round, Measure(publisher, consumer, pixelPtr, 10, 60)));
            }
        } finally {
            Marshal.FreeHGlobal(pixelPtr); Marshal.FreeHGlobal(assocPtr); Marshal.FreeHGlobal(framePtr);
            pin.Free();
        }
    }
}
