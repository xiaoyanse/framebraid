# 攒批候选打包复验（2026-10-28）

补验 [R5 3.2](sdk-r5-final-closeout-20261028.md) 登记的"下次发布候选打包复验"项。源码 `main` 提交 `898ff64`（五包收口后），干净检出 Release 构建（am_camera `2150bea2…` / am_vision `3d8f45de…`，含攒批扩展与死锁修复），锁定材料打包 `FrameBraid-SDK-windows-x64-0.1.0-local-r5clean-20261028.zip`（SHA-256 `845d14b3…`，32,941,326 字节，`distributionApproved=false` 未发布），解压独立目录复验：

| 项 | 结果 |
| --- | --- |
| Python 推理示例 | 通过（sync/async、fetch/ack、共享租约） |
| C 推理 / 生命周期示例（GCC `-Werror` + 包内头/导入库） | 通过 |
| C++17 推理示例（RAII/逆序清理） | 通过 |
| C# 推理示例（csc `/warnaserror` + 包内绑定） | 通过 |
| 新导出在包内绑定（Python FUNCTIONS 表 + AM_BATCHING_ENABLE 常量） | 存在 |
| **攒批六场景实调（包内 DLL + 包内 ORT，真实端到端）** | **全部通过**：固定轴拒绝、默认关闭、8 项凑满（formed=1/8 项）、5 项截止成批、取消剩余项成批交付、运行期关闭清零 |

证据 `.local/evidence/r5-clean-bundle-20261028/`。R5 3.2 的打包复验遗留项关闭；发布仍需另行授权过 RELEASE_CHECKLIST。四类真实模型权重、GPU/OpenVINO 后端依旧未验（设备/依赖条件不变）。
