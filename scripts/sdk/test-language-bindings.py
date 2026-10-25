"""语言封装静态门；--run 显式启用本地真实 DLL 示例，不启动业务服务。"""
import argparse
import ast
import ctypes as c
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdk" if (ROOT / "sdk").is_dir() else ROOT
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a


def invoke(command, env=None):
    process = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", errors="replace", env=env)
    if process.returncode:
        label = command if isinstance(command, str) else command[0]
        raise RuntimeError("Command failed (%d): %s\n%s" % (process.returncode, label, process.stdout+process.stderr))
    return process.stdout


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--csc", type=Path, default=Path("C:/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe"))
    parser.add_argument("--run", action="store_true")
    parser.add_argument("--dll-dir", type=Path)
    parser.add_argument("--runtime-dir", type=Path)
    parser.add_argument("--model", type=Path)
    parser.add_argument("--vcvars64", type=Path)
    parser.add_argument("--import-lib-dir", type=Path, help="导入库单独存放时显式指定；默认与DLL同目录")
    args = parser.parse_args()
    if args.run and not all((args.dll_dir, args.runtime_dir, args.model)):
        parser.error("--run requires --dll-dir, --runtime-dir, --model")
    if args.vcvars64 and not args.dll_dir:
        parser.error("--vcvars64 requires --dll-dir for native import libraries")
    output = args.output.resolve()
    if output == ROOT or ROOT in output.parents:
        parser.error("Use a dedicated test output directory outside the source checkout")
    output.mkdir(parents=True, exist_ok=True)
    python_files = [SDK / "bindings/python/safe_sdk.py", SDK / "bindings/python/load_diagnostics.py",
                    SDK / "examples/inference.py", SDK / "examples/camera.py"]
    for file in python_files:
        ast.parse(file.read_text(encoding="utf-8"), filename=str(file))
    generator = Path(__file__).with_name("generate-csharp-types.py")
    invoke([sys.executable, str(generator), "--check"])
    invoke([sys.executable, str(Path(__file__).with_name("test-safe-bindings.py"))])
    sources = sorted((SDK / "bindings/csharp").glob("*.cs"))
    names = []
    for line in (SDK / "bindings/csharp/NativeTypes.cs").read_text(encoding="utf-8").splitlines():
        if "public struct " in line:
            names.append(line.split("public struct ")[1].split()[0])
    # 结构大小和每字段偏移跨语言核对，防止类型能编译但布局错误。
    probe = output / "Layouts.cs"
    probe.write_text('using System; using System.Runtime.InteropServices; using AmrtSdk;\n'
                     'class Layouts { static void Main() {\n' + "\n".join(
                         'Console.WriteLine("%s|"+Marshal.SizeOf(typeof(%s)));\n%s' %
                         (name, name, "\n".join('Console.WriteLine("%s.%s|"+Marshal.OffsetOf(typeof(%s), "%s"));' %
                                                  (name, field, name, field) for field, _ in getattr(a, name)._fields_))
                         for name in names) + '\n} }\n', encoding="utf-8")
    csc = str(args.csc.resolve())
    layout_exe, inference_exe = output / "Layouts.exe", output / "Inference.exe"
    options = [csc, "/nologo", "/platform:x64", "/warnaserror"]
    invoke(options+["/out:"+str(layout_exe), *map(str, sources), str(probe)])
    rows = dict(line.strip().split("|", 1) for line in invoke([str(layout_exe)]).splitlines())
    fields = 0
    for name in names:
        native = getattr(a, name)
        assert int(rows[name]) == c.sizeof(native), (name, rows[name], c.sizeof(native))
        for field, _ in native._fields_:
            assert int(rows[name+"."+field]) == getattr(native, field).offset, (name, field)
            fields += 1
    invoke(options+["/out:"+str(inference_exe), *map(str, sources), str(SDK / "examples/Inference.cs")])
    summary = {"pythonParsed": len(python_files), "csharpStructures": len(names), "csharpFields": fields,
               "pythonOwnershipTests": 6, "csharpCompile": "passed", "runtime": "not requested"}
    native_executables = []
    native_env = None
    if args.vcvars64:
        vcvars = str(args.vcvars64.resolve())
        if not args.vcvars64.is_file() or any(char in vcvars for char in '%"\r\n'):
            parser.error("--vcvars64 must name a valid local batch file")
        native_env = os.environ.copy()
        for row in invoke('cmd.exe /d /c call "'+vcvars+'" >nul && set').splitlines():
            if "=" in row and not row.startswith("="):
                key, value = row.split("=", 1)
                native_env[key] = value
        compiler = shutil.which("cl.exe", path=native_env.get("Path", native_env.get("PATH", "")))
        if not compiler:
            raise RuntimeError("MSVC environment did not expose cl.exe")
        for suffix, extension, flags, libraries in [
                ("c", "c", [], ["am_vision.dll.lib"]),
                ("cpp", "cpp", ["/std:c++17", "/EHsc"], ["am_camera.dll.lib", "am_vision.dll.lib"])]:
            executable = output / ("inference_"+suffix+".exe")
            invoke([compiler, "/nologo", "/utf-8", "/W4", "/WX", *flags, "/I"+str(SDK/"include"),
                    str(SDK/("examples/inference."+extension)), "/Fe:"+str(executable),
                    "/Fo:"+str(output/("inference_"+suffix+".obj")), "/link",
                    "/LIBPATH:"+str((args.import_lib_dir or args.dll_dir).resolve()), *libraries], native_env)
            native_executables.append((suffix, executable))
        summary["nativeCompile"] = "C and C++17 /W4 /WX passed"
    if args.run:
        env = os.environ.copy()
        env["PATH"] = str(args.dll_dir.resolve()) + os.pathsep + env.get("PATH", "")
        summary["python"] = invoke([sys.executable, str(SDK / "examples/inference.py"), "--dll-dir", str(args.dll_dir.resolve()),
                                    "--runtime-dir", str(args.runtime_dir.resolve()), "--model", str(args.model.resolve())], env).strip()
        summary["csharp"] = invoke([str(inference_exe), str(args.runtime_dir.resolve()), str(args.model.resolve())], env).strip()
        for suffix, executable in native_executables:
            summary[suffix] = invoke([str(executable), str(args.runtime_dir.resolve()), str(args.model.resolve())], env).strip()
        summary["runtime"] = "CPU Identity and shared frames passed"
    (output / "language-results.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2)+"\n", encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=False))


if __name__ == "__main__":
    main()
