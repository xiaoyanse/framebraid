"""按锁定的 Windows 依赖图收集原始许可，不复制注册表路径或修改上游正文。"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "sdk" / "third-party" / "rust"


def collect():
    command = ["cargo", "metadata", "--format-version", "1", "--locked", "--offline",
               "--filter-platform", "x86_64-pc-windows-msvc", "--features",
               "am-camera-ffi/shared-frames,am-vision-ffi/shared-frames"]
    metadata = json.loads(subprocess.check_output(command, cwd=ROOT))
    records = []
    sections = [b"FrameBraid: resolved Windows Cargo dependency license texts\n"
                b"Build dependencies may be included. This is not a linker-level attribution claim.\n"]
    for package in sorted(metadata["packages"], key=lambda item: (item["name"], item["version"])):
        if not package["source"]:
            continue
        base = Path(package["manifest_path"]).parent.resolve()
        if not package["license"]:
            raise ValueError("Undeclared license: " + package["name"])
        files = {p for p in base.rglob("*") if p.is_file() and
                 p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE",
                                             "AUTHORS", "UNLICENSE", "THIRD_PARTY", "THIRDPARTY"))}
        declared = package.get("license_file")
        if declared:
            files.add((base / declared).resolve())
        if not files:
            raise ValueError("No original license text: " + package["name"])
        texts = []
        for filename in sorted(files):
            resolved = filename.resolve()
            relative = resolved.relative_to(base).as_posix()
            if filename.is_symlink():
                raise ValueError("Refuse linked license: " + relative)
            content = filename.read_bytes()
            content.decode("utf-8")
            texts.append({"path": relative, "bytes": len(content),
                          "sha256": hashlib.sha256(content).hexdigest()})
            title = ("\n=== " + package["name"] + " " + package["version"] +
                     " / " + relative + " ===\n").encode("utf-8")
            sections.append(title + content + b"\n")
        records.append({"name": package["name"], "version": package["version"],
                        "declaredLicense": package["license"],
                        "registrySource": package["source"], "originalTexts": texts})
    chunks, pending = [], bytearray()
    for section in sections:
        if pending and len(pending) + len(section) > 450_000:
            chunks.append(bytes(pending))
            pending.clear()
        if len(section) >= 1_000_000:
            raise ValueError("Single upstream notice exceeds source-file size budget")
        pending.extend(section)
    if pending:
        chunks.append(bytes(pending))
    parts = [("NOTICES-" + str(i + 1) + ".txt", chunk) for i, chunk in enumerate(chunks)]
    manifest = {"schemaVersion": 1, "target": "x86_64-pc-windows-msvc",
                "scope": "Resolved Cargo metadata including build dependencies; not exact linked-code attribution",
                "cargoLockSha256": hashlib.sha256((ROOT / "Cargo.lock").read_bytes()).hexdigest(),
                "noticeParts": [{"path": name, "bytes": len(data),
                                 "sha256": hashlib.sha256(data).hexdigest()} for name, data in parts],
                "packages": records}
    return parts, (json.dumps(manifest, indent=2) + "\n").encode("utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    parts, inventory = collect()
    names = parts + [("inventory.json", inventory)]
    if args.check:
        for name, data in names:
            if (OUT / name).read_bytes() != data:
                raise ValueError("License collection drift: " + name)
    else:
        OUT.mkdir(parents=True, exist_ok=True)
        for name, data in names:
            (OUT / name).write_bytes(data)
    print(json.dumps({"packages": len(json.loads(inventory)["packages"]),
                      "noticeBytes": sum(len(data) for _, data in parts), "checked": args.check}))
