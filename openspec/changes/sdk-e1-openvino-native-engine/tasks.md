# Tasks

- [ ] E1.1 合同：dual-dll-abi.md 推理后端章节扩展（openvino-native 候选、AM_BACKEND 新值、运行库目录约定、回退矩阵、能力位）+ OpenVINO 运行库指纹入 materials 体系
- [ ] E1.2 生成器同步（枚举新值）+ 三语言绑定 --check 绿
- [ ] E1.3 OpenVINO C API 最小 FFI（ov_core/model/compiled_model/infer_request/tensor，显式加载 openvino.dll，符号存在性校验）
- [ ] E1.4 多引擎抽象：session 层引擎 trait（ORT 现路径不动，OpenVINO 并列）+ selection 候选扩展与回退（openvino-native→ORT CPU）
- [ ] E1.5 五档案适配：输入张量布局/输出后处理映射（pose/detect/classify/segment/obb），合成单测
- [ ] E1.6 真实模型对拍：OpenVINO 原生 vs ORT CPU 同模型同输入，五档案输出一致（容差内）
- [ ] E1.7 回退矩阵实测：openvino.dll 缺失/模型不支持/设备不可用 → 回退 ORT CPU 且可观测
- [ ] E1.8 验证记录 + fmt/clippy/workspace 回归 + 边界检查 + 本地提交
