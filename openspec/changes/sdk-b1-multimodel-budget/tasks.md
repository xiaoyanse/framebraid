# Tasks

> 门禁等级：L2（业务功能类）。要件：关键路径单测+关键集成点真装配测试+代码取证。
> 红线：护栏弱化即停（见 proposal 护栏承诺段）。

- [x] B1.1 设计冻结：执行份额口径取证定稿（CPU 像素 24B/px 属实无收敛空间；**GPU 直传 exec=0 双 Model 批 4 现行已解锁**——挤兑为 CPU 像素通道现象，实验走 pixel 通道实证 multilane-scheduler.rs:477；错峰后 CPU 批 4 算术 61.1MiB<64 自核通过）+ design.md 回填（2026-11-12）
- [x] B1.2 执行错峰调度实现：ctx 级执行槽（执行期份额互斥）；**受益面=CPU 像素通道**（GPU 直传 exec=0 无需错峰，exec_stage.rs:54-56 全零批旁路）；单 Model 路径行为零变化论证过（四路径：未攒批跳过/单 Model 无竞争首轮获槽/错误路径等价/既有测试零改动全绿）；持槽窗口=补账前取槽至执行完成释放（design §3 已回填定稿；实现语义=意图语义，较初稿字面加长——补账即释会让错峰失效）
- [x] B1.3 执行份额账目精估（范围收缩）：编码路径源 RGB 3B/px 瞬态面已移除出 exec 账（取证：串行 prepare+零共存窗口+解码器 64MB 独立护栏保留，取小者合法；批 E 6MiB 收紧门复验仍凑满）+ budget 包络 host/device 混记注释（budget.rs:3-5）；CPU 24B/px 与 GPU 12B/px 维持不动；批 E 守恒语义零变化（断言零改动 69/0）
- [x] B1.4 多 Model 守恒单测族：批 E 测试族扩展——双 Model topup 叠加压力整笔拒绝+错峰让位/交替周期全周期归零/等槽期挂起份额保持与收口归还/取消 TTL 排空守恒（8 新测试全绿，PM 独立复跑 am-vision-ffi 69/0；批 E 既有测试零改动全绿；boundary 指纹已登记 286 文件绿）
- [x] B1.5 真装配集成测试（FFI 门）：双 Model e2e——两窗口并发攒批+执行错峰+关闭排空互不干扰（CPU 像素通道）；单 Model 回归零变化。新门 `batch_e2e_dual_model_staggered_execution`（batch_e2e_tests.rs:294-372，复用既有门机制零新机制）首跑即绿两轮+PM 复跑 4/4（真机 ORT）；断言=双窗成批/逐项身份/A 在 B 在途期关闭排空且 B 不受干扰/账目守恒/零 rejected
- [x] B1.6 量化验收（MET-2，GPU 监控条款全程，四臂齐，2026-11-12 夜）：①CPU 双 Model 批 3=20.1/批 4=56.5（均 rejected=0 解锁达成；批 3 反常低归因未拆分如实入档——全凑满零截止却周期 355ms，疑等槽队列与窗口轮转交互，后续单独看不阻塞）；单 Model 回归锚 56.3（vs 57.9 差 2.8% 噪声内）零回归✓；②**GPU 直传双 Model 批 4=194.8 items/s、rejected=0、N=4.00——120-150 承诺超额兑现（超区间上限），8 路@25fps（200）实质满额 97.4%**，评估阶段"8 路不可达"定稿在 GPU 直传通道推翻（CPU 像素口径结论不变）；保护指标全过（事件日志全臂零新增/单 Model 零回归/守恒族+e2e 门 69/0+4/4）；验证记录 .local/evidence/sdk-b1-20261112/verification.md
- [x] B1.7 验证记录（.local/evidence/sdk-b1-20261112/verification.md 四臂全）+ workspace 全绿（9 套件全 ok 两轮）+ 文档链（INTEGRATION 多 Model 分通道指导：GPU 直传批 4 主推 194.8=8 路实质满额/CPU 像素批 2 次优 97.7；CHANGELOG Unreleased sdk-b1 条目英中双份含证据链接与诚实边界）
