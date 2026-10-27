# R4 Python 其他结构化结果复制（2026-10-27）

基线 `dd6c347`。在已有 Pose 复制基础上增加 `copy_detect`、`copy_classify`、`copy_segment`、`copy_obb`，均在 Result 锁内校验任务与非空数组，再复制为独立 tuple；旧裸 view 的借用义务不变。检测返回 `(bounds, confidence, class_id, name)`；分类返回 `(class_id, name, logit, probability, has_probability)`；分割返回 `(bounds, confidence, class_id, width, height, stride, mask_bytes, roi_map)`；旋转框返回 `(vertices_xy, center_xy, width, height, angle, confidence, class_id, angle_normalized)`。类别名称缺失为 None；保留 ABI 的坐标、概率、LSB-first 掩膜和角度语义，不做重解释。

防止语言层意外大量分配：检测/分割/旋转框不超过300项，分类不超过1000项；分割宽高1～8192、步长至少 `(width+7)//8`、非空掩膜指针且累计复制最多64 MiB。C ABI 没有掩膜实际缓冲长度字段，封装依赖匹配的可信 SDK 对声明长度正确负责，这不是对任意恶意原生指针的内存安全保证。

`scripts/sdk/test-result-structured.py` 的非空合成 ABI 数据检查四种字段、复制后更改源/释放/GC、重复关闭、错误类型、超量、空数组、分割坏步长/空指针/超预算及四种复制并发关闭等待；上阶段 Pose 复制及真实CPU空白图测试重跑通过。生成器、冻结、来源边界和 OpenSpec 严格检查通过。测试只证明合成结构与生命周期，不是四类真实模型推理、数值精度或实际非空结果对拍；C# 对应安全复制、五类模型验证和 R4 整体验收继续。
