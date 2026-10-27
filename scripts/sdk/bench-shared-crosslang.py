"""R2 1.4 跨语言搬运量化：编译并运行 C# 共享帧发布/借用/复制/归还基准，保存三轮原始数据。"""
import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
CSC = Path("C:/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, default=ROOT / ".local/build/current/release")
    parser.add_argument("--output", type=Path, default=Path(".local/evidence/r2-shared-crosslang-20261027"))
    args = parser.parse_args()
    output = ROOT / args.output
    output.mkdir(parents=True, exist_ok=True)
    sources = sorted((ROOT / "sdk/bindings/csharp").glob("*.cs")) + [
        Path(__file__).with_name("shared-crosslang-bench.cs")]
    executable = output / "SharedBench.exe"
    command = [str(CSC), "/nologo", "/warnaserror", "/platform:x64",
               "/out:" + str(executable), *map(str, sources)]
    compiled = subprocess.run(command, capture_output=True, text=True)
    assert compiled.returncode == 0, compiled.stdout + compiled.stderr
    env = os.environ.copy()
    env["PATH"] = str(args.dll_dir.resolve()) + os.pathsep + env.get("PATH", "")
    ran = subprocess.run([str(executable)], env=env, capture_output=True, text=True)
    assert ran.returncode == 0, ran.stdout + ran.stderr
    rounds = [json.loads(line) for line in ran.stdout.splitlines() if line.strip()]
    (output / "measure.json").write_text(json.dumps(rounds, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(rounds, indent=2))


if __name__ == "__main__":
    main()
