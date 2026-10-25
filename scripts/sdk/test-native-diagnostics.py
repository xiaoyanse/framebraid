"""真实DLL缺依赖诊断；仅使用独立临时目录和回环无服务地址。"""
import argparse
import ctypes as c
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from native_diagnostic_fixture import build_fixture
ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdk" if (ROOT / "sdk/bindings").is_dir() else ROOT
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a

DLLS = ("avutil-61.dll", "swresample-7.dll", "swscale-10.dll", "avcodec-63.dll", "avformat-63.dll")


def sized(kind):
    value = kind()
    value.struct_size = c.sizeof(value)
    return value


def check(code):
    assert code == 0, code


def error(lib, owner):
    info = sized(a.am_error_info_t)
    check(lib.amc_get_last_error(owner, c.byref(info)))
    return {"code": info.code, "message": info.message_utf8.decode(),
            "source": info.source_utf8.decode(), "faultClass": info.fault_class}


def close(lib, owner, method):
    state = c.c_uint32()
    until = time.monotonic() + 8
    while True:
        check(method(owner, 250, c.byref(state)))
        if state.value == a.AM_LIFECYCLE_CLOSED:
            break
        assert time.monotonic() < until, state.value
    check(lib.amc_object_destroy(owner))


def child(args):
    with tempfile.TemporaryDirectory(prefix="am-sdk-native-diagnostics-") as temporary:
        runtime = Path(temporary)
        if args.case == "partial":
            (runtime / DLLS[0]).write_bytes(b"not loaded because another component is missing")
        elif args.case == "corrupt":
            for name in DLLS:
                (runtime / name).write_bytes(b"invalid PE image")
        elif args.case == "symbols":
            fixture = Path(args.symbol_fixture) if args.symbol_fixture else build_fixture(runtime / "safe-fixture.dll")
            for name in DLLS:
                shutil.copyfile(fixture, runtime / name)
        elif args.case == "valid":
            runtime = Path(args.runtime_dir).resolve()
        directory = os.add_dll_directory(str(Path(args.dll_dir).resolve()))
        lib = c.CDLL(str(Path(args.dll_dir).resolve() / "am_camera.dll"))
        a.bind(lib, "amc_")
        params = sized(a.amc_init_params_t)
        params.abi_version = a.AM_ABI_VERSION
        dirs = (c.c_char_p * 1)(str(runtime).encode())
        params.runtime_dirs_utf8, params.runtime_dir_count = dirs, 1
        ctx = c.POINTER(a.amc_context_t)()
        check(lib.amc_init(c.byref(params), c.byref(ctx)))
        cfg = sized(a.am_decoder_config_t)
        cfg.codec, cfg.output_pixel_format = a.AM_CODEC_H264, a.AM_PF_RGBA8
        cfg.hw_policy = a.AM_DHW_SOFTWARE_ONLY
        decoder = c.POINTER(a.amc_decoder_t)()
        result = lib.amc_decoder_create(ctx, c.byref(cfg), c.byref(decoder))
        if args.case == "valid":
            check(result)
            close(lib, decoder, lib.amc_decoder_close)
            details = {"runtimeAvailable": True}
        else:
            assert result == a.AM_ERR_SDK_NOT_SUPPORTED and not decoder, result
            details = error(lib, ctx)
            assert details["code"] == result and details["source"].startswith("camera.ffmpeg"), details
            expected = {"missing": "avutil-61.dll", "partial": "swresample-7.dll",
                        "corrupt": "Windows error 193", "symbols": "exported symbol missing"}[args.case]
            assert expected in details["message"], details
            # 使用相同上下文验证异步RTSP错误同样能从源句柄取得，无须相机可达。
            credentials = sized(a.am_credential_data_t)
            credentials.username_utf8, credentials.password_utf8 = b"diagnostic-user", b"diagnostic-secret"
            desc = sized(a.am_source_desc_t)
            desc.kind, desc.url_utf8, desc.stream_kind_utf8 = a.AM_SRC_RTSP_URL, b"rtsp://127.0.0.1:1/secret-path", b"main"
            desc.creds = c.pointer(credentials)
            source = c.POINTER(a.amc_source_t)()
            check(lib.amc_source_open(ctx, c.byref(desc), c.byref(source)))
            until = time.monotonic() + 4
            while True:
                source_error = error(lib, source)
                if source_error["code"] == result:
                    break
                assert time.monotonic() < until, source_error
                time.sleep(.02)
            assert expected in source_error["message"], source_error
            assert not any(secret in json.dumps(source_error) for secret in
                           ("diagnostic-user", "diagnostic-secret", "secret-path", str(runtime))), source_error
            details["sourceError"] = source_error
            close(lib, source, lib.amc_source_close)
        close(lib, ctx, lib.amc_shutdown)
        directory.close()
        return {"case": args.case, **details}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", required=True)
    parser.add_argument("--runtime-dir", required=True)
    parser.add_argument("--case", choices=("missing", "partial", "corrupt", "symbols", "valid"))
    parser.add_argument("--symbol-fixture", help="可选：其他电脑预构建的无CRT/无入口点缺符号DLL")
    args = parser.parse_args()
    if args.case:
        print(json.dumps(child(args)))
        return
    results = []
    for case in ("missing", "partial", "corrupt", "symbols", "valid"):
        command = [sys.executable, __file__, "--dll-dir", args.dll_dir,
                   "--runtime-dir", args.runtime_dir, "--case", case]
        if args.symbol_fixture:
            command += ["--symbol-fixture", args.symbol_fixture]
        completed = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", timeout=25)
        assert completed.returncode == 0, (case, completed.returncode, completed.stdout, completed.stderr)
        results.append(json.loads(completed.stdout))
    print(json.dumps({"passed": True, "cases": results}))


if __name__ == "__main__":
    main()
