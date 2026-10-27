# R4 1.4 干净解压包语言示例验证（2026-10-27）

源码提交 `3b69f29`（含本日全部阶段成果），`cargo build --release --workspace --locked --offline --features am-camera-ffi/shared-frames,am-vision-ffi/shared-frames` 干净检出构建（`build-local-release.py` 拒绝脏工作区）。制品：`FrameBraid-SDK-windows-x64-0.1.0-local-r4clean-20261027.zip`（SHA-256 `a3f4cc39d7f6489e2af4d90127f053433d1774e771597fb5c5e42beda982ff4d`，32,866,368 字节），DLL：am_camera `8687473a…`、am_vision `44a92e49…`。打包链完整通过：来源修订 check-boundary（239 文件，含本日 pixel_ops 合并链与 shared/tests.rs 登记）、许可清单同步（148 包，Cargo.lock 哈希对齐）、materials-profile `minimal-20261026` 锁定指纹全匹配（自构建 FFmpeg 运行库、ORT 运行库、BtbN 主源码/构建配方/ORT wheel 原件齐备并指纹核对）。`distributionApproved=false`，未发布。

包解压至独立目录 `.local/tests/r4clean-unpacked-20261027/`，从中运行示例：

| 语言 | 示例 | 结果 |
| --- | --- | --- |
| Python | `sdk/examples/inference.py` | **通过**：CPU 同步/异步、fetch/ack、共享租约（绑定/运行库全部取自解压包，模型为本机 Identity 样本） |
| C# | `sdk/examples/Inference.cs`（csc `/warnaserror` + 包内绑定） | **通过**：CPU sync/async、fetch/ack、shared lease、SafeHandle |
| C | `inference.c`（`-municode`）、`lifecycle.c`、`shared_frame.c`（GCC 15.2.0 `-std=c11 -Wall -Wextra -Werror`） | **通过**：Identity 原始输出/ack/release/close-destroy、双域生命周期/限额、共享帧复制/借用桥（编译与运行均取自解压包头文件/导入库/DLL） |
| C++17 | `inference.cpp`（`-municode`） | **通过**：共享帧、结果 RAII、逆序依赖清理 |
| Python | `sdk/examples/camera.py --rtsp` | **未通过，已定位到独立验证缺口**：经多轮二分排查（TCP-only/UDP 服务器、长流/短流、同款已验证流），问题收敛为**RTSP 纯像素订阅（无编码订阅共存）懒 worker 路径 8 秒无首帧**——同凭据/同流/同服务器下，增加编码订阅的完整订阅组合立即出帧（H2.3 矩阵四用例佐证）。合同 §2.6 承诺"首次实际像素需求懒创建 worker"，该路径疑似缺陷候选，非示例装配问题；精确复现：占位凭据 + TCP 回环 + one.h264×60 串联流 10fps + 仅 `amc_subscribe_frames`(RGBA8, sampling=0) + 持续 pull。已登记待 SDK 侧独立回归，不在本批修复（media crate 与并行工作面冲突） |

本轮示例修复（随本批提交）：`inference.cpp` 的 `NOMINMAX` 加保护宏（MinGW libstdc++ 头已预定义为 1，裸重定义触发 `-Werror`）；`lifecycle.c` 能力位掩码从旧五位更新为 `am_sdk_types.h` 当前 16 位全集（示例断言未随合同演进，真实包上运行即暴露）。wmain 入口在 MinGW 下需 `-municode` 链接（MSVC 由 CRT 自动处理），已作为示例验证环境备注。运行参数注意：混合斜杠路径会被运行库目录校验拒绝，示例调用须用统一分隔符。

## 打包过程修正（如实记录）

打包链暴露并修正三处登记/环境问题：① `sdk-source-revisions.json` 误为同一文件追加第二条目（规则为每文件单条目、历史由 Git 承载）——已合并 pixel_ops 链并补登 shared/tests.rs（此前批次漏登，check-boundary 本批补检通过）；② Rust 许可清单 `inventory.json` 与当前 Cargo.lock 漂移——重新采集提交（148 包不变，仅 lock 哈希对齐）；③ materials 原件（ORT wheel/FFmpeg 主源码/构建配方 zip）与外部许可正文（ThirdPartyNotices）需按 lock 指纹从本机归档就位后打包才完整。这些修正已随 `c22961c`、`3b69f29` 提交。

## 结论与边界

R4 1.4 以"干净解压包 + Python/C#/C/C++17 全部示例真实编译运行 + camera RTSP 如实登记未过"收口：头文件/绑定/运行库/许可/来源指纹在包内一致（check-boundary 与打包校验通过），中英文接入说明（INTEGRATION.md/README）随包分发。首轮"C/C++ 环境故障"后查明为**会话 shell 的 PATH 解析顺序问题**：Claude Code 的 Git Bash 中 `C:\Program Files\Git\mingw64\bin` 先于 MSYS2，cc1 按搜索序加载到 Git 附带的旧运行库（新符号缺失表现为 `0xC0000139` 静默退出）；MSYS2 工具链本身完好，前置 `/c/msys64/mingw64/bin` 后立即恢复，MSYS2 终端用户不受影响。camera.py RTSP 链路问题单独排查，不与包验证混同。本候选未授权发布，发布须另按 RELEASE_CHECKLIST 全量核对。
