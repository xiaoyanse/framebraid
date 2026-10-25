"""目录归属回归：只验证输出边界，不构建或发布制品。"""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import local_paths


class LocalPaths(unittest.TestCase):
    def test_owned_categories_and_refusals(self):
        tests = local_paths.ROOT / ".local/tests/path-boundaries"
        tests.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=tests) as directory:
            root = Path(directory)
            with patch.object(local_paths, "ROOT", root):
                for category in ["build", "tests", "releases", "evidence"]:
                    target = root / ".local" / category / "candidate"
                    self.assertEqual(local_paths.local_path(target, category), target)
                for target in [root, root / "crates/new", root.parent / "outside", Path("relative")]:
                    with self.assertRaises(ValueError):
                        local_paths.local_path(target, "build")
                with self.assertRaises(ValueError):
                    local_paths.local_path(root / ".local/build", "build")
                with self.assertRaises(ValueError):
                    local_paths.local_path(root / ".local/wrong/new", "wrong")
                target = root / ".local/evidence/existing"
                target.mkdir(parents=True)
                with self.assertRaises(ValueError):
                    local_paths.local_path(target, "evidence", fresh=True)


if __name__ == "__main__":
    unittest.main(verbosity=2)
