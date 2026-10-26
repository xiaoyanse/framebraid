"""真实 DLL 通知接线加语言侧积压/并发验证；不启动相机或业务服务。"""
import argparse
import ctypes as c
import json
import os
from pathlib import Path
import queue
import subprocess
import sys

from local_paths import local_path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
import amrt_sdk as a
from safe_sdk import VisionContext, sized


CSHARP = r'''
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Reflection;
using System.Threading;
using AmrtSdk;
class Notifications {
    static void Require(bool ok, string reason) { if (!ok) throw new Exception(reason); }
    static void Main(string[] args) {
        using (VisionContext ctx = new VisionContext(new [] { args[0] }, false))
        using (Model model = ctx.LoadModel(args[1], null, 0, true)) {
            model.EnableNotifications(true);
            using (RgbBuffer pixels = new RgbBuffer(2, 2, new byte[12])) {
                ulong request;
                using (InferenceResult result = model.InferPixel(pixels.BorrowPixel(), out request, ImagePreprocessing.StretchRgb(2, 2))) {
                    Require(result.CopyRawOutputs().Count == 1, "actual inference output");
                    ctx.Acknowledge(request);
                }
            }
            Tuple<ulong, uint> item;
            bool actual = false;
            for (int i = 0; i < 1000 && !actual; ++i) {
                actual = model.TryTakeNotification(out item);
                if (!actual) Thread.Sleep(1);
            }
            Require(actual, "actual native callback did not arrive");
            var callbacks = (ConcurrentDictionary<IntPtr, Native.Notify>)typeof(Model).GetField("callbacks", BindingFlags.NonPublic | BindingFlags.Static).GetValue(null);
            Native.Notify callback = callbacks[model.Raw];
            for (ulong i = 0; i < 10000; ++i) callback(model.Raw, i, 2, IntPtr.Zero);
            Require(model.NotificationsDropped == 10000 - Model.NotificationCapacity, "drop count");
            int count = 0;
            while (model.TryTakeNotification(out item)) {
                Require(item.Item1 == (ulong)(10000 - Model.NotificationCapacity + count), "latest FIFO order");
                ++count;
            }
            Require(count == Model.NotificationCapacity, "bounded capacity");
            long before = model.NotificationsDropped;
            Thread[] workers = new Thread[4];
            for (int j = 0; j < workers.Length; ++j) {
                int index = j;
                workers[j] = new Thread(delegate() { for (ulong i = 0; i < 1000; ++i) callback(model.Raw, (ulong)index * 1000 + i, 2, IntPtr.Zero); });
                workers[j].Start();
            }
            HashSet<ulong> consumed = new HashSet<ulong>();
            bool running;
            do {
                while (model.TryTakeNotification(out item)) Require(consumed.Add(item.Item1), "duplicate notification");
                running = false;
                foreach (Thread worker in workers) running |= worker.IsAlive;
            } while (running);
            foreach (Thread worker in workers) worker.Join();
            while (model.TryTakeNotification(out item)) Require(consumed.Add(item.Item1), "duplicate notification");
            Require(consumed.Count + model.NotificationsDropped - before == 4000, "concurrent accounting");
            GC.Collect(); GC.WaitForPendingFinalizers();
            model.EnableNotifications(false);
            Require(!callbacks.ContainsKey(model.Raw), "callback root survived unregister");
            model.EnableNotifications(true);
            model.EnableNotifications(false);
            Require(!callbacks.ContainsKey(model.Raw), "callback replacement root survived");
            Console.WriteLine("C# actual inference/callback, bounded 256/9744, concurrent 4000 accounting and unregister passed");
        }
    }
}
'''


def python_runtime(dll_dir, runtime, model_path):
    storage = (c.c_ubyte * 12)(*[17] * 12)
    frame = sized(a.am_frame_view_t)
    frame.width, frame.height, frame.format, frame.plane_count = 2, 2, a.AM_PF_RGB8, 1
    frame.planes[0], frame.strides[0], frame.plane_size_bytes[0] = storage, 6, 12
    pixel = sized(a.am_pixel_view_t)
    pixel.magic, pixel.view = a.AM_PIXEL_VIEW_MAGIC, c.pointer(frame)
    prep = sized(a.am_raw_preprocess_t)
    prep.resize, prep.target_w, prep.target_h = a.AM_RESIZE_STRETCH, 2, 2
    with VisionContext(dll_dir / "am_vision.dll", [runtime]) as context, context.load_model(model_path) as model:
        model.enable_notifications()
        request, result = model.infer_pixel(pixel, preprocess=prep)
        with result:
            assert len(result.raw_outputs()) == 1
            context.ack(request)
        actual = model.notifications.get(timeout=2)
        assert actual == (request, a.AM_REQ_DONE), actual
        model.enable_notifications(False)
    return {"actualInference": "passed", "actualCallback": "passed", "explicitClose": "passed"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--model", type=Path, required=True)
    args = parser.parse_args()
    output = local_path(args.output, "tests")
    output.mkdir(parents=True, exist_ok=True)
    # 原实现 SimpleQueue 的同负载基线，只保留计数，不模拟推理性能。
    baseline = queue.SimpleQueue()
    for index in range(10000):
        baseline.put((index, a.AM_REQ_DONE))
    baseline_count = baseline.qsize()
    del baseline
    summary = {"baselinePending10000": baseline_count, "candidateCapacity": 256,
               "python": python_runtime(args.dll_dir.resolve(), args.runtime_dir.resolve(), args.model.resolve())}
    source, executable = output / "Notifications.cs", output / "Notifications.exe"
    source.write_text(CSHARP, encoding="utf-8")
    csc = Path("C:/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe")
    command = [str(csc), "/nologo", "/platform:x64", "/warnaserror", "/out:" + str(executable),
               *map(str, sorted((ROOT / "sdk/bindings/csharp").glob("*.cs"))), str(source)]
    compiled = subprocess.run(command, capture_output=True, text=True)
    if compiled.returncode:
        raise RuntimeError(compiled.stdout + compiled.stderr)
    env = os.environ.copy()
    env["PATH"] = str(args.dll_dir.resolve()) + os.pathsep + env.get("PATH", "")
    process = subprocess.run([str(executable), str(args.runtime_dir.resolve()), str(args.model.resolve())],
                             capture_output=True, text=True, env=env, timeout=30)
    if process.returncode:
        raise RuntimeError(process.stdout + process.stderr)
    summary["csharp"] = process.stdout.strip()
    (output / "notification-results.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=False))


if __name__ == "__main__":
    main()
