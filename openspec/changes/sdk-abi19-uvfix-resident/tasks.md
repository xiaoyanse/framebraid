# Tasks: sdk-abi19-uvfix-resident

> 本 change 等级：L3，门禁要件：关键路径单测+关键集成点测试（真装配）+e2e 真机+ABI/生成检查+boundary+**ABI 1.8 向后兼容实证门（协调线加门）**+文档全同步。族 1（P1 修复）优先排序。

## 族 1：UV 寻址修复（P1）

- [x] 1.1 合同：§3.3a 修复说明（UV 中转行为+copies 口径）+§2.6a subresource 语义澄清（slice 口径+ffmpeg 换算说明）+头文件注释（冻结工件归因重算 r15→r16，abiVersion 65544 不变）
- [x] 1.2 中转模块：vp_transit.rs TransitPool（设备×尺寸键控 8 条 LRU+条目锁）。**实证改道**：提案的 R8/R8G8 平面直拷在本机驱动（621.14）全形态静默不可用（探针 Y8/UV8-F1..F6/STAGEUV 定谳），唯一零 host 路径=D3D11 VideoProcessor NV12→B8G8R8A8（BT.601 limited+色度上采样在 VP 内完成）。注册纪律（PRODSEQ/PROD_OPT4 定界）：VP 目标不注册 CUDA——图形队列读已注册纹理且 VP Blt 在队时驱动设备移除（0x887a0005）；注册面=独立整帧暂存纹理，图形队列整帧拷既当 blt 完成序（EVENT 查询只覆盖图形队列）又当映射源
- [x] 1.3 映射链重构：单次 map（idx=0，BGRA 单 slice）弃 k/k+1 双映射；旧 registry.rs（NV12 直映射缓存）与"M2b 实测 UV 紧随 Y"误推随路径删除（孤儿清理，其 2 个 LRU 单测随模块退役）
- [x] 1.4 kernel 改写：bgra_letterbox.ptx（手写 PTX）——采样源改 VP 产出的 BGRA 单 buffer（u32 tap+掩码移位取通道），RGB 域双线性与 CPU 像素路径同域（旧 YUV 域重采样+量化对拍类差消除）；letterbox/center-crop/ROI 几何逐字保留；nv12_letterbox.ptx 随替换删除
- [x] 1.5 测试载体修正+新门：uv_truth_three_form_matrix——slice 各异常量图案（Y±9/U±5/V±8 步长）+两层断言（快照 BGRA vs BT.601 镜像 ±2.5/255；kernel 输出 vs 同 BGRA 双线性镜像 1e-4，实测 0.00e0）+三形态矩阵（单 slice/数组 4/现场 ArraySize=20 同款 desc 0x208/Misc=0）+首中尾取样+批内互异；装载辅助 upload_nv12_slice（D3D staging 整 slice，P1 后 CUDA 不可写 NV12 UV 平面）
- [x] 1.6 既有门回归：am-vision GPU 门全绿（uv_truth/帧推理/classify/batch/interop/coldstart/kernel/ROI prototype）；am-vision-ffi 16/16 绿（seek 档；device_selector 与 ov_remote 为本机 Intel iGPU 楔死环境除外，v0.8.3 在档）+photos 档（bus/zidane 真检出 8/8，min 0.92/0.84）。**阈值重定（依据登记）**：跨预处理域框漂预算按"每跨一域 ≈2px@640"实测重定（cascade source_cmp 0.5→3px、动态 ROI 0.5→3px、seek 档批漂 1→2px、world 跨域框数断言按既有 top_n 注释语义对齐截断集）——VP 双线性色度 vs CPU 最近邻为合同登记域差（§3.3a），噪声底 argmax/病态回归框不可跨域稳定；颜色正确性由 uv_truth 逐 slice 真值覆盖
- [x] 1.7 审计口径：copies_per_frame=3（VP 转换+整帧暂存拷+BGRA D2D）——比提案预估（+2）少一拷：完成序与暂存合一；pipeline.rs 审计注释同步

## 族 2：subresource 语义（并入族 1 合同步）

- [x] 2.1 §2.6a+头文件注释（am_gpu_frame_desc_t/am_gpu_frame_ref_t subresource 扁平 slice 口径+ffmpeg data[1] 直传说明）+INTEGRATION CUDA 知悉项"subresource 口径"条目（与 9e8252a 前置变量条目同节）

## 加门：ABI 1.8 向后兼容实证

- [x] G.1 1.8 供给下 1.5-1.8 特性门全跑绿（bit10 互操作/级联 gpu_cascade/池 pool/stream status/batch/vocab/world/zero-semantics 451 基线）——freeze r16（am-sdk-cabi-1.8-uvfix-clarify-r16，abiVersion 65544 不变）+freeze --check frozen 11 通过
- [ ] G.2 AM-RT 形态 e2e 对拍（slice 口径观测值做锚——真机或 AM-RT 配合复验）

## 族 3：驻留形态（族 1 落地后）

- [ ] 3.1 合同设计定稿：设备驻留 result（raw_outputs 设备侧+标志位）+amv_result_device_readout 导出+独立 device 显存账本+筛选描述结构——四裁决落合同（§3.3a 输出段+§5.1 所有权+新节）
- [ ] 3.2 生成链：ABI 1.9+四生成物+freeze r16
- [ ] 3.3 实现输出侧：iobinding 设备输出绑定（CUDA MemoryInfo 先例 L67-73）+extract_outputs 驻留分支+readout API+device 账本
- [ ] 3.4 GPU 筛选：描述结构+零目标帧只回读计数张量
- [ ] 3.5 GPU 帧导出输入面：amc_frame_encode 纹理输入（四路径选项按侦察报告设计定稿实施）
- [ ] 3.6 测试：readout 语义单测+e2e+筛选门+导出真值门

## 收尾

- [x] 4.1 workspace 全测（560 通过=562 基线−registry 2 单测随模块退役；86 ignored=+uv_truth 门）+fmt+clippy+boundary（306/13，+vp_transit/uv_truth/bgra_letterbox −registry/nv12_letterbox）+doc-coverage（199 符号 0 未提）+freeze --check 全绿
- [ ] 4.2 文档六件套+MIGRATION ABI 1.8→1.9 段（含缺陷披露引用）
- [ ] 4.3 CHANGELOG：Unreleased 记录（P1 修复+澄清+驻留）→v0.8.4 定版含诚实缺陷披露
- [ ] 4.4 v0.8.4 发版（全速不跳门：全门禁+兼容实证+披露说明）
