"""构建无外部媒体依赖的 Windows x64 SDK FFmpeg 运行库；不改系统环境或主项目 vendor。"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tarfile
import urllib.request
import zipfile

SOURCE_SHA256 = "5a1063443078be230709462c289f10b6695dfc88e7fb7009d5a1c58885562f2a"
SOURCE_COMMIT = "303510f9d1"
MAKE_EXTRALIBS = "-Wl,-Bstatic -lwinpthread -Wl,-Bdynamic"
TOOLS = {
    "nasm.zip": (
        "https://www.nasm.us/pub/nasm/releasebuilds/2.16.03/win64/nasm-2.16.03-win64.zip",
        "3ee4782247bcb874378d02f7eab4e294a84d3d15f3f6ee2de2f47a46aa7226e6",
    ),
    "make-4.4.1-3-x86_64.pkg.tar.zst": (
        "https://repo.msys2.org/msys/x86_64/make-4.4.1-3-x86_64.pkg.tar.zst",
        "af0bdba17f06fe037f0194069adaa31a8fe45f1a11381501896aea1fae37bd5d",
    ),
    "diffutils-3.12-1-x86_64.pkg.tar.zst": (
        "https://repo.msys2.org/msys/x86_64/diffutils-3.12-1-x86_64.pkg.tar.zst",
        "7902c8ce3d4dd69a0f5e98dc9d5c83c17b23314ba486169db57ef6e2835ce3b6",
    ),
}


def digest(path):
    return hashlib.file_digest(path.open("rb"), "sha256").hexdigest()


def msys_path(path):
    path = Path(path).resolve().as_posix()
    if len(path) < 3 or path[1:3] != ":/":
        raise ValueError("仅支持明确的本地 Windows 绝对路径")
    return "/" + path[0].lower() + path[2:]


# ⑤ x264/GPL 构建（2026-10-29 用户裁决）：编码推流需要 H.264 编码器。
GPL_BUILD = True
EXPECTED_LICENSE = "GPL version 2 or later" if GPL_BUILD else "LGPL version 2.1 or later"
EXPECTED_LICENSE_ID = "GPL-2.0-or-later" if GPL_BUILD else "LGPL-2.1-or-later"
BUILD_PROFILE = "ffmpeg-sdk-encode-win-x64" if GPL_BUILD else "ffmpeg-sdk-minimal-win-x64"


def configure_args(prefix):
    return [
        "--prefix=" + prefix, "--target-os=mingw32", "--arch=x86_64",
        "--enable-shared", "--disable-static", "--disable-autodetect",
        # ⑤ 编码推流（2026-10-29 用户裁决）：启用 libx264（GPL）。运行库整体
        # 变 GPL 组件——分发时必须随附 x264 GPL 许可文本（materials 体系登记）。
        "--enable-gpl", "--disable-nonfree", "--disable-version3",
        "--disable-programs", "--disable-doc", "--disable-avdevice",
        "--disable-avfilter", "--disable-debug", "--enable-w32threads",
        "--disable-pthreads", "--enable-schannel", "--extra-ldflags=-static-libgcc",
        "--disable-everything", "--enable-avcodec", "--enable-avformat",
        "--enable-avutil", "--enable-swscale", "--enable-swresample", "--enable-network",
        "--enable-libx264",
        "--enable-decoder=h264,hevc,mjpeg", "--enable-parser=h264,hevc,mjpeg",
        "--enable-encoder=libx264,aac",
        "--enable-muxer=mp4,mov,flv,rtmp",
        "--enable-demuxer=h264,hevc,mjpeg,rtsp,rtp,sdp,mov,matroska,mpegts",
        "--enable-protocol=file,tcp,udp,rtp,http,https,tls,rtmp",
        "--enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,extract_extradata,filter_units",
        "--enable-d3d11va", "--enable-dxva2",
        # d3d11va是旧AD_HOC接口；AVHWDeviceContext路径必须同时启用d3d11va2。
        "--enable-hwaccel=h264_d3d11va,h264_d3d11va2,hevc_d3d11va,hevc_d3d11va2,h264_dxva2,hevc_dxva2",
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-archive", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--msys-root", type=Path, default=Path("C:/msys64"))
    parser.add_argument("--jobs", type=int, default=6)
    parser.add_argument("--resume", action="store_true", help="仅复用相同配置的本次专属构建目录")
    args = parser.parse_args()
    if os.name != "nt" or not 1 <= args.jobs <= 32:
        raise ValueError("要求 Windows 主机，jobs 范围 1～32")
    archive = args.source_archive.resolve(strict=True)
    if digest(archive) != SOURCE_SHA256:
        raise ValueError("FFmpeg 源码归档 SHA-256 不匹配；禁止混配版本")
    root = args.output_root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    if root == archive.parent or root == args.msys_root.resolve():
        raise ValueError("输出必须使用专属构建目录")
    tools, source, build = root / "tools", root / "source", root / "build"
    for path in (tools, source, build, root / "tmp"):
        path.mkdir(exist_ok=True)
    bash = args.msys_root.resolve() / "usr/bin/bash.exe"
    for name, (url, expected) in TOOLS.items():
        target = tools / name
        if not target.exists():
            urllib.request.urlretrieve(url, target)
        if digest(target) != expected:
            raise ValueError("构建工具 SHA-256 不匹配：" + name)
    with zipfile.ZipFile(tools / "nasm.zip") as package:
        package.extractall(tools)
    for name in TOOLS:
        if name.endswith(".pkg.tar.zst"):
            subprocess.run([str(bash), "-lc", "cd " + shlex.quote(msys_path(tools))
                            + " && tar --zstd -xf " + shlex.quote(name)], check=True)
    source_dir = source / ("FFmpeg-" + SOURCE_COMMIT)
    # 续建逐文件核对原归档，防止将修改过的源码标记成原版，也避免覆盖正在使用的源码。
    with tarfile.open(archive) as package:
        if args.resume and source_dir.is_dir():
            for member in package:
                if member.isfile():
                    actual = (source / member.name).resolve()
                    if not actual.is_relative_to(source.resolve()):
                        raise ValueError("源码成员越界")
                    archived = package.extractfile(member)
                    if digest(actual) != hashlib.file_digest(archived, "sha256").hexdigest():
                        raise ValueError("续建源码已变化：" + member.name)
        else:
            package.extractall(source, filter="data")
    if not (source_dir / "configure").is_file():
        raise ValueError("源码根目录不符合冻结归档")
    prefix = msys_path(root / "install")
    options = configure_args(prefix)
    options_file = root / "configure.args.json"
    resume = args.resume and (build / "config.h").is_file() and options_file.is_file()
    if resume and json.loads(options_file.read_text(encoding="utf-8")) != options:
        raise ValueError("不允许复用不同 configure 参数的构建目录")
    (root / "configure.args.json").write_text(json.dumps(options, indent=2) + "\n", encoding="utf-8")
    tool_path = ":".join([msys_path(tools / "nasm-2.16.03"), msys_path(tools / "usr/bin"),
                           msys_path(args.msys_root / "mingw64/bin"),
                           msys_path(args.msys_root / "usr/bin")])
    make = msys_path(tools / "usr/bin/make.exe")
    script = "\n".join([
        "#!/usr/bin/env bash", "set -euo pipefail", "export PATH=" + shlex.quote(tool_path),
        "export PKG_CONFIG_PATH=/mingw64/lib/pkgconfig",
        "export TMPDIR=" + shlex.quote(msys_path(root / "tmp")),
        "cd " + shlex.quote(msys_path(build)),
        (":" if resume else shlex.quote(msys_path(source_dir / "configure")) + " "
         + " ".join(map(shlex.quote, options)) + " > ../configure.log 2>&1"),
        shlex.quote(make) + " -j" + str(args.jobs) + " "
        + shlex.quote("EXTRALIBS=" + MAKE_EXTRALIBS) + " > ../build.log 2>&1",
        shlex.quote(make) + " install > ../install.log 2>&1", "",
    ])
    (root / "run.sh").write_text(script, encoding="utf-8")
    print("构建日志：", root / "build.log", flush=True)
    subprocess.run([str(bash), str(root / "run.sh")], check=True)
    runtime = root / "runtime"
    runtime.mkdir(exist_ok=True)
    binaries = []
    versions = {}
    actual_configurations = {}
    with os.add_dll_directory(str(root / "install/bin")):
        for library, major in [("avutil", 61), ("swresample", 7), ("swscale", 10),
                               ("avcodec", 63), ("avformat", 63)]:
            name = f"{library}-{major}.dll"
            path = root / "install/bin" / name
            loaded = ctypes.CDLL(str(path))
            version = getattr(loaded, library + "_version")
            version.restype = ctypes.c_uint
            assert version() >> 16 == major
            versions[library] = version()
            configuration_fn = getattr(loaded, library + "_configuration")
            configuration_fn.restype = ctypes.c_char_p
            actual_configurations[library] = configuration_fn().decode()
            license_fn = getattr(loaded, library + "_license")
            license_fn.restype = ctypes.c_char_p
            # ⑤ x264 构建为 GPL（2026-10-29 用户裁决）；license 声明随 profile。
            assert license_fn().decode() == EXPECTED_LICENSE
            imports = subprocess.check_output([
                str(args.msys_root / "mingw64/bin/objdump.exe"), "-p", str(path)
            ], text=True, encoding="utf-8")
            imports = [line.split("DLL Name:", 1)[1].strip()
                       for line in imports.splitlines() if "DLL Name:" in line]
            if any("pthread" in value.lower() or "libgcc" in value.lower() for value in imports):
                raise ValueError("存在意外 GCC 动态依赖：" + name)
            if library == "avcodec":
                hardware_configs = validate_hardware_configs(loaded)
            shutil.copy2(path, runtime / name)
            binaries.append({"group": "ffmpeg", "file": name, "sha256": digest(path),
                             "bytes": path.stat().st_size, "directPeImports": imports})
    license_names = ["COPYING.LGPLv2.1", "LICENSE.md"]
    if GPL_BUILD:
        license_names.append("COPYING.GPLv2")
    for name in license_names:
        shutil.copy2(source_dir / name, root / name)
    license_files = []
    for package in ("gcc-libs", "crt", "headers", "libwinpthread", "winpthreads"):
        directory = args.msys_root / "mingw64/share/licenses" / package
        if not directory.is_dir():
            raise ValueError("工具链许可证目录缺失：" + str(directory))
        for path in directory.rglob("*"):
            if path.is_file():
                target = root / "toolchain-licenses" / package / path.relative_to(directory)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, target)
                license_files.append({"path": target.relative_to(root).as_posix(), "sha256": digest(target)})
    shutil.copy2(build / "config.h", root / "config.h")
    config = (build / "config.h").read_text(encoding="utf-8")
    for token in ("#define HAVE_X86ASM 1", "#define HAVE_W32THREADS 1",
                  f"#define CONFIG_GPL {1 if GPL_BUILD else 0}",
                  "#define CONFIG_NONFREE 0", "#define CONFIG_VERSION3 0"):
        if token not in config:
            raise ValueError("构建策略未生效：" + token)
    gcc = args.msys_root / "mingw64/bin/gcc.exe"
    record = {"schema": 1, "profile": BUILD_PROFILE, "sourceCommit": SOURCE_COMMIT,
              "sourceSha256": SOURCE_SHA256, "sourceArchive": str(archive), "configure": options,
              "sourceModified": False, "license": EXPECTED_LICENSE_ID,
              "externalMediaLibraries": [], "threadBackend": "w32threads", "x86Assembly": True,
              "makeVariables": {"EXTRALIBS": MAKE_EXTRALIBS},
              "compiler": subprocess.check_output([str(gcc), "--version"], text=True).splitlines()[0],
              "compilerPackages": [path.name for path in (args.msys_root / "var/lib/pacman/local").glob("mingw-w64-x86_64-gcc-*")],
              "mingwRuntimePackages": [path.name for path in (args.msys_root / "var/lib/pacman/local").glob("mingw-w64-x86_64-crt-*")],
              "compilerSource": "https://packages.msys2.org/packages/mingw-w64-x86_64-gcc",
              "toolchainLicenseFiles": license_files,
              "tools": [{"file": name, "url": item[0], "sha256": item[1]}
                        for name, item in TOOLS.items()], "versions": versions,
              "actualConfigurations": actual_configurations, "binaries": binaries,
              "decoderHardwareConfigs": hardware_configs,
              "scope": "H264/HEVC/MJPEG SDK transport/decode, native Windows hardware backends; no programs"}
    (root / "build-record.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"runtime": str(runtime), "record": str(root / "build-record.json")}), flush=True)


def validate_hardware_configs(library):
    """设备类型可枚举不代表解码器支持；核验每种编码的HW_DEVICE_CTX合同。"""
    class Config(ctypes.Structure):
        _fields_ = [("pixelFormat", ctypes.c_int), ("methods", ctypes.c_int),
                    ("deviceType", ctypes.c_int)]
    find = library.avcodec_find_decoder_by_name
    find.argtypes, find.restype = [ctypes.c_char_p], ctypes.c_void_p
    query = library.avcodec_get_hw_config
    query.argtypes, query.restype = [ctypes.c_void_p, ctypes.c_int], ctypes.POINTER(Config)
    results = {}
    for name in ("h264", "hevc"):
        codec = find(name.encode("ascii"))
        if not codec:
            raise ValueError("required decoder missing: " + name)
        configs = []
        for index in range(64):
            item = query(codec, index)
            if not item:
                break
            item = item.contents
            configs.append({"pixelFormat": item.pixelFormat, "methods": item.methods,
                            "deviceType": item.deviceType})
        for device in (4, 7):
            if not any(item["deviceType"] == device and item["methods"] & 1 for item in configs):
                raise ValueError(f"{name}: required HW_DEVICE_CTX backend {device} missing")
        results[name] = configs
    return results


if __name__ == "__main__":
    main()
