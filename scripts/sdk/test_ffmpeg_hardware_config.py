"""核对运行库解码入口，防止仅启用旧AD_HOC硬解而误报支持。"""
import ctypes
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("ffmpeg_builder", Path(__file__).with_name("build-ffmpeg-runtime.py"))
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class Config(ctypes.Structure):
    _fields_ = [("pixelFormat", ctypes.c_int), ("methods", ctypes.c_int),
                ("deviceType", ctypes.c_int)]


class Function:
    def __init__(self, callback):
        self.callback = callback

    def __call__(self, *args):
        return self.callback(*args)


class Library:
    def __init__(self, entries):
        self.entries = {key: [Config(*row) for row in rows] for key, rows in entries.items()}
        self.avcodec_find_decoder_by_name = Function(lambda name: 1 if name == b"h264" else 2)
        self.avcodec_get_hw_config = Function(self.query)

    def query(self, codec, index):
        rows = self.entries["h264" if codec == 1 else "hevc"]
        return ctypes.pointer(rows[index]) if index < len(rows) else None


class HardwareConfigurationTests(unittest.TestCase):
    def test_generic_device_context_is_required_for_each_codec(self):
        library = Library({name: [(51, 11, 4), (172, 3, 7)] for name in ("h264", "hevc")})
        self.assertEqual(set(builder.validate_hardware_configs(library)), {"h264", "hevc"})

    def test_legacy_d3d11_ad_hoc_does_not_satisfy_contract(self):
        library = Library({name: [(51, 11, 4), (116, 8, 0)] for name in ("h264", "hevc")})
        with self.assertRaisesRegex(ValueError, "backend 7 missing"):
            builder.validate_hardware_configs(library)

    def test_hevc_is_checked_independently(self):
        library = Library({"h264": [(51, 11, 4), (172, 3, 7)], "hevc": [(51, 11, 4)]})
        with self.assertRaisesRegex(ValueError, "hevc.*backend 7 missing"):
            builder.validate_hardware_configs(library)

    def test_configure_enables_modern_d3d11_for_both_codecs(self):
        option = next(value for value in builder.configure_args("/local/install") if value.startswith("--enable-hwaccel="))
        self.assertIn("h264_d3d11va2", option)
        self.assertIn("hevc_d3d11va2", option)


if __name__ == "__main__":
    unittest.main()
