using System;
using System.IO;
using AmrtSdk;

internal static class SelectionExample {
    private static void Require(bool condition) { if (!condition) throw new Exception("Selection contract mismatch"); }
    private static void Main(string[] args) {
        if (args.Length != 2) throw new ArgumentException("Usage: Selection.exe CPU_ORT_DIRECTORY IDENTITY_IMAGE_MODEL");
        using (VisionContext vision = new VisionContext(new string[] { args[0] }, false)) {
            am_backend_candidate_t[] cpu = { new am_backend_candidate_t { backend = 0, device_id = 0 } };
            ulong attempt;
            using (Model model = vision.LoadModelSelected(args[1], cpu, out attempt)) {
                Require(attempt != 0 && vision.GetSelectionAttemptReport(attempt).Contains("\"ready\""));
                ulong request;
                using (RgbBuffer buffer = new RgbBuffer(2, 2, new byte[12]))
                using (InferenceResult result = model.InferPixel(buffer.BorrowPixel(), out request, ImagePreprocessing.StretchRgb(2, 2))) {
                    vision.Acknowledge(request);
                    Require(result.CopyRawOutputs().Count == 1);
                }
                Require(model.GetSelectionReport().Contains("\"native_runs\":1"));
            }
            using (Model model = vision.LoadModelMemorySelected(File.ReadAllBytes(args[1]), cpu, out attempt))
                Require(model.GetSelectionReport().Contains("\"backend\":0"));
            am_backend_candidate_t[] cuda = { new am_backend_candidate_t { backend = 2, device_id = 0 } };
            bool refused = false;
            try { using (Model unexpected = vision.LoadModelSelected(args[1], cuda, out attempt, null, false)) { } }
            catch (SdkException) {
                refused = true;
                Require(attempt != 0 && vision.GetSelectionAttemptReport(attempt).Contains("\"failed\""));
            }
            Require(refused);
        }
        Console.WriteLine("C# ordered selection passed: path/memory, CPU Run, strict rejection, attempt and model reports");
    }
}
