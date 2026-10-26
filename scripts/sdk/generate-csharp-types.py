"""从生成的 ctypes ABI 选取 C# 主流程所需布局，不另维护 ABI 真相源。"""
import argparse
import ctypes as c
from pathlib import Path
import sys
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdk" if (ROOT / "sdk").is_dir() else ROOT
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a

SELECTED = ["amc_init_params_t", "amv_init_params_t", "am_model_desc_t",
            "am_backend_policy_t", "am_image_input_t", "am_infer_params_t",
            "am_raw_preprocess_t", "am_tensor_out_t", "amv_result_t",
            "am_request_snapshot_t", "am_error_info_t", "am_frame_view_t",
            "am_frame_meta_t", "am_pixel_view_t", "am_frame_assoc_t",
            "am_source_desc_t", "am_credential_data_t", "am_capture_request_t",
            "am_captured_image_t", "am_frame_sub_desc_t", "am_shared_pool_config_t",
            "am_shared_pool_attach_desc_t", "am_shared_frame_view_t"]


def generate():
    needed = {}

    def visit(kind):
        if kind.__name__ in needed:
            return
        needed[kind.__name__] = kind
        for _, field in kind._fields_:
            if issubclass(field, c.Structure):
                visit(field)
    for name in SELECTED:
        visit(getattr(a, name))
    primitives = {c.c_uint32: "uint", c.c_int32: "int", c.c_uint64: "ulong",
                  c.c_int64: "long", c.c_float: "float", c.c_double: "double",
                  c.c_ubyte: "byte", c.c_char: "byte"}

    def typename(kind):
        return primitives.get(kind, kind.__name__ if issubclass(kind, c.Structure) else "IntPtr")

    lines = ["// 从公开 ctypes ABI 选取生成；运行 scripts/sdk/generate-csharp-types.py 更新。",
             "using System;", "using System.Runtime.InteropServices;", "namespace AmrtSdk {"]
    for name, kind in needed.items():
        lines += ["    [StructLayout(LayoutKind.Sequential)]", "    public struct " + name + " {"]
        for field, value in kind._fields_:
            if issubclass(value, c.Array):
                lines.append("        [MarshalAs(UnmanagedType.ByValArray, SizeConst = %d)]" % value._length_)
                target = typename(value._type_) + "[]"
            else:
                target = typename(value)
            lines.append("        public %s @%s;" % (target, field))
        lines += ["    }", ""]
    lines += ["}", ""]
    return "\n".join(lines)


def hardware_functions():
    names = ["amc_get_abi_version", "amc_probe_decode_hardware", "amc_get_decode_hardware_report", "amc_source_set_decode_policy", "amv_get_abi_version", "amv_probe_inference_hardware", "amv_get_inference_hardware_report"]
    primitive = {c.c_uint32: "uint", c.c_uint64: "ulong", c.c_int32: "int"}
    lines = ["// Generated from the public ctypes ABI; do not edit.",
             "using System;", "using System.Runtime.InteropServices;",
             "namespace AmrtSdk {", "    internal static partial class Native {"]
    for name in names:
        result, args = a.FUNCTIONS[name]
        parameters = ", ".join(primitive.get(kind, "IntPtr") + " arg" + str(i) for i, kind in enumerate(args))
        dll = 'am_camera.dll' if name.startswith('amc_') else 'am_vision.dll'
        lines.append('        [DllImport("'+dll+'", CallingConvention=CallingConvention.Cdecl)]')
        lines.append("        internal static extern " + primitive[result] + " " + name + "(" + parameters + ");")
    return "\n".join(lines + ["    }", "}", ""])


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    target = SDK / "bindings/csharp/NativeTypes.cs"
    expected = generate()
    if args.check:
        if not target.is_file() or target.read_text(encoding="utf-8") != expected:
            raise SystemExit("C# ABI layouts differ; regenerate selected types")
    else:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(expected, encoding="utf-8", newline="\n")
    extension = SDK / "bindings/csharp/NativeHardware.cs"
    if args.check:
        if not extension.is_file() or extension.read_text(encoding="utf-8") != hardware_functions():
            raise SystemExit("C# hardware declarations differ; regenerate")
    else:
        extension.write_text(hardware_functions(), encoding="utf-8", newline="\n")
