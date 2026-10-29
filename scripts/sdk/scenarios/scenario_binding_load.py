"""R9 场景：Python 绑定 require-style 全符号装载（amrt_sdk 登记函数逐个 getattr）。

am_camera/am_vision 各自按 FUNCTIONS 前缀遍历，逐符号 getattr DLL，
bind(require_all=True) 强制齐全；abi version 可调且等于绑定头版本=过。
判读：退出码 0=过。
用法：python scenario_binding_load.py --dll-dir <release> [--output result.json]
"""
import argparse
import ctypes as c
import json
import os
import platform
import sys
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "sdk" / "bindings" / "python"))
import amrt_sdk as a  # noqa: E402

PREFIXES = (("am_camera.dll", "amc_"), ("am_vision.dll", "amv_"))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    stage = "load"
    try:
        directory = Path(args.dll_dir).resolve()
        os.add_dll_directory(str(directory))
        result = {"ok": True, "schema": "framebraid.scenario-binding-load/1",
                  "os": platform.platform(), "libraries": []}
        for dll_name, prefix in PREFIXES:
            lib = c.CDLL(str(directory / dll_name))
            expected = [name for name in a.FUNCTIONS if name.startswith(prefix)]
            missing = [name for name in expected if not hasattr(lib, name)]
            a.bind(lib, prefix, require_all=True)
            abi = None
            getter = f"{prefix}get_abi_version"
            if not missing and hasattr(lib, getter):
                abi_call = getattr(lib, getter)
                abi_call.restype = c.c_uint32
                abi = abi_call()
                assert abi == a.AM_ABI_VERSION, f"{dll_name} ABI {abi} != 绑定头 {a.AM_ABI_VERSION}"
            result["libraries"].append({
                "dll": dll_name,
                "expected": len(expected),
                "resolved": len(expected) - len(missing),
                "missing": missing,
                "abi_version": abi,
            })
            print(f"[binding] {dll_name} 期望 {len(expected)} 符号，缺失 {len(missing)}，ABI {abi}", flush=True)
            assert not missing, f"{dll_name} 缺失符号: {missing}"
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(result, ensure_ascii=False, indent=1), encoding="utf-8")
        print(json.dumps(result, ensure_ascii=False))
    except Exception as error:
        print(json.dumps({"ok": False, "stage": stage, "error": repr(error)}, ensure_ascii=False))
        sys.exit(1)


if __name__ == "__main__":
    main()
