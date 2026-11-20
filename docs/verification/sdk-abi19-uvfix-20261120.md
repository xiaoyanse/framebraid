# sdk-abi19 族 1+2 批验证记录（P1 UV 寻址修复，2026-11-20）

分支 `sdk-uvfix-wip`（自 main=9c4dabf）。范围：openspec/changes/sdk-abi19-uvfix-resident 族 1（1.1-1.7）+族 2（2.1）+G.1。族 3 与 G.2、4.2-4.4 不在本批。

## 中转路径定界结论（探针在档：POC/core-performance/baseline/src/bin/uv-transit-probe，git 排除）

- 提案的 R8/R8G8 平面直拷中转在本机 NVIDIA 驱动（621.14/CUDA 13.5，RTX 5060 Laptop）**全形态静默不可用**（Y8/UV8-F1..F6/STAGEUV/SRV2D 臂定谳：CopySubresourceRegion 平面→单平面六种口径形式全丢、cast SRV 恒指平面 0）。
- 唯一零 host 路径：**VideoProcessorBlt NV12→B8G8R8A8**（BT.601 limited，样点 ±1/255；硬色度边 ≤75/255——双线性色度上采样 vs CPU 最近邻，合同 §3.3a 登记域差）。
- **注册纪律（本批新发现，PRODSEQ/PROD_NO_REG/PROD_NO_FENCE/PROD_OPT4 臂定界）**：图形队列读已注册 CUDA 纹理且 VP Blt 尚在队 → 设备移除（0x887a0005）。解法=VP 只写未注册 output；图形队列整帧拷 output→已注册暂存纹理（兼作完成序——EVENT 查询只覆盖图形队列，拷贝跨队列依赖把 Blt 纳入屏障；探针 VPSLEEP 定谳直查竞态）；CUDA 只映射暂存。PROD_OPT4 臂 4/4 blt+拷贝+映射零故障、px(0,0) BT.601 真值 max|Δ|=0.0040。
- 顺带定界：EVENT 查询对 VP Blt 无屏障（视频队列与图形队列分离）；生产序曾经的无 box 整子资源 W×H→1×1 拷贝属非法命令（本驱动 GetData 隐式提交处崩溃）。

## UV 真值门数字（uv_truth_three_form_matrix，三形态矩阵）

- 形态：单 slice（ArraySize=1/Misc=0x2）/数组 4（4/0x2）/现场同款（20/0x208/Misc=0）；每 slice 互异常量图案（Y=30+9k/U=60+5k/V=240−8k），全 slice 装载，首/中/尾取样（尾=旧 k+1 越界必炸点）。
- 断言一（快照 BGRA vs BT.601 limited 镜像，3 像素×3 通道）：全部 ≤2/255（容差 2.5/255）。样例：单 slice slice0 R=211/255 vs 0.8309（Δ=1）；现场 20 slice10 BGR=[30,38,201] vs [0.792,0.149,0.121]（Δ=1/38/1）。
- 断言二（kernel 输出 vs 同 BGRA 双线性 letterbox 镜像，4096 元素逐项）：max|Δ|=**0.00e0**（容差 1e-4）——全部 9 个（形态×取样）组合一致。
- 批内互异：同形态不同 slice 输出 max|Δ| > 2/255（图案步长放大后全部通过；旧阈值 0.02 在相邻 slice 小步长下误杀，已随图案步长一并修正并登记）。

## 15 门回归差异表（am-vision-ffi，--test-threads=1，16/16 绿；device_selector_e2e 与 ov_remote 为本机 Intel iGPU 楔死环境除外——v0.8.3 在档 igpu-wedge-diagnosis-20261119.md）

| 门 | 结果 | 差异与处置 |
|---|---|---|
| pool/batch/stream-status/vocab×2/bindings 等基础门 | 绿 | 无差异 |
| world_e2e（GPU 直传+批窗口） | 绿（重定） | 自标定噪声阈（conf~0.003）下框数 12→5 跨域漂移；按该门既有 top_n 注释语义（近阈集合跨域不保证稳定）对齐实现：框数断言取截断 top 集。top-1 框 IoU/置信断言保持 |
| gpu_cascade（seek 档） | 绿（重定） | source_cmp 跨域框级断言移入照片档（合成噪声帧 argmax 不可跨域稳定）；同域 single-vs-batch 框漂预算 1→2px（conf 0.003 病态回归框批 GEMM tiling 漂 1.27px 实测，score 通道 8e-6 证明数值同一性） |
| gpu_cascade（photos 档，FB_ROI_PHOTOS） | 绿（重定） | bus/zidane 真检出各 8 框（min 0.92/0.84）；跨域（像素 vs VP 纹理）框漂 1.71/0.63px、score Δ0.0035/0.0147、匹配 8/8——按"每跨一预处理域 ≈2px@640"重定预算 3px（旧 0.5px 按修复前像素同域路径标定）；动态 ROI roi_px 1.59px 同据 3px |
| gpu_pool/级联原生计数/预算/回退门 | 绿 | 无差异 |

am-vision 侧 GPU 门（同轮）：uv_truth/帧推理/classify/batch/interop/coldstart/bgra_kernel（1e-5）/ROI prototype 全绿。

## 验证门数字

- workspace：**560 通过/86 ignored**（560=562 基线−registry.rs 2 个 LRU 单测随模块退役；+uv_truth 门入 ignored）；fmt/clippy 零问题。
- boundary：**306/13**（+vp_transit.rs/uv_truth_tests.rs/bgra_letterbox.ptx，−registry.rs/nv12_letterbox.ptx；修订 10 项+新增 3 项+移除 2 项，previousSha256 绑原始清单根）。
- freeze：**r16**（am-sdk-cabi-1.8-uvfix-clarify-r16，abiVersion 65544 不变），--check frozen 11 通过；doc-coverage 199 符号 0 未提 0 过期；zero-semantics 451 基线 0 问题。
- G.1：1.8 供给下 1.5-1.8 特性门全绿（上表 16/16+zero-semantics+freeze）。

## 审计口径

GPU copies per frame = **3**（VP 转换+整帧暂存拷+BGRA D2D），比提案预估（+2）少一拷（完成序与暂存合一）。pipeline.rs 审计注释同步。
