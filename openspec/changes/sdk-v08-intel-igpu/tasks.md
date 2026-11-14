# Tasks

> 门禁等级：L3（公共 API 契约新增+跨模块）。
> 串行链：v0.7.2（已完成 04f0d66）→ 本 change → v0.8.x RTMP。

- [x] V8.0 侦察与设计（2026-11-15，侦察代理+PM 选型）：报告 .local/evidence/sdk-v08-20261115/v80-recon.md——①枚举策略实证：camera DXGI 序取首个非软件（selection.rs:165-209）、vision device_id 仅 CUDA≠0、DML 裸 build 不可控、OV 硬编码 "CPU"；**DXGI 序本身漂移**（今日 idx0=Arc/M3 时 idx0=NVIDIA）——纯核显机器当前能命中，缺陷精确定位="多卡机器默认跟 DXGI 序漂移"+无定向能力。②合同挂载点：am_backend_selection_t 尾追加（struct_size 先例）+能力位 bit12+camera 挂 decoder_config 尾追加。③引擎层零缺口（DirectML::with_device_id/OpenVINO::with_device_type 均在 rc.13）；openvino-native 无 GPU 插件（ort-ov-124-final 为候选）。④DML 运行库盘上确无（onnxruntime 均 非 DML 构建；System32 DirectML.dll 1.15.6 在）——渠道 PyPI onnxruntime-directml wheel（E2 验证 1.24.4）；**硬约束：DML 包替 CPU 线 ORT+与 CUDA 线同进程互斥**（部署形态=专用进程，INTEGRATION 指导）。⑤键稳定性：LUID 跨重启变/vendor+device 稳但 Arc 双实例歧义/索引最弱。**PM 选型**：定向键=适配器描述子串匹配+实例序号消歧（稳定且接入方友好）；默认策略维持现状（性能优先零变化）+定向=新能力（最兼容）。顺带债三处登记随 V8.3 修（device_utf8 DML 误写 CPU/DML deps 检查与合同不符/矩阵缺 backend=4）；freeze 基线应为 r13（r12 已被 v0.7.1 消费，tasks 原笔误更正）
- [x] V8.1 合同冻结（2026-11-15 凌晨，选型经协调线批）：主文件 **§3.5b 设备定向选择器**+types §3.1 `am_device_selector_t`（描述子串+实例序号；匹配域排除软件适配器；NULL/空=不定向默认零变化）+`am_backend_selection_t` 尾追加 device 指针（CUDA 候选=子串匹配 CUDA 设备名且 selector 优先于 device_id）+`am_decoder_config_t` 定向字段（reserved[4]→[3]+指针）+能力位 bit12+错误码零新增（invalidArgument/utf8Invalid/structSizeInvalid 复用）+定向与回退正交语义+DML/CUDA 同进程互斥部署约束+线程节全落
- [x] V8.2 生成链同步（2026-11-15 凌晨）：AM_ABI_VERSION 65542（1.6）+AMV_CAP_DEVICE_SELECTOR bit12（constants+头文件 #define+合同注释三处，Codex bit11 先例同款）+生成器全量同步（168 函数/3 头）+**freeze 基线升 r13**（id am-sdk-cabi-1.6-device-selector-r13/链指 history/freeze-v1.5-r12.json 归档）+zero-semantics 新结构五字段标注清零（struct_size=explicit 惯例）+constants.rs 指纹链式登记（previousSha256=manifest 原指纹基点，非链式中继——记忆规则命中）；四检绿+boundary 296+fmt/clippy 干净+workspace 528/0
- [ ] V8.3 实现：枚举默认策略修正+定向参数贯通（vision 引擎选择+camera 适配器）
  +DML 运行库落位
- [ ] V8.4 关键路径单测+e2e 门（定向命中/回退矩阵/纯核显形态）
- [ ] V8.5 纯核显端到端验收：Arc 140T 核显全链路（硬解+推理+多路）+性能实测
  如实登记（与独显口径分列）
- [ ] V8.6 文档链：INTEGRATION Intel 部署指导+CHANGELOG/README×2/RELEASE_*
- [ ] V8.7 发版收口：门禁全套+制品+版本 0.7.2→0.8.0+tag+推送+回执（Release 页
  等令）
