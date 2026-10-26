# Spec Delta

## Purpose

为 FrameBraid 的跨语言易用性与五类档案提供可重复、可观测且受资源与兼容边界约束的验收依据，区分已有实现、改造效果与未验证范围。

## ADDED Requirements

### Requirement: Owned cross-language access

语言封装 MUST 为复制输出和借用视图提供明确拥有者、只读与有效期，错误和关闭可预测，不依赖 GC 保障原生帧有效。

#### Scenario: Borrowed view outlives request

- **WHEN** 用户持有视图时请求结束或触发 GC
- **THEN** 安全拥有者继续持有或明确拒绝操作，释放配对且无额外无界队列

### Requirement: Versioned model profile diagnostics

模型档案 SHALL 明确版本/权重摘要/导出配置和前后处理，形状或语义不兼容明确拒绝。

#### Scenario: Unsupported profile input

- **WHEN** 模型或输入不匹配声明档案
- **THEN** 返回可定位诊断；五类支持按限定代表模型验证，不从 Identity 图推出业务准确性


### Requirement: Language efficiency preserves ownership and model correctness

跨语言性能 SHALL 分开测复制和借用的额外字节/分配/等待，保护拥有者、GC/关闭和档案容差；不同语言不要求相同耗时，五类模型实际语义不得被合成微基准替代。

#### Scenario: A borrowed view reduces copying

- **WHEN** 语言封装改用持有拥有者的只读视图
- **THEN** 核对实际复制图及生命周期，只有确实减少的复制可记收益；必要的解码回读/布局转换单列，输入提前归还不能导致悬空
