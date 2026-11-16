# sdk-prepopt CPU prepare 位级等价实现形态优化验证

2026-11-16，分支`sdk-prepopt-wip`（基线 main@`bb96193`），演化模式第四波实施批，SDK main 串行，不改 AM-RT、不发布。依据侦察批裁决（MVO=a+b 路径；c=u8 域、d=多线程按 R2 1.4 否决先例与收益存疑不做）。内部优化：公开接口、行为、依赖、支持范围零变化，不升 ABI。

## 实现与位级等价论证

`crates/vision/src/sdk/prepare.rs` 单文件热循环重排，每个输出值的公式与运算顺序逐字保留，仅求值位置提升：

- **a1 行/列采样常数预计算**：`xx`只依赖 x、`yy`只依赖 y——逐像素的除法/clamp/浮转整串行链（旧 :127-131）改为列常数 `ColumnTap{fx,ix,x1,fill}` 一次算完、行常数进入行循环头部一次算完；运算与冻结参考逐序相同。
- **a2 零权重快速路**：`fx=fy=0`（整像素对齐采样，如 scale=1 letterbox 恒等几何）时双线性按位退化为单像素直采。证明（注释在代码）：rgb 分量均为有限非负值（格式 0/1/2 为 u8 直转；格式 3 为 clamp 到 [0,255] 的和式，舍入到最近下 x+(-x)=+0、luma 不产生 -0.0），故 `b*0=+0`、`a*1=a`、`a+(+0)=a`、`(a)*1=a`、`+(c*1+d*0)*0=+(+0)`，全程逐位等于 a。跳过 4 tap 与 3 次混合。
- **a3 rgb 单次化**：快速路天然 1 次 rgb（4 tap 共享同一变换基的极限情形）；通用路径 tap 级缓存评估结论——行级 rgb 缓存对本批两几何（恒等/3× 下采样）零复用（下采样每输出行源行不重复、源列步进 3 不重叠），仅保留**边界 clamp 去重**（`x1==ix` 或 `y1==iy` 时 tap 与相邻 tap 同坐标，纯函数同值复用，逐位相同），不引入任何近似。
- **b 行缓冲两段写出**：采样行先写平面 raw 行缓冲（3×width f32），第二段通道外连续归一化（同式同序，bgr 交换取基址；双缓冲防跨平面串道），再按输出布局整行连续写出——NCHW 每通道 `copy_from_slice`、NHWC 交织一次；消除逐像素跨步写（stride=area×4B）。letterbox 填充带（整行）以预计算归一化常值直接按平面段填充，跳过采样与归一化循环。

签名/语义/错误键/预算校验/8 字节对齐/TensorData 拥有路径全部不动；`pixels.rs`、调度器零触碰。

## 位级等价证明链（验收门 1/2/4）

- **972 组合冻结 oracle 字节对拍**（prepare/tests.rs 冻结 reference.rs 副本，防同实现自证）：RGBA/RGB/BGR、NV12 三矩阵×两范围、行填充、三类 resize/normalize、NHWC/NCHW、RGB/BGR、三种目标尺寸（含 1×1/奇数）——张量字节、shape/name、变换浮点位、8 字节对齐全一致，`cargo test -p am-vision --lib sdk::prepare` 5 项全绿（实现全程 7 轮复跑均绿）。
- **perf_prepare 离线 A/B**（门 2，R2 1.3 同款）：1920×1080 RGBA→640 letterbox/NCHW/f32，200 样本/20 热身——FNV `ddf89c9aea3b3f3b` 旧新相同，且与 R2 1.3 定档记录值一致；Rust 分配 4→7、请求字节 4,915,254→4,940,854（差 +25,600B = 列常数 10,240 + 行缓冲双份 15,360，精确对账，约为输出张量 0.5%）。
- **真机 Pose 全链 SHA**（门 4，R2 1.3 先例口径 `sdk-r2-prepare-direct-20261026.md:40`）：冻结模型 `yolo26n-pose.onnx`（SHA `80fb5f58…`）+冻结 PNG（`c0309f14…`），backend0 strict 子进程观测——旧 DLL 先复现冻结值自证 harness，新 DLL 原始输出 SHA `1463641ea7696dfaa5eaa41087b1d804831fb1fe8ffa16225a0797f957c8e909` 23/23 样本（warmup+3 轮+observation_off）全部一致。

## 量化（验收门 3，release 同二进制 A/B）

计时矩阵：POC 既有 bin `realmodel-split`（prepare/run/decode 分段，sdk-bench 同款）增输入几何参数（`--input-size/--photos/--photos-dir`，默认行为与侦察批逐字节同语义；POC 排除区零 tracked 改动），旧 exe 先行构建（prepare=main 语义）再改源构建新 exe，A/B 同 harness。cpu 后端、single 形态、warmup 20、3 轮×20s 定时窗、轮中位；输入 bus/zidane NV12（BT.709 有限范围）双几何：640²（侦察批同款）、1920×1080（ffmpeg 上采样生成夹具 `bus-1080p.nv12`/`zidane-1080p.nv12`，确定性输入非现场分布）。检测数身份断言旧新同值（640² bus=5/zidane=3、1080p bus=5/zidane=2，top conf 四位一致）。

| 几何（NV12→640 letterbox/NCHW） | 旧 prepare p50（3 轮） | 新 prepare p50（3 轮） | 加速 |
| --- | --- | --- | --- |
| 640×640 | 13.298/13.286/13.272 ms | 3.906/3.883/3.846 ms | **3.42×**（中位比） |
| 1920×1080 | 8.080/8.074/8.086 ms | 2.654/2.657/2.662 ms | **3.04×** |
| RGBA 1920×1080（perf_prepare 口径，p50 200 样本） | 4.859 ms | 1.801 ms | 2.70× |

旧 640² 基线 13.286ms 与 sdk-bench 侦察批 13.39/13.45ms 吻合（同口径复现）。1080p 新值含 280/640 行填充带直写收益。

## 分段探针归因（侦察未决 4，验证并修正静态归因）

`#[ignore]` 探针（prepare/tests.rs，release `--nocapture` 手动跑）：镜像旧循环相位（坐标链/坐标+rgb×4+混合/全段）与新结构相位（列常数/采样到 raw/全段）。release 进程内中位：

| 口径 | 坐标 | rgb+混合 | 归一化+写出 | 合计 |
| --- | --- | --- | --- | --- |
| 旧 640² | 0.706ms（~4.5%） | 8.632ms（~55%） | 6.447ms（~41%） | 15.786ms |
| 旧 1080p | 0.650ms | 4.959ms | 4.376ms | 9.985ms |
| 新 640² | 列常数 0.001ms | 采样 3.089ms | 归一化+终写 1.205ms | 4.294ms |

**归因修正**：侦察静态归因三因（rgb×4/坐标链/逐像素分支）方向正确但权重失准——坐标链仅 ~4.5%（高估），归一化+跨步写 ~41%（低估，静态归因未单列）；重排后该两项被压至 1.2ms 量级。**剩余地板**：新 640² 结构中逐像素单次 NV12 `rgb()` 标量转换占残余 ~72%（3.089/4.294）——位级等价下不可再降（rgb 语义冻结于 pixels.rs；u8 域撞 R2 1.4 否决先例与 Pose SHA 冻结链；SIMD/多线程出批范围）。**640² 实测 3.42× 低于任务书探索预期 5-9×，即此地板所致，如实报告不虚标**；1.1ms 朴素对照系 OV 预处理捷径（非本仓位级等价语义），不可作为位级等价地板引用。

## 门禁与债登记

- workspace 全测 548 通过/78 ignore（真机门维持既有忽略清单）；`cargo fmt --all --check` 干净；clippy am-vision 全 targets/features 零告警。**预存债登记不碰**：`crates/vision-ffi/src/inference/world_e2e_tests.rs:618` dead-assignment 为 main@bb96193 既有（sdk-q1 批 `2a5939b` 引入，文件与 HEAD 逐字节一致，非本批新增，外科手术不越界修）。
- check-boundary 297 文件通过/零业务依赖/零仓外路径——`sdk-source-revisions.json` 按仓规更新 prepare.rs 与 prepare/tests.rs 两条目（previous=提取原指纹链保持，reason 追加本批），原始 `sdk-source-manifest.json` 未动。
- ABI 冻结 11 工件零漂移（freeze-abi --check）；bindings 覆盖 0 问题；无公开接口变化故无生成物同步义务。
- 文档同步核对：CHANGELOG Unreleased 双语条目（实测数值）、RELEASE_STATUS 当前源码验证新节、ROADMAP 进度条目（并回填上条 ovproto 登记的候选方向实施结果）；**README/快速接入/INTEGRATION/SUPPORT 不适用**——内部优化，无接入面/命令/支持范围变化；本记录为验证状态真相源。

## 工具合规

- 一律 Edit/Write 工具改代码与文档；数据检查 Read/Grep/node 只读。bin 夹具生成用系统 ffmpeg（数据文件，非代码）；`python -c` 一处被权限拒绝后改用 node 只读解析，未再使用。
- 旧基线二进制先于源码改动构建（perf_prepare-old/realmodel-split-old/am_vision-old，SHA 见 manifest），A/B 同 harness；构建输出均落本仓 `.local/build` 或 POC 自锁 target，无仓外散落。
- 分支 `sdk-prepopt-wip` 零提交零推送（PM 验收后统一提交）；未 stash/checkout 任何文件；无现场设备/凭据/模型入库。

## 复现

```
cargo test -p am-vision --lib sdk::prepare          # 门1 oracle
cargo test --release -p am-vision --lib segment_probe -- --ignored --nocapture  # 分段探针
cargo build --release --example perf_prepare && \
  ./target-dir/release/examples/perf_prepare.exe 200 20              # 门2（A/B 两份 exe）
cd POC/core-performance/baseline && CARGO_TARGET_DIR=./target \
  CARGO_BUILD_JOBS=1 CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
  cargo build --release --bin realmodel-split                        # 门3（A/B 两份 exe）
python scripts/sdk/benchmark-inference-resources.py --child --backend 0 --strict \
  --dll-dir <old|new dll 目录> --runtime-dir .local/runtimes/ort-dml-124 \
  --model .local/tests/met2-s3/yolo26n-pose.onnx --input .local/tests/met2-s3/input.png --samples 5  # 门4
```

证据`.local/evidence/sdk-prepopt-20261116/`（raw 计时 JSON×4+冒烟×2、perf A/B、pose 旧新、探针输出、manifest）；A/B 用二进制与冻结模型/照片夹具在`.local/tests/prepopt-20261116/`（manifest 记 SHA）。1080p 夹具为 ffmpeg 上采样生成（bus/zidane 1080p NV12），非现场分布。探针窗 3×20s（侦察 3×60s，排序与比值稳定，绝对值按口径纪律不外推）。
