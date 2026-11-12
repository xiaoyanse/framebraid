# Tasks

> 门禁等级：L1-L2（顺序修复 L1 改动 L2 验证要件：定向测试+GPU 门回归）。

- [x] B2.1 establish_context 前置（pipeline.rs L60/L61 对调+2 行归因注释）+错误路径复核（三段错误串逐字不变；唯一可观察差异=冷线程伪 JIT 失败消除、两步同败时先冒真实上下文错误）；diff +19-1（含 B2.2 测试）
- [x] B2.2 冷启动定向测试：pipeline.rs 内联 coldstart_tests（直调私有 new() 避免 OnceLock 污染）；RED→GREEN 双证（临时还原旧序复现探针原文 JIT 失败(3)→修复后绿）；PM 复跑 cold_thread_first_touch 1/1 过
- [x] B2.3 GPU 门回归：m2b 四门（kernel 对拍/互操作/帧推理 e2e）+批 C 组装门+B1.5 双 Model 门全 PASS；am-vision 112/0（PM 复跑同数）；fmt/clippy 零新增；boundary 287 登记（pipeline.rs）。**随回归暴露独立预存问题**：vocab/world 真实权重 e2e 门族 4 门红（A/B 双证非 B2.1 引入——旧序同点同码失败+world CPU 臂零 GPU 同红；批 D 时代绿，疑 ORT 1.26→1.31/驱动组合漂移；失败点 amv_infer_image_sync 本体 rc=28 无 detail）→登记归因项上报
- [x] B2.4 批 3 等槽归因（2026-11-13 早）：**错峰交互假设证伪**——对照臂 6 路批 3 单 Model（shards=1，busy 互斥下槽必空、无跨 Model 等槽）=**19.4 fps ≈ 双 Model 错峰 20.1（差 3.6% 噪声级）**，批 3 低吞吐与错峰无关；批 3 形态固有供给塌陷（N=3 组合执行链——周期 127ms/批 vs 批 4 的 66ms，凑满零截止，疑 cuDNN N3 kernel 选择次优，同 R1.4"N=8 次优"同类现象）。结论=登记形态边界（批 3 不推荐的部署建议维持），深挖 N3 kernel 无产品必要；JSON b2-n6w40-b3-single.json。顺带：错峰前 35.0 为 75% 拒绝形态扭曲数字，批 3 干净供给（19-20）首次实测
