"""公开候选打包器的路径与原始资料复制边界回归，不启动 SDK。"""

import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile


spec = importlib.util.spec_from_file_location("bundle", Path(__file__).with_name("prepare-public-bundle.py"))
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)


class BundleBoundaries(unittest.TestCase):
    def test_publication_requires_explicit_approval_and_safe_version(self):
        local = bundle.release_labels("0.1.0-local", False)
        self.assertFalse(local["approved"])
        public = bundle.release_labels("0.1.0", True)
        self.assertEqual(public["windows"], "FrameBraid-SDK-windows-x64-0.1.0.zip")
        self.assertTrue(public["approved"])
        for version in ["../escape", "1.0.0/path", "0.1.0-local"]:
            with self.assertRaises(ValueError):
                bundle.release_labels(version, True)

    def test_reject_source_and_existing_output(self):
        with self.assertRaises(ValueError):
            bundle.safe_directory(str(bundle.ROOT), fresh=True)
        with tempfile.TemporaryDirectory(prefix="framebraid-bundle-") as root:
            with self.assertRaises(ValueError):
                bundle.safe_directory(root, fresh=True)

    def test_archive_traversal_and_links(self):
        with tempfile.TemporaryDirectory(prefix="framebraid-bundle-") as root:
            root = Path(root)
            for name, linked in [("../escaped.txt", False), ("linked", True)]:
                archive = root / "bad.zip"
                with zipfile.ZipFile(archive, "w") as stream:
                    info = zipfile.ZipInfo(name)
                    if linked:
                        info.external_attr = 0o120777 << 16
                    stream.writestr(info, b"unexpected")
                with self.assertRaises(ValueError):
                    bundle.extract_source(archive, root / "stage")
                self.assertFalse((root / "escaped.txt").exists())

    def test_valid_source_is_copied_exactly(self):
        with tempfile.TemporaryDirectory(prefix="framebraid-bundle-") as root:
            root = Path(root)
            original = b"upstream notice\r\nexact bytes\n"
            with zipfile.ZipFile(root / "source.zip", "w") as stream:
                stream.writestr("licenses/LICENSE", original)
            bundle.extract_source(root / "source.zip", root / "stage")
            bundle.copy_tree(root / "stage", root / "copy")
            self.assertEqual((root / "copy/licenses/LICENSE").read_bytes(), original)
            self.assertEqual(bundle.inventory(root / "stage"), bundle.inventory(root / "copy"))

    def test_packaged_document_links(self):
        with tempfile.TemporaryDirectory(prefix="framebraid-bundle-") as root:
            root = Path(root)
            (root / "licenses").mkdir()
            readme = root / "licenses/README.md"
            readme.write_text("[license](../../LICENSE)\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                bundle.verify_document_links(root)
            (root / "LICENSE").write_text("test license\n", encoding="utf-8")
            readme.write_text("[license](../LICENSE)\n", encoding="utf-8")
            self.assertEqual(bundle.verify_document_links(root), 1)


if __name__ == "__main__":
    unittest.main()
