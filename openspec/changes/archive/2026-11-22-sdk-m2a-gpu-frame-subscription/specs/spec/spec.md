## ADDED Requirements

### Requirement: GPU 帧池创建与租约

SDK SHALL 提供 GPU 帧池的创建/借出/归还/销毁导出，语义（有界/租约/关闭排空）与既有共享帧池一致，按显存字节新类目计费。

#### Scenario: 池满拒绝
- **WHEN** GPU 帧池帧数达上限且有借出未还
- **THEN** 新发布被拒绝且不覆写活帧

### Requirement: GPU 帧订阅

像素订阅 SHALL 支持 GPU 帧交付模式（frame_kind 追加字段，struct_size 协商）；SOFTWARE_ONLY 策略下 GPU 订阅按合同报不支持，不静默降级。

#### Scenario: 硬解激活时 GPU 订阅交付
- **WHEN** PREFER_HW 实际硬解且订阅 frame_kind=GPU
- **THEN** 拉取返回 GPU 帧句柄（含元数据与共享 NT 句柄）

### Requirement: 搬运审计挂钩

GPU 帧路径 SHALL 预留搬运计数器（gpu_frames_delivered/cpu_fallbacks/bytes_transferred_est），经统计接口可读。

#### Scenario: 回退路径计数
- **WHEN** 纹理缺共享标志降级回读
- **THEN** 元数据标注 fallback 且 cpu_fallbacks 递增
