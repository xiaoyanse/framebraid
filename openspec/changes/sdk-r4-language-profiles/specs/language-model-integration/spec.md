# Spec Delta

## Purpose

为 FrameBraid 的跨语言易用性与五类档案提供可重复、可观测且受资源与兼容边界约束的验收依据，区分已有实现、改造效果与未验证范围。

## ADDED Requirements

### Requirement: Owned cross-language access

语言封装 MUST 为复制输出和借用视图提供明确拥有者、只读与有效期，错误和关闭可预测，不依赖 GC 保障原生帧有效。

#### Scenario: Borrowed view outlives request

- **WHEN** 用户持有视图时请求结束或触发 GC
- **THEN** 安全拥有者继续持有或明确拒绝操作，释放配对且无额外无界队列

#### Scenario: Copy a pose result before releasing its owner

- **WHEN** Python 调用方读取 Pose 检测并在返回后释放结果
- **THEN** 框和关键点为独立复制值，复制期间关闭等待结果锁，关闭后再次调用明确拒绝；低层借用视图仍受原生所有者寿命限制

#### Scenario: Copy non-Pose structured results

- **WHEN** Python 调用方复制检测、分类、分割或旋转框结果后释放拥有者
- **THEN** 返回字段和掩膜是独立值，按原 ABI 保留概率、映射和角度语义；过量或无效掩膜明确拒绝，不能借用已释放指针

#### Scenario: C# copies a structured result

- **WHEN** C# 调用方复制检测、分类、分割或旋转框结果并释放拥有者
- **THEN** 复制数据保持独立、非空指针与尺寸及掩膜累计预算先验证；错误任务或关闭后拒绝，保留原 ABI 坐标及掩膜约定

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

### Requirement: Pixel operations retain their language owner

高层像素操作 SHALL 在同步调用及默认复制异步提交返回前保护拥有者，先锁owner再锁model；显式关闭后拒绝，不导出额外裸指针，独立复制接口写清成本。既有低层调用义务和C ABI不变。

#### Scenario: A shared borrow is returned after two model submissions

- **WHEN** 同一共享像素先提交给两个模型，默认复制提交返回后归还并触发GC
- **THEN** 两请求独立读取已接受内容、归还只一次；关闭后的门面明确拒绝，异常解锁；不将低层逃逸或外部数组写入称安全
