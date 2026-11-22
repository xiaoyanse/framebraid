# Tasks

- [x] 1.1 合同：dual-dll-abi.md 枚举段新增 AM_BACKEND_DIRECTML=3（device_id 忽略语义、矩阵行、probe 报告 directml 段），先合同后实现
- [x] 1.2 生成器模型更新 + 运行生成器同步 C 头/Rust ABI/Python/C# 绑定与常量，--check 全绿
- [x] 1.3 Rust 实现：backend.rs DirectML 分支（运行库探测）、会话 DmlExecutionProvider 附着、矩阵第四行
- [x] 1.4 单测：枚举值兼容、矩阵行、无运行库不误报、策略回退
- [x] 1.5 真实运行库实测（.local/runtimes/ortdml-rt）：加载+真实推理+矩阵 verified=1
- [x] 1.6 验证记录 + fmt/clippy/workspace 回归 + 冻结/边界检查 + 本地提交
