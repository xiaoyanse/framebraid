# Proposal: sdk-abi19-uvfix-resident（UV 寻址修复 P1+subresource 语义澄清+驻留形态——ABI 1.8→1.9 三族一版）

## Why

1. **P1 正确性缺陷**（排障定谳 .local/tasks/nv12-array-interop-experiment-20261120.md）：CUDA 对 NV12 纹理子资源映射=扁平 slice 口径且**仅暴露 luma 平面**（UV 无直寻址路径，runtime+driver 双 API 定谳）；SDK 现行"Y=idx k、UV=idx k+1"双映射是历史误推（测试载体 ArraySize=4 同图案、两种口径不可区分）——三种失效：数组尾 slice 越界（AM-RT 现场阻塞）/ArraySize=1 越界/其余 **UV 实采下一 slice luma=伪色彩**（v0.3.0 起 CUDA 直传线颜色语义缺陷；检测类亮度主导影响低故既有门全绿掩盖；Intel OV 线不受影响）。
2. **合同歧义**：am_gpu_frame_ref_t.subresource 语义未定义（消费方无换算口径——AM-RT 现场即踩）。
3. **驻留形态需求**（消费方热路径）：8 路直传 lane 每帧输出回读为下一档吞吐瓶颈（侦察 sdk-resident-output-recon-20261120.md 四问全链+PM 四裁决）。

排期：协调线 2026-11-20 批准（P1 立即开工+全速不跳门 v0.8.4+ABI 1.8 向后兼容实证加门）。

## What Changes

### 族 1：UV 寻址修复（P1，先行）

- **实现**：preprocess_to_binding 重构——单次映射拿 luma array（idx=slice）+**双纹理中转**（CopySubresourceRegion：NV12 Y plane→R8 纹理、UV plane→R8G8 纹理，两标准格式 CUDA 完全支持映射）+kernel 从双 array 重建色彩（nv12_letterbox.ptx 采样源改双纹理）。D3D11 device 经纹理 GetDevice（COM，ov_remote 基建复用）；中转纹理池化（按尺寸缓存）。
- **测试载体修正**（P1 教训落地）：slice 各异图案（杜绝同图案掩盖）+尾 slice 必测+**UV 通道真值断言**（颜色级比对，不再只检检测身份）；数组/单 slice/现场形态（ArraySize=20 desc 同款）三形态矩阵。
- **审计**：copies_per_frame 值如实涨（中转 +2），合同语义不变。

### 族 2：subresource 语义澄清（合同，与族 1 同批）

- §2.6a+头文件注释：subresource=**扁平 slice 口径**（0..ArraySize-1，NV12 平面含于 slice 内由 SDK 内部处理；消费方 ffmpeg 形态传 AVFrame.data[1] 原值即正确）。
- 头文件注释变更=冻结工件单文件归因重算（先例在）。

### 族 3：驻留形态（输出侧，族 1 落地后）

- **PM 四裁决**（侦察报告待裁决点回应）：①驻留随 result 扩展（设备驻留 result：raw_outputs 留设备+标志位+readout 新导出 amv_result_device_readout——不发明新句柄类型，B08"无孤立 tensor retain"保持）②独立 device 显存账本（对齐相机 GPU 帧池先例，不挤 CPU 路径 64MiB 包络）③GPU 筛选走描述结构（struct 尾部追加，声明式无回调）④AM-RT 预算口径=宿主 1 MiB 常量已对齐。
- GPU 帧导出输入面（amc_frame_encode 纹理输入）按侦察四路径选项设计定稿（合同 L566 既有预告"NV12 硬编后续+GPU 帧直通同批"）。
- ABI 演进：1.8→1.9 兼容新增（新导出+结构尾部+能力位若需）。

### 加门（协调线）：ABI 1.8 向后兼容实证

1.9 供给下 1.8 消费路径实测不变——1.8 语义既有门全跑+AM-RT 形态 e2e 对拍（其 slice 口径观测值做修复对拍锚），不做纸面承诺。

## Impact

- 门禁 L3（公共 API 契约+跨模块+正确性修复）。触碰：gpu/{pipeline.rs,interop.rs,nv12_letterbox.ptx,新中转模块}、vision-ffi input 分派、合同文档+生成链（1.9）、iobinding/model 输出侧（族 3）、am-camera encode（族 3 导出）、测试全套、boundary。
- 发版 v0.8.4：全门禁绿+兼容实证过+发布说明含诚实缺陷披露（v0.3.0 起色彩语义缺陷+影响边界+检测类低影响依据）。
- 下游：AM-RT 0.6.0（ABI 1.8 上产）——1.9 兼容新增零变化（实证门背书）；vl pin 不受影响。
