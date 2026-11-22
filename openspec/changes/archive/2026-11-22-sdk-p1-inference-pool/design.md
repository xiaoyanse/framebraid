# Design: SDK P1 — 推理池（分片编排容器）

> P1.1 冻结稿。API 面最小化裁决如下；合同章节按此起草。

## 1. API 面最小化裁决（B 案：编排容器，提交路径零新增）

**取舍**：A 案=独立 pool 提交族（pool_submit_*，语义强但 API 面大、
新 FFI 路径）；**B 案（裁决）=池只做编排容器**——

- `amv_pool_create(ctx, &desc, &out_pool)`：desc 含模型源/shards k/
  攒批配置/lane 数；内部 load k 个 Model 实例+逐片 configure_batching+
  预建 lane→shard 连续分组映射（lane i → shard ⌊i×k/lanes⌋，实测形态）
- `amv_pool_lane_model(pool, lane, &out_model)`：返回该 lane 绑定的分片
  Model **借用句柄**（所有权归池，生命周期随池；句柄即现有
  amv_model_t，可直接用于现有 amv_infer_image_sync/async/GPU 帧入口/
  get_batching/close 语义全部继承）
- `amv_pool_close(pool, timeout_ms, &out_state)`：逐片排空关闭（复用
  model close 语义，顺序无关）

**裁掉**（YAGNI）：pool 级 submit 族（提交路径零新增——现有 infer 家
族原样用，全部既有测试/绑定/语义免费继承）；pool 级聚合 stats（逐片
get_batching 已有，调用方持 lane 句柄可自查）；自动通道探测（分通道
批形状 desc 显式传，文档指导值）。

**收益论证**：API 面 3 个导出（ABI 次版本演进最小）；接入方从"建 k
个 Model+配置+分组"变为一处 desc；B1 错峰/GPU 直传旁路语义内建继承
零改动（池内 Model 即普通 Model）。

## 2. 合同要点（章节起草输入）

- **desc 结构**：`am_pool_desc_t`（struct_size 显式；model 源复用
  am_model_desc_t 形态或嵌引用；shards 2..=8（SDK_MAX_CONTEXTS 内语义
  核算）；lanes ≥ shards 且整除校验；batching 配置段复用
  am_batching_options_t）
- **错误码**：参数域沿用 sdk.invalidArgument/limitOutOfRange；shards
  超 ctx 槽位=resourceExhausted（max_models 语义）；lane 越界=
  sdk.invalidArgument
- **生命周期**：池 Model 句柄为借用——**池存活期内**可提交/查询；
  pool_close 后句柄失效（与 model close 后语义一致）；lane_model 借用
  不增加引用计数（文档写明"句柄在池 close 后禁用"）
- **线程**：lane_model 线程安全只读查询；create/close 与现有
  model_load/close 线程语义一致
- **ABI**：AM_ABI_VERSION 次版本 +1（65539→65540）；三导出走生成链

## 3. 验证面（对 tasks P1.4/5 的输入）

- 单测：desc 校验域（shards/lanes 整除/越界）、lane 映射边界
  （首/末/越界）、close 后句柄失效
- e2e 真装配：池创建→8 路 lane_model 取句柄→GPU 直传提交→逐片
  身份（请求 id 域分片可辨）→攒批窗口活跃（双片 formed 增长）→
  pool_close 排空守恒；CPU 像素批 2 臂同
