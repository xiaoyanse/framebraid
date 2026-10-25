"""生成同源源码包和 Windows 包，显式记录分发授权；不创建远程或上传。"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import zipfile


ROOT = Path(__file__).resolve().parents[2]


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True, encoding="utf-8").strip()


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def safe_directory(value, fresh=False):
    original = Path(value)
    if not original.is_absolute():
        raise ValueError("Absolute directory required")
    for part in [original, *original.parents]:
        if part.is_symlink() or (part.exists() and part.is_junction()):
            raise ValueError("Refuse linked directory")
    resolved = original.resolve()
    if resolved == ROOT or ROOT in resolved.parents or resolved in ROOT.parents:
        raise ValueError("Output must not contain or be inside the source checkout")
    if fresh and resolved.exists():
        raise ValueError("Refuse existing output; choose a fresh directory")
    return resolved


def inventory(directory):
    rows = []
    for filename in sorted(directory.rglob("*")):
        if filename.is_symlink() or (filename.exists() and filename.is_junction()):
            raise ValueError("Refuse linked artifact")
        if filename.is_file():
            rows.append({"path": filename.relative_to(directory).as_posix(),
                         "bytes": filename.stat().st_size, "sha256": digest(filename)})
    return rows


def extract_source(archive, target):
    with zipfile.ZipFile(archive) as bundle:
        for info in bundle.infolist():
            path = target / info.filename
            if "\\" in info.filename or path.resolve() == target or target not in path.resolve().parents:
                raise ValueError("Archive path escapes output")
            if (info.external_attr >> 16) & 0o170000 == 0o120000:
                raise ValueError("Refuse archived link")
        bundle.extractall(target)


def copy_tree(source, target):
    inventory(source)
    shutil.copytree(source, target, dirs_exist_ok=True)


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def release_labels(version, publication_authorized):
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[a-zA-Z0-9.-]+)?", version):
        raise ValueError("Version must be a safe semantic version")
    if publication_authorized and version.endswith("-local"):
        raise ValueError("A local version cannot be approved for publication")
    return {"version": version, "approved": publication_authorized,
            "source": f"FrameBraid-SDK-source-{version}.zip",
            "windows": f"FrameBraid-SDK-windows-x64-{version}.zip",
            "purpose": "authorized-public-release" if publication_authorized else "local-release-candidate-not-published"}


def verify_document_links(directory):
    count = 0
    for document in directory.rglob("*.md"):
        text = re.sub(r"```.*?```", "", document.read_text(encoding="utf-8"), flags=re.S)
        for href in re.findall(r"\[[^\]]*\]\(([^)]+)\)", text):
            target = href.split("#")[0].strip("<>")
            if not target or re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target):
                continue
            if not (document.parent / target).exists():
                raise ValueError("Packaged link missing: " + str(document.relative_to(directory)) + " -> " + target)
            count += 1
    return count


def prepare(args):
    labels = release_labels(args.version, args.publication_authorized)
    output = safe_directory(args.output, fresh=True)
    evidence = safe_directory(args.evidence, fresh=True)
    if output in evidence.parents or evidence in output.parents:
        raise ValueError("Evidence and deliverables must be separate")
    if run("git", "status", "--porcelain"):
        raise ValueError("Commit source/documentation before packaging")
    commit = run("git", "rev-parse", "HEAD")
    record = json.loads(Path(args.build_record).read_text(encoding="utf-8"))
    if record["sourceCommit"] != commit:
        raise ValueError("Build and source snapshot must identify the same commit")
    run("python", "-X", "utf8", "scripts/sdk/collect-rust-licenses.py", "--check")
    run("node", "scripts/sdk/check-boundary.mjs")
    evidence.mkdir(parents=True)
    raw = evidence / "runtime-validation-stage"
    checked = run("node", "scripts/sdk/package-delivery.mjs", "--output", str(raw),
                  "--dll-dir", args.dll_dir, "--ffmpeg-dir", args.ffmpeg_dir,
                  "--ort-dir", args.ort_dir, "--materials-dir", args.materials_dir,
                  "--materials-profile", "minimal-20261026", "--build-record", args.build_record,
                  "--require-distribution-ready")
    (evidence / "runtime-validator.json").write_text(checked + "\n", encoding="utf-8")
    original = json.loads((raw / "runtime-manifest.json").read_text(encoding="utf-8"))
    if original["licensing"]["gaps"]:
        raise ValueError("Native material gaps remain")
    output.mkdir(parents=True)
    source_name = labels["source"]
    windows_name = labels["windows"]
    run("git", "archive", "--format=zip", "--output=" + str(output / source_name), commit)
    windows = output / "windows-x64"
    extract_source(output / source_name, windows)
    for name in ["bin", "lib", "licenses"]:
        copy_tree(raw / name, windows / name)
    for name in ["ffmpeg", "onnxruntime-cpu"]:
        copy_tree(raw / "runtime" / name, windows / "runtime" / name)
    copy_tree(ROOT / "sdk/third-party/rust", windows / "licenses/rust")
    shutil.copy2(ROOT / "sdk/third-party/profiles/minimal-20261026.json",
                 windows / "licenses/runtime-profile.json")
    (windows / "licenses/README.md").write_text(
        "# FrameBraid dependency materials / 依赖材料\n\n"
        "Project: [Apache-2.0](../LICENSE), [NOTICE](../NOTICE).\n\n"
        "Runtime fingerprints: [profile](runtime-profile.json). "
        "FFmpeg corresponding source and build recipes: ffmpeg/sources/. "
        "ORT declarations: onnxruntime/. Cargo originals: rust/.\n\n"
        "匹配的原始声明和对应源码在以上目录，保留各组件版权与许可。"
        "材料在场不表示负责人已批准发布。\n", encoding="utf-8")
    shutil.copy2(args.build_record, windows / "build-record.json")
    status = ("Authorized release package; includes the matching SDK source tree."
              if labels["approved"] else "Local candidate, not approved for publication; includes matching SDK source.")
    (windows / "PREBUILT.md").write_text(
        f"# FrameBraid Windows x64 {labels['version']}\n\n"
        f"{status}\n\n"
        "Start with [prebuilt integration](docs/USE_PREBUILT.md). "
        "DLLs: bin/win-x64; import libraries: lib/win-x64. "
        "FFmpeg and CPU ORT are separate directories under runtime/.\n\n"
        "附同源 SDK 源码；先读预编译接入说明，"
        "无需为使用 DLL 安装 Rust。匹配依赖许可、FFmpeg 对应源码/配方在 licenses/。\n",
        encoding="utf-8")
    checked_links = verify_document_links(windows)
    package = {"schemaVersion": 1, "project": "FrameBraid SDK", "packageVersion": labels["version"],
               "purpose": labels["purpose"], "sourceCommit": commit,
               "configuration": "release", "platform": "windows-x64-cpu",
               "sourceArchive": {"file": source_name, "sha256": digest(output / source_name)},
               "materialsProfile": "minimal-20261026", "registeredNativeMaterialGaps": [],
               "distributionApproved": labels["approved"], "runtimeAcceptance": "verify-actual-packaged-binaries-separately",
               "rustDependencyInventory": "sdk/third-party/rust/inventory.json",
               "relativeDocumentLinksChecked": checked_links,
               "files": inventory(windows)}
    write_json(windows / "artifact-manifest.json", package)
    with zipfile.ZipFile(output / windows_name, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for row in inventory(windows):
            archive.write(windows / row["path"], row["path"])
    rows = [{"file": name, "bytes": (output / name).stat().st_size,
             "sha256": digest(output / name)} for name in [source_name, windows_name]]
    write_json(output / "delivery-manifest.json", {"schemaVersion": 1, "sourceCommit": commit,
               "purpose": labels["purpose"], "distributionApproved": labels["approved"], "artifacts": rows})
    (output / "SHA256SUMS.txt").write_text(
        "".join(row["sha256"] + "  " + row["file"] + "\n" for row in rows) +
        digest(output / "delivery-manifest.json") + "  delivery-manifest.json\n", encoding="utf-8")
    print(json.dumps({"sourceCommit": commit, "artifacts": rows, "distributionApproved": labels["approved"]}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ["output", "evidence", "dll-dir", "ffmpeg-dir", "ort-dir", "materials-dir", "build-record"]:
        parser.add_argument("--" + name, required=True)
    parser.add_argument("--version", default="0.1.0-local")
    parser.add_argument("--publication-authorized", action="store_true",
                        help="仅在负责人明确授权发布后设置；本脚本仍不上传任何文件")
    prepare(parser.parse_args())
