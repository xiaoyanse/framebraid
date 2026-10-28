# Design: E2 — DirectML 后端枚举扩展

## 枚举与选择语义

- `AM_BACKEND_DIRECTML = 3`：追加在 CUDA(2) 之后，不重排既有值（ABI 冻结规则）
- 后端矩阵行新增：hw_detected 语义 = DirectML.dll 可加载 + 枚举到至少一个适配器；deps_present = 系统存在 d3d11 与 DirectML 运行库；provider_loadable = ORT 会话以 DmlExecutionProvider 初始化成功；verified = 真实推理成功一次
- device_id：DirectML EP 自行选择适配器（ORT 1.24 的 DML EP 不暴露多设备枚举给 API），候选元组中 DirectML 的 device_id 忽略（合同注明）
- 回退：与其他后端一致（候选列表次序+allow_fallback）

## 探测

- 显式探测（amv_probe_inference_hardware）：CUDA 驱动探测保持不变；DirectML 探测 = 尝试加载运行库目录下 DirectML.dll + 系统目录 fallback，成功则在报告中新增 directml 段（available/adapter 计数不承诺具体型号）
- 无设备时不误报：probe 报告 directml.available=false

## 实现层

- backend.rs 新增 DirectML 分支：运行库文件探测（onnxruntime.dll 同目录 DirectML.dll 存在性 + d3d11.dll 系统存在性）
- executor/session：创建时附加 DmlExecutionProvider（当策略选中 DirectML）
- 矩阵：第四行填入

## 不做

- 不做 DirectML 设备级选择（等 ORT 上游暴露）
- 不改 OpenVINO/CUDA 既有语义
- 不在本 change 做 IOBinding/GPU 帧输入（M2b 范围）
