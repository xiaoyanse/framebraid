"""语言绑定装载诊断：真实缺文件、受控系统错误和可选真实 DLL，不改系统依赖。"""
import argparse
import ctypes as c
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdk" if (ROOT / "sdk").is_dir() else ROOT
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a
from safe_sdk import SdkError, VisionContext, CameraContext, copied_error, sized
from load_diagnostics import SdkLoadError, load_library


class LoaderTests(unittest.TestCase):
    def test_missing_exports_are_rejected_before_initialization(self):
        path = Path("incompatible/am_vision.dll").resolve()
        with patch("safe_sdk.load_library", return_value=SimpleNamespace()):
            with self.assertRaisesRegex(RuntimeError, "deploy matching SDK and bindings") as caught:
                VisionContext(path, shared_frames=True)
        self.assertIsInstance(caught.exception.__cause__, RuntimeError)
        self.assertIn("SDK symbols missing", str(caught.exception.__cause__))
        self.assertIn("amv_init", str(caught.exception.__cause__))
        self.assertIn(str(path), str(caught.exception))

    def test_missing_file_preserves_system_exception(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(SdkLoadError) as caught:
                load_library(Path(folder) / "am_vision.dll")
            self.assertIn("SDK file is missing", str(caught.exception))
            self.assertIsInstance(caught.exception.__cause__, OSError)
            self.assertIn("process", str(caught.exception))

    def test_real_invalid_image(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "invalid.dll"
            path.write_bytes(b"not a native library")
            with self.assertRaises(SdkLoadError) as caught:
                load_library(path)
            self.assertIsInstance(caught.exception.__cause__, OSError)
            if os.name == "nt":
                self.assertEqual(caught.exception.system_code, 193)
                self.assertIn("architecture mismatch", str(caught.exception))

    def test_loader_codes_have_specific_non_speculative_hints(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "exists.dll"
            path.write_bytes(b"fixture")
            for code, hint in [(126, "Code 126 alone does not identify"),
                               (193, "architecture mismatch"), (127, "entry point")]:
                error = OSError("original loader detail")
                error.winerror = code
                with patch("load_diagnostics.c.CDLL", side_effect=error):
                    with self.assertRaises(SdkLoadError) as caught:
                        load_library(path)
                    self.assertEqual(caught.exception.system_code, code)
                    self.assertIs(caught.exception.__cause__, error)
                    self.assertIn(hint, str(caught.exception))
                    self.assertIn("original loader detail", str(caught.exception))

    def test_request_error_is_copied_before_snapshot_release(self):
        info = sized(a.am_error_info_t)
        info.code, info.message_utf8, info.source_utf8 = 7, b"backend missing", b"vision.backendUnavailable"
        info.retryable, info.fault_class = 1, 3
        error = copied_error(7, "request", info)
        info.message_utf8, info.source_utf8 = b"changed", b"changed"
        self.assertEqual(error.message, "backend missing")
        self.assertEqual(error.source, "vision.backendUnavailable")
        self.assertEqual((error.retryable, error.fault_class), (1, 3))


CSHARP_PROBE = r'''
using System; using AmrtSdk;
class LoadProbe {
    static int Main(string[] args) {
        Console.OutputEncoding = new System.Text.UTF8Encoding(false);
        if (args[0] == "injected") {
            Exception[] errors = { new BadImageFormatException("original architecture detail"), new EntryPointNotFoundException("original symbol detail") };
            foreach (Exception original in errors) {
                SdkLoadException wrapped = new SdkLoadException("am_vision.dll", original);
                if (!Object.ReferenceEquals(wrapped.InnerException, original) || !wrapped.Message.Contains(original.Message)) return 5;
                Console.WriteLine(wrapped.Message);
            }
            return 0;
        }
        try {
            using (VisionContext context = new VisionContext(args[0] == "shared" ? new string[0] : new string[] { args[1] }, true)) {
                if (args[0] == "shared") { Console.WriteLine("SHARED_WITHOUT_ORT_OK"); return 0; }
            }
            return 2;
        } catch (SdkLoadException error) {
            Console.WriteLine(error.Message);
            return args[0] == "missing" && error.InnerException is DllNotFoundException ? 0 : 3;
        } catch (SdkException error) {
            Console.WriteLine(error.Message);
            return args[0] == "runtime" && !String.IsNullOrEmpty(error.Detail) && !String.IsNullOrEmpty(error.SourceKey) ? 0 : 4;
        }
    }
}
'''


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path)
    parser.add_argument("--runtime-dir", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--csc", type=Path, default=Path("C:/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe"))
    args = parser.parse_args()
    output = args.output.resolve()
    if output == ROOT or ROOT in output.parents:
        parser.error("Use a dedicated output directory outside the checkout")
    output.mkdir(parents=True, exist_ok=True)
    result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(LoaderTests))
    if not result.wasSuccessful():
        raise RuntimeError("Python loader diagnostics failed")
    summary = {"pythonCases": result.testsRun, "csharp": [], "actualDll": "not requested"}
    invalid_runtime = output / "missing-runtime"
    if args.dll_dir:
        if args.runtime_dir:
            with VisionContext(args.dll_dir.resolve() / "am_vision.dll", [args.runtime_dir], True) as vision:
                try:
                    vision.load_model(output / "does-not-exist.onnx")
                except SdkError as error:
                    assert error.message and error.source, str(error)
                    summary["modelError"] = str(error)
                else:
                    raise AssertionError("Missing model was accepted")
        for kind, context in [("camera", CameraContext), ("vision", VisionContext)]:
            path = args.dll_dir.resolve() / ("am_" + kind + ".dll")
            with context(path, shared_frames=True) as instance:
                if kind == "camera":
                    try:
                        instance.open_source("not-a-url")
                    except SdkError as error:
                        assert error.message and error.source, str(error)
                        summary["cameraSourceError"] = str(error)
                    else:
                        raise AssertionError("Invalid camera URL was accepted")
            if kind == "camera":
                continue
            try:
                with context(path, [invalid_runtime], True):
                    raise AssertionError("Missing explicitly requested runtime was accepted")
            except SdkError as error:
                assert error.message and error.source, str(error)
                summary[kind + "InitError"] = str(error)
        summary["actualDll"] = "shared-only initialization and structured failures passed"
    if args.csc.is_file():
        probe, exe = output / "LoadProbe.cs", output / "LoadProbe.exe"
        probe.write_text(CSHARP_PROBE, encoding="utf-8")
        command = [str(args.csc), "/nologo", "/platform:x64", "/warnaserror", "/out:" + str(exe),
                   *map(str, sorted((SDK / "bindings/csharp").glob("*.cs"))), str(probe)]
        subprocess.run(command, check=True, capture_output=True)
        env = os.environ.copy()
        env["PATH"] = os.path.join(os.environ["SystemRoot"], "System32")
        modes = ["missing", "injected"]
        if args.dll_dir:
            modes += ["shared", "runtime"]
        for mode in modes:
            if mode not in ("missing", "injected"):
                env["PATH"] = str(args.dll_dir.resolve()) + os.pathsep + env["PATH"]
            process = subprocess.run([str(exe), mode, str(invalid_runtime)], env=env,
                                     capture_output=True, text=True, encoding="utf-8", errors="replace")
            assert process.returncode == 0, (mode, process.stdout, process.stderr)
            summary["csharp"].append({"mode": mode, "output": process.stdout.strip()})
    (output / "diagnostics-results.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=False))


if __name__ == "__main__":
    main()
