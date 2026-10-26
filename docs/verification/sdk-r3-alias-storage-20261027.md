# R3共享源别名底层存储准入（2026-10-27）

## 目标与范围

源码评审基线 `5f329641e3a307db540c2d8a20ed517684acdad9`，接续1.1s死亡Weak清理。主目标是补齐可变别名引用槽实际容量及增长重叠的控制字节，不是单帧速度；保护共享source_id、所有权、失败原子性、关闭屏障和原64KiB默认预算。只修改SDK内部，C ABI/绑定/版本协商未变，不升级AM-RT。

## 实现与取舍

SharedSource.sources复用内部有序Table，增长前以CameraState.metadata预付新容量，旧存储释放后才退旧额度；回收死亡Weak不退仍保留的容量。沿用域4096句柄的数量边界，不另提高默认值。句柄发布前完成reserve，拒绝由Source Drop归还已claim的共享所有权；域登记失败原清理不变。回调登记按句柄二分查找，不复制像素或凭据。基础全域Source注册表、事件/统计临时快照仍另待，不用本项关闭父1.1。

本机x64 `(usize,Weak<Source>)` 为16字节，旧Weak为8字节：引入键增加单槽尺寸，不能宣称所有内存下降。新首槽capacity=1计16字节，扩2槽时需要旧16+新32=48字节，成功后常驻32字节；不算新的控制项或外持缓冲。关闭/销毁某别名不会强制缩容；共享拥有者真正销毁后释放存储许可。

## 实测与证据

原始日志 `.local/evidence/r3-alias-storage-20261027/`，保留改前失败和首轮编译错误；不存在只保留绿色轮次。测试为合成等待凭据源，不连接相机；Release DLL SHA256 `84a1f3aff7973a281fdc6f33400085ac9d1b594088a627cd916210fed08098cc`。

| 检查 | 结果 |
| --- | --- |
| 改前同一回归 | 实际控制字节0，期望16，明确失败 |
| 源定向8项 | 通过，含增长47拒绝/48接受、旧所有者和旧登记保持、Closed仍计容量、256次无事件别名回收峰值2 |
| Media全特性 | 65通过、11显式忽略 |
| 受影响包全target全特性clippy | 通过 |
| Release构建及真实C ABI | 通过；15字节两次均拒绝且输出NULL；47字节首个成功、扩容拒绝；48字节首个+256次别名共257次成功，同一source_id且逐次close/destroy，Context最终Closed/destroy |

可重放：`cargo test -p am-camera-ffi --all-features sources::tests -- --nocapture`，`python scripts/sdk/test-source-alias-budget.py --dll-dir .local/build/current/release`。外部stats没有control_bytes字段，精确字节采用内部账断言，外部DLL只验证临界行为，不伪造新观测字段。

## 审查与边界

正确性/安全/维护/性能审查：储备先于domain.insert，失败不发布句柄；Source Drop归还claim，登记/回调锁序不变，metadata不新增buffers关闭屏障。无致命或阻断项；复用抓图内部Table而非新增相同容器，captures::tables仅crate内可见，不成为C/Rust外部API。O(n)有序插入和额外key成本如实保留，不宣称吞吐提升。完整基础表、原生OOM、硬件与4h/24h继续开放。

同步CHANGELOG、状态/路线、R3设计/规格/任务及基础清单；公开调用方式、模型档案、示例和语言README无变化，故不重写。来源修订保留历史清单原字节；没有推送/标签/发布。
