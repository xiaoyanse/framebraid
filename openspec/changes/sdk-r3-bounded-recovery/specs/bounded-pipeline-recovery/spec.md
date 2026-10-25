# Spec Delta

## Purpose

为 FrameBraid 的全链路有界压力和故障恢复提供可重复、可观测且受资源与兼容边界约束的验收依据，区分已有实现、改造效果与未验证范围。

## ADDED Requirements

### Requirement: End-to-end bounded pressure

SDK MUST 约束所有内部队列和中转暂存的数量、字节及线程，外部持有独立统计，不用无限重试消化过载。

#### Scenario: Slow consumer under overload

- **WHEN** 消费速度低于输入且通知接收停滞
- **THEN** 预算内拒绝/丢弃有明确原因，健康消费者不被阻塞，压力解除后恢复

### Requirement: Generation-safe recovery and draining

恢复和关闭 SHALL 保留正确源代次与请求终态，重建参考链后才交付，关闭排空后不再回调。

#### Scenario: Disconnect during outstanding work

- **WHEN** 断流重连与在途请求或关闭重叠
- **THEN** 旧代次结果不冒充新结果，租约读取结束后释放，不出现迟到回调

