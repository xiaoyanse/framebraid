# Performance metrics directory / 性能指标目录

FrameBraid SDK 的性能指标体系单一定义。每项验证记录从此处引用指标名与量法；新验收必须覆盖适用路径的主指标与保护指标。基线值随版本更新，跨版本对比以此为准。

## 指标定义

### 速度类

| 指标 | 量法 | 适用路径 | 当前基线 |
| --- | --- | --- | --- |
| `infer.p50/p95/p99` | 同步单帧推理，perf_counter 逐次 | CPU/CUDA/OpenVINO/DirectML | CPU 97~116ms / CUDA 26.7~28.6ms（Pose，v0.2） |
| `convert.time` | 逐帧转换耗时（如 NV12） | 像素转换 | 1080p 6.8ms（RG 表后） |
| `sampling.effective_fps` | 帧龄预算内交付帧率 | 订阅 | 29.5/30（30fps 输入） |
| `session.cold_start` | 模型加载→首会话可用 | 各推理后端 | CPU 0.18s / CUDA 0.5~1.4s |
| **`startup.first_result`**（新） | SDK init→首个推理结果交付总时延 | 全链 | 待 E2/E1 建立 |

### 交付质量类

| 指标 | 量法 | 适用路径 | 当前基线 |
| --- | --- | --- | --- |
| `delivery.latency` | submit→receive | 解码/订阅 | P95 137~155ms（单路） |
| `recovery.resync` | 断流→恢复首帧时间 | RTSP | 关键帧重同步精确 |
| `capacity.lanes` | 合格路数（全部帧龄达标） | 多路解码 | 4/4 @1080p30 |
| **`latency.breakdown`**（新） | 采集→解码→前处理→推理→交付各段毫秒 | 全链 | 待 M2b/M2c 建立 |
| **`jitter.ratio`**（新） | P99÷P50（>3 列抖动告警） | 全部延迟类 | 待 M2c 起用 |

### 资源类

| 指标 | 量法 | 适用路径 | 当前基线 |
| --- | --- | --- | --- |
| `cpu.process_time` | 窗口进程 CPU-s | 全部 | 双路 CPU 1.6~1.7 CPU-s/6s 窗 |
| `mem.rss` / `mem.vram` | 采样峰/走势 | 全部 | 有基线（各记录） |
| `pool.borrow_us` | 共享池借出/归还微秒 | 共享帧 | 22/25µs P50 |
| **`transfer.copies_per_frame`**（新，用户吸纳） | 每帧拷贝次数+总字节+来源分解 | 全链 | 待 M2c（零回读前后对比主指标） |
| **`scaling.incremental_cost`**（新） | 每加一路的 CPU/带宽/延迟增量 | 多路 | 待 M3 建立 |

### 正确性联动类

| 指标 | 量法 | 适用路径 |
| --- | --- | --- |
| `parity.tolerance` | 双后端输出对拍（有效行/置信度/坐标差） | GPU vs CPU |
| `backpressure.reject_rate` | 队列满/预算不足拒绝率 | 全部 |
| `notify.drop_rate` | 通知丢弃率（256 上限） | Python/C# |

## 新指标挂钩排期（不新增专项，搭既有里程碑）

- `transfer.copies_per_frame` → M2c 验收硬指标
- `latency.breakdown` / `jitter.ratio` → M2b/M2c 验收建立（零回读前后对比必需）
- `startup.first_result` → E2/E1 每后端实测时建立
- `scaling.incremental_cost` → M3 回退矩阵验收建立

## 使用规则

- 验证记录引用指标名，不临时造口径；跨版本对比查基线列
- 历史记录不改写；新指标首次建立时在对应里程碑记录注明"首次建立"
- MET-2（PERFORMANCE_ACCEPTANCE.md）定义验收方法论，本目录定义指标本身
