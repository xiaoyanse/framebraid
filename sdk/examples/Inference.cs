using System;
using System.Collections.Generic;
using AmrtSdk;

internal static class InferenceExample {
    private static void Validate(InferenceResult result) {
        Dictionary<string, TensorCopy> tensors = result.CopyRawOutputs();
        if (tensors.Count != 1) throw new Exception("Expected one Identity output");
        foreach (TensorCopy tensor in tensors.Values) {
            if (tensor.Data.Length != 48 || tensor.Shape.Length != 4) throw new Exception("Unexpected output shape");
            for (int i = 0; i < 12; ++i) if (BitConverter.ToSingle(tensor.Data, i * 4) != 17F) throw new Exception("Identity output mismatch");
        }
    }
    private static void Main(string[] args) {
        if (args.Length != 2) throw new ArgumentException("Usage: Inference.exe ORT_DIRECTORY IDENTITY_IMAGE_MODEL");
        using (CameraContext camera = new CameraContext(new string[0], true))
        using (VisionContext vision = new VisionContext(new string[] { args[0] }, true))
        using (Model model = vision.LoadModel(args[1], null, 0, true)) {
            model.EnableNotifications(true);
            am_raw_preprocess_t preprocessing = ImagePreprocessing.StretchRgb(2, 2);
            ulong request;
            using (RgbBuffer buffer = new RgbBuffer(2, 2, new byte[] {17,17,17,17,17,17,17,17,17,17,17,17})) {
                using (InferenceResult result = model.InferPixel(buffer.BorrowPixel(), out request, preprocessing)) {
                    vision.Acknowledge(request); Validate(result);
                }
            }
            using (SharedPublisher publisher = camera.CreatePool(1, 64))
            using (SharedConsumer consumer = vision.Attach(publisher.Descriptor)) {
                ulong publication;
                using (RgbBuffer pixels = new RgbBuffer(2, 2, new byte[] {17,17,17,17,17,17,17,17,17,17,17,17}, FrameAssociation.Identity(1, 1, 1, 0))) publication = publisher.Publish(pixels.BorrowPixel());
                using (SharedBorrow borrowed = consumer.Borrow(publication, 0)) request = model.SubmitPixel(borrowed.BorrowPixel(), preprocessing);
                // 借用已归还，异步推理继续使用接受时的副本。
                vision.Wait(request, 10000);
                using (InferenceResult result = vision.Fetch(request)) { vision.Acknowledge(request); Validate(result); }
            }
            model.EnableNotifications(false);
        }
        Console.WriteLine("C# inference passed: CPU sync/async, fetch/ack, shared lease, SafeHandle");
    }
}
