## ADDED Requirements

### Requirement: OpenVINO 原生引擎

Vision SDK SHALL 支持以 OpenVINO 原生 C API（ov_core 直连）作为与 ORT 并列的推理引擎，经 AM_BACKEND 新枚举值选择，五档案（pose/detect/classify/segment/obb）输入输出语义与 ORT 路径一致。

#### Scenario: 原生引擎推理
- **WHEN** 模型以 openvino-native 后端加载且 openvino.dll 在 runtime_dirs
- **THEN** 推理经 ov_core 直连执行，输出与 ORT CPU 同模型同输入在容差内一致

#### Scenario: 回退矩阵
- **WHEN** openvino.dll 缺失/模型格式不支持/设备不可用
- **THEN** 按既有多后端 selection 回退 ORT CPU，回退原因可观测（不静默）

### Requirement: 多引擎抽象

session 层 SHALL 以引擎抽象并列 ORT 与 OpenVINO 原生两条路径，ORT 现有行为零改动；GPU 帧输入（M2b §3.3a）的引擎适配位对两引擎统一。

#### Scenario: ORT 路径不受影响
- **WHEN** 以既有 backend 值加载模型
- **THEN** 行为与 E1 之前完全一致（selection/回退/统计）
