# R2/H2.0b 共用硬件选择工厂（2026-10-26）

基线：`0cbf638`（完整提交由本记录对应Git历史核定）。仅FrameBraid main、本地合成H.264和隔离FFmpeg，AM-RT未动、未发布。

## 本阶段范围

Rust `CpuDecoder::create_with_policy`返回解码器和拥有的选择报告。SoftwareOnly不枚举/初始化硬件；PreferHardware复用probe共享准入，按目标解码器配置、D3D11VA优先及DXGI快照顺序、再DXVA2选择。无匹配、缺配置、初始化错误、探测忙或预算耗尽均保留原因并返回CPU；非法PreferHardware预算明确拒绝。`selected`表示初始化选择，实际后端仍在真实输出后由`hardware_backend()`确认。

工厂不接管码流队列，不自动重放包，不用初始化证明profile/位深可用。C ABI与RTSP仍未接入此工厂；H2.1～H2.4、全局字节预算及运行期关键帧恢复保持未完成。本阶段仅关闭H2.0b。

## 实测

工具：`cargo run --release --example hardware_select -- <runtime> <single-au-1080p.h264>`。真实FFmpegCPU路径、按需真实设备路径与函数指针故障注入分别记账；不连接设备。

| 场景 | 结果 |
|---|---|
| SoftwareOnly，预算0（不用于CPU路径） | 硬件创建次数0，CPU校验`8816365bfed01b95` |
| 新运行库真实重试，三轮 | 选中Intel D3D11VA，初始化125/128/127ms；随后真实帧验证成功，PTS=17、1920×1080 |
| 旧运行库对照 | D3D11目标解码配置缺失时不创建该设备；选中映射正确的DXVA2，真实输出成功，初始化144ms |
| 原生设备初始化全部注入-12 | 有限尝试后CPU回退，输出与CPU校验一致，保留逐项错误 |
| 目标解码配置全部注入缺失 | 不尝试创建硬件设备，CPU输出一致 |
| 显式probe占据共享准入 | 选择工厂立即CPU回退，原因Busy，三轮耗时均低于1ms显示精度；不等待probe完成 |
| 1ns预算，已不足以继续 | 在枚举前CPU回退，原因BudgetExhausted，三轮低于1ms显示精度 |
| PreferHardware预算0 | InvalidInput，不静默改变预算 |
| 失败后的新真实请求 | 仍能选硬件并输出真实帧，无永久负缓存 |

这些是选择及故障隔离证据，不是解码吞吐或性能提升。输入仍为单AU，硬解RGBA与CPU路径差异仍按H2.0记录保留；不推导P/B帧、长跑或其他平台结果。软预算只能在原生调用间检查，已进入驱动无法强杀。

证据：`.local/evidence/h2-selection[-round-N|-legacy-runtime]-20261026.json`，源文件、样本、EXE、运行库摘要见`.local/evidence/h2-selection-manifest-20261026.json`。故障模式在本示例的FfmpegApi副本注入，不改变系统驱动或磁盘运行库。

## 门禁与自查

am-camera回归47通过/1显式样本测试未在本轮重跑；严格clippy/fmt、am-camera-ffi全目标check通过；ABI冻结8项/生成121函数、来源166文件检查和五包OpenSpec严格校验均通过。注意初始化选择与真实执行状态分离、probe/selection共享锁、所有权和现有CPU输出；没有扩展C ABI或改写冻结文件。选择函数与验收示例较长，但每轮/项数有界且按策略职责隔离。现有外部`.gitignore`变更仍保留、不提交。

下一步：直接解码/RTSP统一接线前，补足回读预算与输出一致性；运行期故障按本代次停止硬件重试并等待随机访问点，不盲重发已经接受的包。之后继续H5和R2～R5剩余任务。
