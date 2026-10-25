"""用MSVC构建无CRT和入口点的缺符号测试DLL；远机可复用此预构建样本。"""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def compiler_path(explicit=None):
    if explicit:
        return Path(explicit).resolve()
    found = shutil.which("cl.exe")
    if found:
        return Path(found)
    vswhere = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)"),
                   "Microsoft Visual Studio/Installer/vswhere.exe")
    if vswhere.is_file():
        installation = subprocess.check_output([str(vswhere), "-latest", "-products", "*",
                                                "-property", "installationPath"], text=True).strip()
        root = Path(installation, "VC/Tools/MSVC")
        for version in sorted(root.glob("*"), reverse=True):
            compiler = version / "bin/Hostx64/x64/cl.exe"
            if compiler.is_file():
                return compiler
    raise RuntimeError("MSVC compiler unavailable; build the safe fixture elsewhere and pass --symbol-fixture. Do not rename a system DLL as a fixture.")


def build_fixture(output, compiler=None):
    output = Path(output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    cl = compiler_path(compiler)
    source = Path(__file__).with_name("fixtures") / "native-diagnostic.c"
    with tempfile.TemporaryDirectory(prefix="am-sdk-fixture-build-") as temporary:
        obj = Path(temporary, "native-diagnostic.obj")
        commands = [
            [str(cl), "/nologo", "/utf-8", "/c", "/GS-", "/Zl", "/O1", "/Fo" + str(obj), str(source)],
            [str(cl.with_name("link.exe")), "/nologo", "/DLL", "/NOENTRY", "/NODEFAULTLIB",
             "/MACHINE:X64", "/DYNAMICBASE", "/NXCOMPAT", "/Brepro", "/OUT:" + str(output), str(obj)],
        ]
        for command in commands:
            result = subprocess.run(command, capture_output=True, timeout=30)
            if result.returncode:
                raise RuntimeError((result.returncode, result.stdout.decode("utf-8", "replace"),
                                    result.stderr.decode("utf-8", "replace")))
    return output


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True)
    parser.add_argument("--compiler")
    args = parser.parse_args()
    print(build_fixture(args.output, args.compiler))
