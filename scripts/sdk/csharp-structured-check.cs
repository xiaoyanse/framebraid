using System;
using System.Runtime.InteropServices;
using System.Threading;
using AmrtSdk;
internal static class StructuredCheck {
    static void Check(bool value) { if (!value) throw new Exception("structured assertion failed"); }
    static void Reject(Action action) { try { action(); throw new Exception("invalid result accepted"); } catch (InvalidOperationException) {} }
    static void Put<T>(IntPtr ptr, T value) { Marshal.StructureToPtr(value, ptr, false); }
    static void Main() {
        Check(Marshal.SizeOf(typeof(am_detect_box_t)) == 32 && Marshal.OffsetOf(typeof(am_detect_box_t), "class_name_utf8").ToInt32() == 24);
        Check(Marshal.SizeOf(typeof(am_classify_entry_t)) == 32 && Marshal.OffsetOf(typeof(am_classify_entry_t), "class_name_utf8").ToInt32() == 8);
        Check(Marshal.SizeOf(typeof(am_segment_instance_t)) == 72 && Marshal.OffsetOf(typeof(am_segment_instance_t), "mask_bits").ToInt32() == 40);
        Check(Marshal.SizeOf(typeof(am_obb_instance_t)) == 64 && Marshal.OffsetOf(typeof(am_obb_instance_t), "angle_normalized").ToInt32() == 60);
        IntPtr raw = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(amv_result_t)));
        IntPtr row = Marshal.AllocHGlobal(256), mask = Marshal.AllocHGlobal(4), name = Marshal.StringToHGlobalAnsi("person");
        bool released = false;
        try {
            amv_result_t r = Arena.Sized<amv_result_t>();
            InferenceResult result = new InferenceResult(raw, delegate(IntPtr p) { released = true; return 0; });
            r.task = 2; r.detect.count = 1; r.detect.boxes = row; Put(raw, r);
            am_detect_box_t d = new am_detect_box_t { bounds = new float[] { 1,2,3,4 }, class_id = 7, class_name_utf8 = name }; Put(row, d);
            DetectCopy[] detect = result.CopyDetect(); Check(detect[0].ClassId == 7 && detect[0].ClassName == "person");
            d.bounds[0] = 9; Put(row, d); Check(detect[0].Bounds[0] == 1);
            r.detect.count = 301; Put(raw, r); Reject(delegate { result.CopyDetect(); });
            r.detect.count = 1; r.detect.boxes = IntPtr.Zero; Put(raw, r); Reject(delegate { result.CopyDetect(); });
            r.task = 3; r.classify.count = 1; r.classify.entries = row; Put(raw, r);
            am_classify_entry_t c = new am_classify_entry_t { class_id = 9, probability = .7f, logit = 3.5f }; Put(row, c);
            ClassifyCopy[] classify = result.CopyClassify(); Check(classify[0].Logit == 3.5f && !classify[0].HasProbability && classify[0].ClassName == null);
            r.classify.count = 1001; Put(raw, r); Reject(delegate { result.CopyClassify(); });
            r.classify.count = 1; r.classify.entries = IntPtr.Zero; Put(raw, r); Reject(delegate { result.CopyClassify(); });
            r.task = 4; r.segment.count = 1; r.segment.instances = row; Put(raw, r);
            Marshal.WriteByte(mask, 0, 5); Marshal.WriteByte(mask, 1, 2); Marshal.WriteByte(mask, 2, 7); Marshal.WriteByte(mask, 3, 3);
            am_segment_instance_t s = new am_segment_instance_t { bounds = new float[] { 0,0,1,1 }, mask_width = 9, mask_height = 2, mask_stride_bytes = 2, mask_bits = mask }; Put(row, s);
            SegmentCopy[] segment = result.CopySegment(); Check(segment[0].Mask.Length == 4 && segment[0].Mask[0] == 5);
            Marshal.WriteByte(mask, 0, 9); Check(segment[0].Mask[0] == 5);
            s.mask_stride_bytes = 1; Put(row, s); Reject(delegate { result.CopySegment(); });
            s.mask_stride_bytes = 2; s.mask_bits = IntPtr.Zero; Put(row, s); Reject(delegate { result.CopySegment(); });
            s.mask_bits = mask; s.mask_stride_bytes = 67108864; Put(row, s); Reject(delegate { result.CopySegment(); });
            s.mask_stride_bytes = 2; s.mask_width = 8193; Put(row, s); Reject(delegate { result.CopySegment(); });
            s.mask_width = 8; s.mask_height = 1; s.mask_stride_bytes = 33554433;
            Put(row, s); Put(IntPtr.Add(row, Marshal.SizeOf(typeof(am_segment_instance_t))), s);
            r.segment.count = 2; Put(raw, r); Reject(delegate { result.CopySegment(); });
            r.segment.count = 1;
            r.task = 5; r.obb.count = 1; r.obb.instances = row; Put(raw, r);
            am_obb_instance_t o = new am_obb_instance_t { vertices_xy = new float[] {1,2,3,4,5,6,7,8}, angle = .5f, angle_normalized = 1 }; Put(row, o);
            ObbCopy[] obb = result.CopyObb(); Check(obb[0].Vertices[0] == 1 && obb[0].AngleNormalized);
            o.vertices_xy[0] = 9; Put(row, o); Check(obb[0].Vertices[0] == 1);
            Reject(delegate { result.CopyDetect(); });
            r.obb.count = 301; Put(raw, r); Reject(delegate { result.CopyObb(); });
            r.obb.count = 1; r.obb.instances = IntPtr.Zero; Put(raw, r); Reject(delegate { result.CopyObb(); });
            r.obb.instances = row; Put(raw, r);
            ManualResetEvent entered = new ManualResetEvent(false), proceed = new ManualResetEvent(false), ended = new ManualResetEvent(false);
            Exception failure = null;
            Thread reader = new Thread(delegate() { try { lock (result.Gate) { entered.Set(); Check(proceed.WaitOne(5000)); result.CopyObb(); } } catch (Exception e) { failure = e; } });
            reader.Start(); Check(entered.WaitOne(5000));
            Thread closer = new Thread(delegate() { result.Dispose(); ended.Set(); }); closer.Start();
            Check(!ended.WaitOne(50)); proceed.Set(); Check(reader.Join(5000) && closer.Join(5000) && released && failure == null);
            try { result.CopyObb(); throw new Exception("closed result accepted"); } catch (ObjectDisposedException) {}
            Check(detect[0].ClassName == "person" && classify[0].ClassId == 9 && segment[0].Mask[0] == 5 && obb[0].Vertices[0] == 1);
        } finally { Marshal.FreeHGlobal(raw); Marshal.FreeHGlobal(row); Marshal.FreeHGlobal(mask); Marshal.FreeHGlobal(name); }
        Console.WriteLine("C# structured copy, ABI layout, invalid data and close competition passed");
    }
}
