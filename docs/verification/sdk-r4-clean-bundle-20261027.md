# R4 1.4 干净解压包语言示例验证（2026-10-27）

源码提交 `3b69f29`（含本日全部阶段成果），`cargo build --release --workspace --locked --offline --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames` 干净检出构建（`build-local-release.py` 拒绝脏工作区）。制品：`FrameBraid-SDK-windows-x64-0.1.0-local-r4clean-20261027.zip`（SHA-256 `a3f4cc39d7f6489e2af4d90127f053433d1774e771597fb5c5e42beda982ff4d`，32,866,368 字节），DLL：am_camera `8687473a…`、am_vision `44a92e49…`。打包链完整通过：来源修订 check-boundary（239 文件，含本日 pixel_ops 合并链与 shared/tests.rs 登记）、许可清单同步（148 包，Cargo.lock 哈希对齐）、materials-profile `minimal-20261026` 锁定指纹全匹配（自构建 FFmpeg 运行库、ORT 运行库、BtbN 主源码/构建配方/ORT wheel 原件齐备并指纹核对）。`distributionApproved=false`，未发布。

包解压至独立目录 `.local/tests/r4clean-unpacked-20261027/`，从中运行示例：

| 语言 | 示例 | 结果 |
| --- | --- | --- |
| Python | `sdk/examples/inference.py` | **通过**：CPU 同步/异步、fetch/ack、共享租约（绑定/运行库全部取自解压包，模型为本机 Identity 样本） |
| C# | `sdk/examples/Inference.cs`（csc `/warnaserror` + 包内绑定） | **通过**：CPU sync/async、fetch/ack、shared lease、SafeHandle |
| C/C++ | inference.c/.cpp、lifecycle.c、shared_frame.c | **本轮未重跑**：本机 MinGW gcc 15.2.0 的 `cc1` 静默失败（`--version` 零输出、最小程序 exit 1，环境故障非源码问题；无 MSVC/clang 可用）。同示例集在 2026-10-26 [ABI 1.1 记录](sdk-r2-abi11-hardware-20261026.md)有 GCC C11/C++17 `-Wall/-Wextra/-Werror` 编译与真实 DLL 调用证据（源码树构建）；干净包与本批源码差异仅本日测试脚本/RG 表优化（已过 workspace 回归），头文件与导入库在包内经 check-boundary 生成链核对一致 |
| Python | `sdk/examples/camera.py --rtsp` | **环境装配未通过**：本地 MediaMTX 回环推流（外部 ffprobe 可读到 H.264 流）下 SDK 帧拉取 5 秒超时，根因未定位；不推断 SDK 缺陷也不冒充通过，登记待查 |

## 打包过程修正（如实记录）

打包链暴露并修正三处登记/环境问题：① `sdk-source-revisions.json` 误为同一文件追加第二条目（规则为每文件单条目、历史由 Git 承载）——已合并 pixel_ops 链并补登 shared/tests.rs（此前批次漏登，check-boundary 本批补检通过）；② Rust 许可清单 `inventory.json` 与当前 Cargo.lock 漂移——重新采集提交（148 包不变，仅 lock 哈希对齐）；③ materials 原件（ORT wheel/FFmpeg 主源码/构建配方 zip）与外部许可正文（ThirdPartyNotices）需按 lock 指纹从本机归档就位后打包才完整。这些修正已随 `c22961c`、`3b69f29` 提交。

## 结论与边界

R4 1.4 以"干净解压包 + Python/C# 真实运行 + C/C++ 凭既有同示例证据 + camera RTSP 如实登记未过"收口：头文件/绑定/运行库/许可/来源指纹在包内一致（check-boundary 与打包校验通过），中英文接入说明（INTEGRATION.md/README）随包分发。C/C++ 干净包重跑待编译环境修复后补一次即可（示例与包不变）；camera.py RTSP 链路问题单独排查，不与包验证混同。本候选未授权发布，发布须另按 RELEASE_CHECKLIST 全量核对。
