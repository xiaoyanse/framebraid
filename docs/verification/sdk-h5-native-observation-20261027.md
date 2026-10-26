# H5 原生执行观察与结果交付分离

状态：**已真实merge到main并由主代理完成源码编译、真实CPU定向验证及全工作区回归**。这是状态正确性修复，不是性能收益或完整硬件验收。

评审基线：`ca537222f040c122fe6549d01607be9446340ced`。本窄包静态复核仅涉及原生成功分支的记账位置与定向测试；rustfmt 和 git diff --check 已执行，编译、clippy及实测尚未执行。没有新线程、额外输入复制、模型重建或新的原生调用；观察仍取私有 selection.selected，不采信可修改的兼容字段。

## 目标与边界

按 MET-2 的 M-SELECT/M-OBS 核对真实执行证据：ORT Run 成功即记录进程后端执行观察；后续结果预算拒绝不抹去该事实，成功交付另记 delivered_runs。保护允许列表、唯一执行、输出预算、错误返回及真实后端身份，不追求推理提速。

原实现仅在输出全部复制及 batch 校验通过后设置进程 executed_mask；因此 Run 成功但输出预算不足时，模型 native_runs=1、delivered_runs=0，而进程观察仍可能为0。候选将 runtime::executed 移至原生成功分支，不改旧 query_backends 的驱动探测行为、ABI、语言绑定或设备范围。

## 预定验证

使用已有合成 Identity 模型，精确测试名在独立测试进程执行，避免进程累计执行观察被其他测试预热污染。模型路径及运行库由环境显式提供，不提交或复制输入。

1. 加载模型后 executed_mask=0，初始化/建会话不能当真实执行。
2. 将旧公开 actual_backend 字段改为CUDA，保持真实选择CPU，证明它不能污染观察。
3. 显式调用一次 Run、输出预算为0：返回 sdk.resourceExhausted；native_runs=1、delivered_runs=0、native_failures=0、executed_mask=1。
4. 连续三次查询计数不变，不因查询自动重放。
5. 调用方再显式调用一次，正确交付原48字节：native_runs=2、delivered_runs=1，观察仍仅CPU位。

此用例不证明真实GPU执行、五类YOLO精度、跨语言DLL调用或4h/24h稳定性；原有原生失败不记成功与无自动重放测试须同时保留。

## 主代理执行命令

在集成后的 FrameBraid 主目录执行，编译和测试输出由主代理统一保存；该工作树未执行编译或测试：

```powershell
$env:AM_SDK_ORT_LIBRARY = 'E:/projects/framebraid/.local/releases/candidates/0.1.0-local-20261026-final/windows-x64/runtime/onnxruntime-cpu/onnxruntime.dll'
$env:AM_SDK_RAW_MODEL = 'E:/projects/framebraid/.local/tests/r1-baseline-inputs/tensors/image.onnx'
cargo test -p am-vision --all-features sdk::selection_tests::native_success_is_observed_when_output_budget_rejects_delivery -- --ignored --exact --nocapture
cargo clippy -p am-vision --all-features --all-targets -- -D warnings
```

主代理需补实际命令结果、候选源码身份和输入/运行库摘要，再据结果登记通过或失败。公共路线、OpenSpec、来源修订与交付引用由主代理集成时统一同步；本保存检查点不关闭H5完整硬件矩阵。

## 集成复核结果

主代理在集成源码上实测：新增Identity回归通过，输出预算0时明确拒绝交付，但native_runs=1、delivered_runs=0、executed_mask=CPU；显式第二次Run才得到正确48字节及2/1计数。原生Gather失败回归也通过：native_failures=1、native_runs=0、无自动重放。am-vision全特性/全目标clippy通过。

同次工作区全特性回归302项通过、20项明确忽略；两个本节显式ignored回归各独立执行通过，不将其余ignored项计为通过。日志在主仓`.local/evidence/h5-native-observation-20261027`，不重新构建或替换旧Release DLL。源输入与CPU ORT摘要另存identity.json。

审查基线为修复分支的ca53722，已读真实差异与native结果处理上下文；致命0、阻断0。只将native Ok观察前移，Run失败和交付累计语义保留。旧同步query_backends驱动行为、加速实际Run及长期故障仍待；不以本小包关闭H5.1/3/4。
