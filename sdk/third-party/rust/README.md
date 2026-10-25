# Cargo dependency notices / Cargo 依赖许可

`inventory.json` identifies 148 resolved Windows-target registry packages,
their declared license expressions, original notice filenames and SHA-256.
`NOTICES-1.txt` through `NOTICES-3.txt` concatenate those originals without
rewriting their contents. Section headers identify each package/version/file.

清单与三份正文覆盖当前 Windows Cargo 元数据中的 148 个第三方包，
包括保守纳入的构建依赖，不宣称每个包都静态链接进最终 DLL。
原始许可内容及版权人不改写。

Regenerate/check using Python 3.12+ after fetching locked Cargo dependencies:

```text
python scripts/sdk/collect-rust-licenses.py
python scripts/sdk/collect-rust-licenses.py --check
```

The collector uses local Cargo metadata and cached upstream crate contents.
It fails if an upstream package lacks a declaration or original notice file.
It collects nested notices as well, including ring's BoringSSL/ISC material.
This inventory is separate from native FFmpeg/ORT material validation and is
not a statement that all components use the project's Apache-2.0 license.

使用本地缓存和锁定依赖图，无声明或无原文则失败；也收集嵌套声明。
它不代替 FFmpeg/ORT 材料核验，不将整棵依赖图归为 Apache-2.0。
