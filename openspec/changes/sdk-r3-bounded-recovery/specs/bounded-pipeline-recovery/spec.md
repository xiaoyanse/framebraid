# Spec Delta

## Purpose

为 FrameBraid 的全链路有界压力和故障恢复提供可重复、可观测且受资源与兼容边界约束的验收依据，区分已有实现、改造效果与未验证范围。

## ADDED Requirements

### Requirement: End-to-end bounded pressure

SDK MUST 约束所有内部队列和中转暂存的数量、字节及线程，外部持有独立统计，不用无限重试消化过载。

#### Scenario: Slow consumer under overload

- **WHEN** 消费速度低于输入且通知接收停滞
- **THEN** 预算内拒绝/丢弃有明确原因，健康消费者持续服务并满足测前目标卡预算，压力解除后恢复

#### Scenario: Native pixel work blocks

- **WHEN** 一源的像素初始化、解码或回读原生调用暂时阻塞
- **THEN** 压缩分发不在同一执行线程等待该调用；像素worker先取得现有decoder额度且不创建替代线程，队列数量/字节有界，溢出后旧在途结果不发布并等待真实关键帧恢复，关闭在原生调用返回前保持Closing

#### Scenario: Inference classes share an executor

- **WHEN** 一个 Context 的实时输入尚未归还，另一类或另一 Context 在自身字节预算内提交
- **THEN** 分别核分类暂存、本 Context 合计及执行器总额，不将其他分类或 Context 用量套入当前类较小上限；超额拒绝不留账、不产生请求 ID，归还屏障完成后才释放原占用

### Requirement: Generation-safe recovery and draining

恢复和关闭 SHALL 保留正确源代次与请求终态，重建参考链后才交付，关闭排空后不再回调。

#### Scenario: Disconnect during outstanding work

- **WHEN** 断流重连与在途请求或关闭重叠
- **THEN** 旧代次结果不冒充新结果，租约读取结束后释放，不出现迟到回调


### Requirement: Recovery and stability have scenario budgets

故障验收 SHALL 按目标卡分别记录恢复首个正确新鲜输出、恢复持续达标窗口、健康消费者服务和稳定段资源趋势；不得用故障期间低吞吐或首尾RSS差替代恢复/稳定性结论。

#### Scenario: A slow decoder shares a process with compressed consumers

- **WHEN** 原生解码/回读变慢且健康压缩消费者仍在读取
- **THEN** 按测前响应/交付预算验证隔离效果与队列/线程上限，不以“不重连”或“延迟绝对零影响”替代实测要求
