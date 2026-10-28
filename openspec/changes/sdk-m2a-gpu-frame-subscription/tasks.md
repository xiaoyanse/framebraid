# Tasks

- [x] 2.1 合同：dual-dll-abi.md 新增 GPU 帧对象/池/订阅/frame_kind/能力位/计费类目/审计计数器章节（含共享 NT 句柄同进程约束）
- [x] 2.2 生成器模型更新 + 同步 C 头/Rust ABI/Python/C# 绑定，--check 全绿
- [x] 2.3 am-camera：硬解帧分流（GPU 路径旁路 readback；缺共享标志帧降级+标注）
- [x] 2.4 camera-ffi：GPU 帧池（复用共享池骨架分账）+ GPU 订阅 + 四个新导出
- [x] 2.5 单测：池满拒绝/租约归还/关闭排空/SOFTWARE_ONLY 拒绝/降级计数
- [ ] 2.6 真实硬解实测（合成 H.264 + 本机 D3D11VA）：GPU 帧交付 + NT 句柄可得 + 审计计数正确
- [ ] 2.7 验证记录 + fmt/clippy/workspace 回归 + 冻结/边界检查 + 本地提交
