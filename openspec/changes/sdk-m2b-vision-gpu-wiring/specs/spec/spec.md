## ADDED Requirements

### Requirement: Vision GPU 帧输入

推理输入 SHALL 支持同进程 GPU 帧直传（纹理指针，借用期内有效）；frame_kind 协商失败或 GPU 帧不可用时 SHALL 回退 CPU 像素输入且不失败。

#### Scenario: GPU 帧直传推理
- **WHEN** 模型支持 GPU 输入且调用方传入 M2a GPU 帧句柄（同进程）
- **THEN** 推理经 CUDA 互操作与 GPU 预处理直达 IOBinding，全程零 CPU 回读

#### Scenario: 回退矩阵
- **WHEN** 无 CUDA 引擎/SOFTWARE_ONLY/GPU 帧句柄无效
- **THEN** 按 CPU 像素路径执行并计数回退，不失败整个请求

### Requirement: CUDA 互操作纹理生命周期

纹理注册（cudaGraphicsD3D11RegisterResource）SHALL 在 GPU 帧借用期内完成注册与 unmap；归还/释放前 SHALL unmap 注销，不跨租约持有纹理。

#### Scenario: 借用期内注册与注销
- **WHEN** GPU 帧发布→消费→归还
- **THEN** 注册/unmap 全程发生在租约窗口内，归还后无残留注册

### Requirement: 输入搬运审计

GPU 输入路径 SHALL 计数每帧拷贝次数与字节数（copies_per_frame/bytes_transferred），CPU 回退路径单列；数字经统计接口可读（M2c 验收主指标）。

#### Scenario: GPU 路径搬运计数
- **WHEN** 一帧经 GPU 直传推理完成
- **THEN** 搬运计数反映实际 GPU 内拷贝次数（目标 0 次 CPU 回读）
