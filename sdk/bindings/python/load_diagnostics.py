"""SDK 装载之前的诊断；此阶段不能调用尚未装载的 SDK 错误接口。"""
import ctypes as c
from pathlib import Path


class SdkLoadError(OSError):
    def __init__(self, path, original):
        self.path = str(path)
        self.system_code = getattr(original, "winerror", None) or getattr(original, "errno", None)
        if not path.is_file():
            hint = "SDK file is missing; deploy the matching SDK library at this absolute path."
        elif self.system_code == 193:
            hint = "Invalid binary format or architecture mismatch; use matching x64 process, SDK and dependencies."
        elif self.system_code == 126:
            hint = ("SDK file exists but a transitive dependency may be missing; check the package manifest, "
                    "matching VC++ x64 runtime and native DLL dependencies. Code 126 alone does not identify the missing library.")
        elif self.system_code == 127:
            hint = "A required entry point is missing; deploy matching SDK and dependency versions."
        else:
            hint = "Check the library format, permissions, architecture and native dependencies."
        super().__init__("Cannot load SDK '%s' (process %d-bit, system code %s). %s Original error: %s" %
                         (path, c.sizeof(c.c_void_p) * 8, self.system_code, hint, original))


def load_library(path):
    path = Path(path).resolve()
    try:
        return c.CDLL(str(path))
    except OSError as error:
        raise SdkLoadError(path, error) from error
