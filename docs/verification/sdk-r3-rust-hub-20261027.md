# R3 Rust辅助H264Hub线程与会话身份

2026-10-27；评审基线`9d157500fb5a29d2536c84af2abcc5af5a8f6922`，R3 1.1r/BP12。SDK main串行、本地未发布，未改AM-RT、未接现场设备。

## 问题与修复

此前矩阵将直接Rust路径笼统称为SessionPool；当前源码无此类型，实际辅助入口是`am_camera::media::h264::session::H264Hub`，与C ABI source/decoder不同。

旧ensure_session没有源总数上限；存活标志在会话锁释放后才写，首订存在重复启动窗口。stop=true即可替换条目，旧线程仍阻塞时可能被新线程绕过。订阅Drop只按URL+客户端ID退订，而新Core的ID从1重启，旧订阅有机会移除新客户端。旧线程正常清理不覆盖resolver panic，资源不足的30秒等待不及时检查停止。

- 每Hub最多16个登记，满额在创建Core/启动线程前拒绝TooManySources；存活标记在锁内发布。
- 存活的停止工作继续占位，subscribe在客户端锁内再次核停止/存活，返回NotReady；最后订阅退订在同锁内写stop，避免新订阅插入后被误停。
- H264Subscription持原Core，退订及Map删除都核Arc身份，不通过URL/重置ID操作新Core。
- 线程守卫覆盖正常返回、spawn捕获销毁和panic展开；原生/用户resolver结束后才关队列、清就绪/配置/参数/GOP及存活登记。队列已有终态不覆盖；不是强杀原生调用或允许替代线程。
- 30秒资源退避和普通退避共用分段停止检查，最长睡眠片为100ms；实际停止响应仍受OS调度及正在执行的用户/原生调用约束。

Rust公开DenyReason增加TooManySources，穷举match调用方必须补分支；这是Rust源码迁移与资源行为收紧，未发布，不当成C ABI变更。C结构/导出/绑定/冻结不变，AM-RT固定依赖没有升级。上限为每Hub，调用方创建多个Hub时不自动合并成进程全局额度，旧Closed订阅持有的引用不冒充活工作。数据缓存、外部持有与原生内存继续分别核查。

## 验证与机制计数

6项确定性回归，不依赖现场设备：

| 场景 | 结果 |
| --- | --- |
| 16个在册源，再尝试32次新源 | 全部TooManySources，登记仍16，不启动第17个源 |
| 16个并发首订调用者，每人20次 | 总320次subscribe，同源resolver实际进入1次 |
| resolver被屏障阻塞后请求stop，再20次首订 | 仍NotReady/同一Core，未创建替代；解除屏障后归还登记 |
| 停止但存活的Core上100次重试 | 不替换Core、不绕过原工作 |
| 新旧Core同URL/同客户端ID1，释放旧订阅 | 旧客户端移除，新客户端1条及新Map条目/运行状态保持 |
| 受控panic与已stop的30秒退避 | 守卫清就绪/配置/参数、关队列；退避无需等满30秒 |

这些是线程/身份机制验证，不是吞吐提速或真实网络峰值承载。panic日志为刻意触发并被测试捕获；首轮编译的私有借用类型不匹配已修正，原失败日志保留。

全工作区all-features：301 passed、19 ignored；am-camera为66 passed/2 ignored。受影响am-camera与camera-ffi严格clippy通过。真实仓内MP4样本存在且SHA-256为`67b6202d84fc721a5e49922c0f05fc58f6df0d2a8f605d8b2dc3f515bd3e3e73`；实际FFmpeg打开、H.264 Annex B配置/包交付和EOF重连代次用例通过，没有走样本缺失的跳过分支。运行库使用本仓隔离FFmpeg候选，未调用客户相机。

主证据`.local/evidence/r3-rust-hub-20261027/`含unit-first/unit-final、workspace-tests、clippy和身份。测试没有创建外部服务、业务库、模型副本或大型视频副本；屏障解除后实际辅助线程退出，测试进程终止，无本包待停服务。

## 审查及后续

核对会话锁→客户端锁次序、退订释放客户端锁后再取会话锁、线程占位发布、Guard拥有者、旧Core和队列终态；仓内使用点编译通过。未发现阻断项。Rust辅助公开API限制和枚举迁移在接入/支持及双语README同步，不改旧冻结和历史发布包。

最终来源检查224文件、零业务依赖/外部本地路径，C头132函数、冻结11项和R3严格OpenSpec校验通过；原始提取清单和受保护.gitignore未改变，runtimeAcceptance仍pending。测试可执行文件与合成输入SHA绑定私有identity.json，不用旧DLL身份冒充本组Rust源码验证。

BP12辅助线程基数/重启已补，原生不可管线程、混合硬件故障/长期趋势仍开放。BP10服务体/诊断字符串/其他缓存注册、R4对象scope及完整语言/模型档案、4h/24h未由本阶段关闭。既有辅助源认证/协议行为不由本组机制用例证明完整，正式C ABI恢复证据仍单独查看。只本地提交、不发布。
