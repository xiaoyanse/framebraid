# Tasks

- [x] E1.1 合同：dual-dll-abi.md 推理后端章节扩展（openvino-native 候选、AM_BACKEND 新值、运行库目录约定、回退矩阵、能力位）+ OpenVINO 运行库指纹入 materials 体系
- [x] E1.2 生成器同步（枚举新值）+ 三语言绑定 --check 绿
- [x] E1.3 OpenVINO C API 最小 FFI（28符号对照vcpkg头文件逐个核签；openvino_c.dll显式加载+同目录依赖解析）
- [x] E1.4 多引擎抽象：ModelEngine枚举（ORT分支零逻辑改动）+ selection候选扩展与回退（三档实测：目录缺失/环境未设/strict）
- [x] E1.5 档案适配：NCHW字节直传+输出走现有decode链（3合成单测；五档案真实对拍以pose为代表，其余档案无本地权重沿用既有未验标注）
- [x] E1.6 真实对拍：序不敏感口径通过（matched max_rel=3.15e-4；逐元素口径对该end2end模型不可达——内置top-300排序不稳定，PM裁决依仓内先例；统计量≤1e-4）
- [x] E1.7 回退矩阵实测：三档全过（目录缺失217μs回退+真实推理/环境未设27μs回退/strict报错不回退，selection报告可观测）
- [x] E1.8 验证记录（.local/evidence/sdk-e1-closeout-20261029.md）+ workspace 393全绿 + 提交链
