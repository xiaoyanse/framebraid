"""项目产物只允许写入忽略的 .local 分类目录。"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def local_path(value, category, fresh=False):
    original = Path(value)
    if not original.is_absolute():
        raise ValueError("Output must be an absolute path")
    for part in [original, *original.parents]:
        if part.is_symlink() or (part.exists() and part.is_junction()):
            raise ValueError("Refuse linked output directory")
    resolved = original.resolve()
    allowed = ROOT / ".local" / category
    if category not in {"build", "tests", "evidence", "releases"} or allowed not in resolved.parents:
        raise ValueError("Output must be a child of SDK .local/" + category)
    if fresh and resolved.exists():
        raise ValueError("Refuse existing output")
    return resolved
