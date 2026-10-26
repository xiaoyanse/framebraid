using System;
using System.Threading;
using AmrtSdk;

internal static class OwnedPixelsCheck {
    private static void Require(bool condition) { if (!condition) throw new Exception("Owned pixels assertion failed"); }
    private static void Validate(InferenceResult result) {
        foreach (TensorCopy tensor in result.CopyRawOutputs().Values) {
            Require(tensor.Data.Length == 48);
            for (int i = 0; i < 12; ++i) Require(BitConverter.ToSingle(tensor.Data, i * 4) == 17F);
        }
    }
    private static void MakeRetained(out OwnedPixels pixels, out WeakReference weak) {
        RgbBuffer owner = new RgbBuffer(2, 2, new byte[12]);
        weak = new WeakReference(owner); pixels = owner.Pixels();
    }
    private static void Main(string[] args) {
        byte[] bytes = new byte[12];
        bytes[0] = 17;
        using (RgbBuffer legacy = new RgbBuffer(2, 2, bytes)) {
            bytes[0] = 18; Require(legacy.Pixels().CopyPlanes()[0][0] == 18);
        }
        bytes[0] = 17;
        RgbBuffer buffer = RgbBuffer.Copy(2, 2, bytes);
        OwnedPixels pixels = buffer.Pixels();
        bytes[0] = 0;
        Require(pixels.CopyPlanes()[0][0] == 17);
        buffer.Dispose(); buffer.Dispose();
        try { pixels.CopyPlanes(); throw new Exception("Closed owner accepted"); }
        catch (ObjectDisposedException) { }
        RgbBuffer racing = new RgbBuffer(2, 2, new byte[12]);
        ManualResetEvent entered = new ManualResetEvent(false), release = new ManualResetEvent(false), ended = new ManualResetEvent(false);
        // 真正拥有者锁由反射读取仅用于测试 Dispose 竞争，不向公开门面暴露。
        object gate = typeof(RgbBuffer).GetField("gate", System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance).GetValue(racing);
        OwnedPixels blocked = new OwnedPixels(racing, gate, delegate { entered.Set(); Require(release.WaitOne(5000)); return racing.BorrowPixel(); });
        Exception failure = null;
        Thread reader = new Thread(delegate() { try { blocked.CopyPlanes(); } catch (Exception e) { failure = e; } });
        reader.Start(); Require(entered.WaitOne(5000));
        Thread closer = new Thread(delegate() { racing.Dispose(); ended.Set(); });
        closer.Start(); Require(!ended.WaitOne(50)); release.Set(); Require(reader.Join(5000)); Require(closer.Join(5000)); Require(failure == null);
        entered.Dispose(); release.Dispose(); ended.Dispose();
        OwnedPixels retained;
        WeakReference weak;
        MakeRetained(out retained, out weak);
        GC.Collect(); GC.WaitForPendingFinalizers(); Require(weak.IsAlive);
        Require(retained.CopyPlanes()[0].Length == 12);
        try { ulong id; retained.Infer(null, out id); throw new Exception("Null model accepted"); }
        catch (NullReferenceException) { }
        ((RgbBuffer)weak.Target).Dispose();
        try { retained.CopyPlanes(); throw new Exception("Closed GC owner accepted"); }
        catch (ObjectDisposedException) { }
        if (args.Length == 2) {
            using (CameraContext camera = new CameraContext(new string[0], true))
            using (VisionContext vision = new VisionContext(new string[] { args[0] }, true))
            using (Model first = vision.LoadModel(args[1], null, 0, true))
            using (Model second = vision.LoadModel(args[1], null, 0, true))
            using (SharedPublisher publisher = camera.CreatePool(1, 64))
            using (SharedConsumer consumer = vision.Attach(publisher.Descriptor)) {
                am_raw_preprocess_t prep = ImagePreprocessing.StretchRgb(2, 2);
                byte[] rgb = new byte[12]; for (int i = 0; i < rgb.Length; ++i) rgb[i] = 17;
                ulong publication;
                using (RgbBuffer source = new RgbBuffer(2, 2, rgb, FrameAssociation.Identity(1, 1, 1, 0))) {
                    ulong request;
                    using (InferenceResult result = source.Pixels().Infer(first, out request, prep)) { vision.Acknowledge(request); Validate(result); }
                    using (InferenceResult result = source.Pixels().Infer(second, out request, prep)) { vision.Acknowledge(request); Validate(result); }
                    publication = source.Pixels().Publish(publisher);
                }
                ulong one, two;
                OwnedPixels shared;
                using (SharedBorrow borrowed = consumer.Borrow(publication, 0)) {
                    try { consumer.Close(0); throw new Exception("Consumer closed with live borrow"); }
                    catch (InvalidOperationException) { }
                    shared = borrowed.Pixels(); one = shared.Submit(first, prep); two = shared.Submit(second, prep);
                    borrowed.Dispose();
                }
                try { shared.CopyPlanes(); throw new Exception("Returned lease accepted"); } catch (ObjectDisposedException) { }
                GC.Collect(); GC.WaitForPendingFinalizers();
                foreach (ulong request in new ulong[] {one, two}) {
                    vision.Wait(request, 10000);
                    using (InferenceResult result = vision.Fetch(request)) { vision.Acknowledge(request); Validate(result); }
                }
            }
        }
        Console.WriteLine("C# input copy/closed owner/Dispose competition passed; runtime="+(args.Length == 2));
    }
}
