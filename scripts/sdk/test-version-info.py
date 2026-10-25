"""只验证构建身份和真实版本导出，不初始化媒体或推理运行库。"""

import argparse
import ctypes as c
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from local_paths import local_path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/bindings/python"))
import amrt_sdk as a


def run(*command, cwd, env=None):
    return subprocess.check_output(command, cwd=cwd, env=env, text=True, encoding="utf-8").strip()


def git(root, *args):
    return run("git", *args, cwd=root)


def commit(root):
    git(root, "add", ".")
    git(root, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
        "-c", "commit.gpgsign=false", "commit", "--quiet", "--no-verify", "-m", "fixture")
    return git(root, "rev-parse", "HEAD")


class VersionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="version-", dir=WORK)
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)

    def fixture(self, directory, version="8.7.6"):
        root = self.base / directory
        crate = root / "crates/sdk-support"
        (crate / "src").mkdir(parents=True)
        (root / ".gitattributes").write_text("* -text\n", encoding="utf-8")
        (root / "Cargo.toml").write_text(
            '[workspace]\nmembers=["crates/sdk-support"]\nresolver="2"\n'
            '[workspace.package]\nversion="' + version + '"\nedition="2021"\n', encoding="utf-8")
        (crate / "Cargo.toml").write_text(
            '[package]\nname="version-probe"\nversion.workspace=true\nedition.workspace=true\n',
            encoding="utf-8")
        (crate / "build.rs").write_bytes((ROOT / "crates/sdk-support/build.rs").read_bytes())
        (crate / "src/main.rs").write_text(
            'include!(concat!(env!("OUT_DIR"), "/sdk_version.rs"));\n'
            'fn main() { for s in [PACKAGE_VERSION, BUILD_TARGET, BUILD_NOTES] '
            '{ println!("{}", s.to_str().unwrap()); } }\n', encoding="utf-8")
        env = os.environ.copy()
        env["CARGO_TARGET_DIR"] = str(self.base / "target")
        run("cargo", "generate-lockfile", "--offline", cwd=root, env=env)
        return root, env

    def probe(self, root, env):
        lines = run("cargo", "run", "--quiet", "--locked", "--offline", cwd=root, env=env).splitlines()
        self.assertEqual(len(lines), 3)
        return lines[0], lines[1], dict(item.split("=", 1) for item in lines[2].split(";"))

    def test_cargo_rebuild_tracks_version_head_and_dirty_inputs(self):
        root, env = self.fixture("repository")
        git(root, "init", "--quiet")
        first_commit = commit(root)
        first = self.probe(root, env)
        self.assertEqual(first[0], "8.7.6")
        self.assertEqual(first[2]["source_commit"], first_commit)
        self.assertEqual(first[2]["source_state"], "clean")
        manifest = root / "Cargo.toml"
        manifest.write_text(manifest.read_text().replace("8.7.6", "8.7.7"), encoding="utf-8")
        run("cargo", "generate-lockfile", "--offline", cwd=root, env=env)
        second_commit = commit(root)
        second = self.probe(root, env)
        self.assertEqual(second[0], "8.7.7")
        self.assertEqual(second[2]["source_commit"], second_commit)
        # 只推进提交、不改编译输入，也必须刷新嵌入的提交号。
        (root / "NOTES.md").write_text("fixture", encoding="utf-8")
        third_commit = commit(root)
        self.assertNotEqual(second_commit, third_commit)
        self.assertEqual(self.probe(root, env)[2]["source_commit"], third_commit)
        source = root / "crates/sdk-support/src/main.rs"
        source.write_text(source.read_text() + "// changed\n", encoding="utf-8")
        self.assertEqual(self.probe(root, env)[2]["source_state"], "dirty")
        self.assertEqual(first[2]["profile"], "debug")
        self.assertEqual(first[2]["support_shared_frames"], "disabled")

    def test_source_archive_does_not_claim_parent_repository(self):
        git(self.base, "init", "--quiet")
        (self.base / "outer.txt").write_text("outer repository", encoding="utf-8")
        commit(self.base)
        root, env = self.fixture("source-archive")
        value = self.probe(root, env)
        self.assertEqual(value[0], "8.7.6")
        self.assertEqual(value[2]["source_commit"], "unknown")
        self.assertEqual(value[2]["source_state"], "unknown")
        self.assertTrue(value[1])

    def test_both_real_dll_exports_agree_with_this_build(self):
        import tomllib
        expected = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
        target = run("rustc", "-vV", cwd=ROOT).split("host: ", 1)[1].splitlines()[0]
        values = []
        for name, prefix in [("am_camera.dll", "amc_"), ("am_vision.dll", "amv_")]:
            path = (DLL_DIR / name).resolve(strict=True)
            library = c.CDLL(str(path))
            a.bind(library, prefix, require_all=True)
            function = getattr(library, prefix + "get_version_info")
            result = a.am_version_info_t()
            result.struct_size = c.sizeof(result)
            self.assertEqual(function(c.byref(result)), a.AM_OK)
            self.assertEqual(function(None), a.AM_ERR_SDK_NULL_ARGUMENT)
            self.assertEqual(result.package_version_utf8.decode(), expected)
            self.assertEqual(result.build_target_utf8.decode(), target)
            self.assertEqual((result.abi_major, result.abi_minor), (1, 0))
            notes = dict(item.split("=", 1) for item in result.runtime_notes_utf8.decode().split(";"))
            self.assertEqual(notes["source_commit"], git(ROOT, "rev-parse", "HEAD"))
            self.assertEqual(notes["profile"], DLL_DIR.name)
            self.assertEqual(notes["support_shared_frames"], "enabled")
            state = git(ROOT, "status", "--porcelain", "--untracked-files=all", "--",
                        "Cargo.toml", "Cargo.lock", "crates", "src")
            self.assertEqual(notes["source_state"], "dirty" if state else "clean")
            values.append((result.package_version_utf8, result.build_target_utf8, result.runtime_notes_utf8))
            OBSERVATIONS.append({"file": name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                                 "version": expected, "target": target, "notes": notes})
        self.assertEqual(values[0], values[1])


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    args = parser.parse_args()
    DLL_DIR = args.dll_dir.resolve(strict=True)
    WORK = local_path(args.work_dir, "tests")
    WORK.mkdir(parents=True, exist_ok=True)
    OBSERVATIONS = []
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(VersionTests))
    print(json.dumps({"tests": result.testsRun, "failures": len(result.failures),
                      "errors": len(result.errors), "dlls": OBSERVATIONS}, ensure_ascii=False))
    sys.exit(0 if result.wasSuccessful() else 1)
