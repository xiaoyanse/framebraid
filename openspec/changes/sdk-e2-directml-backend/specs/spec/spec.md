## ADDED Requirements

### Requirement: AM_BACKEND_DIRECTML 枚举值

`am_backend_e` SHALL 包含 `AM_BACKEND_DIRECTML = 3`，作为 Windows DirectML 执行后端的标识。

#### Scenario: 枚举追加不破坏旧值
- **WHEN** 使用 SDK 1.0/1.1 头文件的调用方检查 CPU/OpenVINO/CUDA 值
- **THEN** 三值语义与位不变，DirectML 为纯追加

### Requirement: 后端矩阵第四行

`amv_query_backends` 矩阵 SHALL 为 DirectML 提供独立行，字段语义与其他行一致（hw_detected/deps_present/provider_loadable/verified）。

#### Scenario: 有运行库与核显
- **WHEN** 运行库目录含 DirectML.dll 且系统枚举到 DirectML 适配器
- **THEN** hw_detected=1, deps_present=1；策略选择后 loadable/verified 置位

#### Scenario: 无 DirectML 运行库
- **WHEN** 运行库目录与系统均无 DirectML.dll
- **THEN** hw_detected=0, deps_present=0, 后续字段不置位

### Requirement: 策略选择与回退

`am_backend_selection_t` 候选列表 SHALL 接受 DirectML 候选；device_id 字段对 DirectML 忽略（合同注明）。

#### Scenario: DirectML 首选允许回退
- **WHEN** 候选=[(DIRECTML,0),(CPU,0)]，allow_fallback=1，DirectML 会话创建失败
- **THEN** 模型加载成功且回退原因可查询，实际执行为 CPU
