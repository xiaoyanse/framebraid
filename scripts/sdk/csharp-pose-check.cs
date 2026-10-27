using System;
using System.Runtime.InteropServices;
using System.Threading;
using AmrtSdk;
internal static class PoseCheck {
    static void Check(bool v) { if (!v) throw new Exception("Pose assertion failed"); }
    static void Reject(Action action) { try { action(); throw new Exception("Invalid input accepted"); } catch (InvalidOperationException) {} }
    static void Main(string[] args) {
        Check(Marshal.SizeOf(typeof(am_pose_detection_t)) == 32 && Marshal.OffsetOf(typeof(am_pose_detection_t), "keypoints").ToInt32() == 24);
        Check(Marshal.SizeOf(typeof(am_keypoint_t)) == 12 && Marshal.OffsetOf(typeof(am_keypoint_t), "confidence").ToInt32() == 8);
        IntPtr points = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(am_keypoint_t)));
        IntPtr rows = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(am_pose_detection_t)));
        IntPtr raw = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(amv_result_t)));
        bool released = false;
        try {
            am_keypoint_t point = new am_keypoint_t { x = .1f, y = .2f, confidence = .3f };
            Marshal.StructureToPtr(point, points, false);
            am_pose_detection_t row = new am_pose_detection_t { bounds = new float[] { 0, .25f, .5f, 1 }, confidence = .75f, kpt_count = 1, keypoints = points };
            Marshal.StructureToPtr(row, rows, false);
            amv_result_t value = Arena.Sized<amv_result_t>();
            value.task = 1; value.pose.count = 1; value.pose.detections = rows;
            Marshal.StructureToPtr(value, raw, false);
            InferenceResult result = new InferenceResult(raw, delegate(IntPtr p) { released = true; return 0; });
            PoseDetectionCopy[] copy = result.CopyPose();
            Check(copy.Length == 1 && copy[0].Keypoints.Length == 1 && copy[0].Keypoints[0].X == .1f);
            point.x = .9f; Marshal.StructureToPtr(point, points, false);
            Check(copy[0].Keypoints[0].X == .1f);
            value.pose.count = 301; Marshal.StructureToPtr(value, raw, false); Reject(delegate { result.CopyPose(); });
            value.pose.count = 1; value.pose.detections = IntPtr.Zero; Marshal.StructureToPtr(value, raw, false); Reject(delegate { result.CopyPose(); });
            value.pose.count = 0; Marshal.StructureToPtr(value, raw, false); Check(result.CopyPose().Length == 0);
            value.pose.detections = rows; value.pose.count = 1; value.task = 0; Marshal.StructureToPtr(value, raw, false); Reject(delegate { result.CopyPose(); });
            value.task = 1; Marshal.StructureToPtr(value, raw, false);
            row.kpt_count = 1025; Marshal.StructureToPtr(row, rows, false); Reject(delegate { result.CopyPose(); });
            row.kpt_count = 1; row.keypoints = IntPtr.Zero; Marshal.StructureToPtr(row, rows, false); Reject(delegate { result.CopyPose(); });
            row.keypoints = points; Marshal.StructureToPtr(row, rows, false);
            ManualResetEvent entered = new ManualResetEvent(false), proceed = new ManualResetEvent(false), ended = new ManualResetEvent(false);
            Exception failure = null;
            Thread reader = new Thread(delegate() { try { lock (result.Gate) { entered.Set(); Check(proceed.WaitOne(5000)); result.CopyPose(); } } catch (Exception e) { failure = e; } });
            reader.Start(); Check(entered.WaitOne(5000));
            Thread closer = new Thread(delegate() { result.Dispose(); ended.Set(); });
            closer.Start(); Check(!ended.WaitOne(50)); proceed.Set(); Check(reader.Join(5000)); Check(closer.Join(5000)); Check(released && failure == null);
            try { result.CopyPose(); throw new Exception("Closed result accepted"); } catch (ObjectDisposedException) {}
            Check(copy[0].Keypoints[0].X == .1f);
        } finally { Marshal.FreeHGlobal(raw); Marshal.FreeHGlobal(rows); Marshal.FreeHGlobal(points); }
        if (args.Length == 2) {
            using (VisionContext vision = new VisionContext(new string[] { args[0] }, false))
            using (Model model = vision.LoadModel(args[1], "pose-yolo26-e2e-coco17-v1", 0, true))
            using (RgbBuffer image = new RgbBuffer(640, 640, new byte[640*640*3])) {
                ulong request;
                using (InferenceResult result = image.Pixels().Infer(model, out request, null)) {
                    PoseDetectionCopy[] copy = result.CopyPose(); Check(copy.Length == result.BorrowView().pose.count);
                    vision.Acknowledge(request); Console.WriteLine("real_cpu_pose_count=" + copy.Length);
                }
            }
        }
        Console.WriteLine("synthetic deep copy, invalid input, close competition passed");
    }
}
