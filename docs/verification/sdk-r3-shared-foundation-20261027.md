# R3：共享帧本地基础索引固定容量（2026-10-27）

状态：隔离实施及主代理定向验证通过，等待 main 整合、最终双 DLL 和跨进程回归；不代表整体 R3 通过。

## 范围与指标

- 主目标：三个已有硬上限的小索引不再首次登记/扩容分配 HashMap；保持 4 个映射、8 个消费者身份、32 个初始借用的既有合同。
- 保护指标：跨进程共享布局/C ABI 不变；不提前清除未知进程或外部 retain 持有的租约；任何登记失败不会遗留共享租约或不可归还 buffer。
- 内联索引属于固定基础对象内存，不计作额外控制消息；实际目标结构字节由 `fixed_shared_index_sizes_and_capacity` 输出。只证明索引存储无堆分配，不声称整条共享链零分配、全程零拷贝、吞吐提升或 RSS 下降。

## 实现与风险控制

| 索引 | 之前 | 本次 | 边界处理 |
| --- | --- | --- | --- |
| `Ledger.maps` | HashMap，首次/扩容可分配 | 4 个内联 `Option<(tag, Weak<Pair>)>` | 先清死 Weak，再检查既有配额；满表拒绝，不淘汰活映射 |
| `Pool.initial` | HashMap | 32 个内联 `(borrow_id, pointer)` 槽 | 持同一锁预留槽，buffer 登记成功后无失败提交；失败由新 Lease 的 Drop 写归还意向 |
| `Pair.verified` | HashMap，旧8+新8可瞬时扩容 | 8 个内联进程对象槽 | 扫消费者前先删除不在共享消费者表中的缓存；打开进程失败不是死亡证据，不清消费者/lease |

内联数组使用线性查找，最坏仅 4/8/32 个槽。基础对象常驻空间增加，换取可确定上限和无表分配路径；尚无端到端性能优越结论。`Arc<Lease>`、映射、进程句柄、全域 buffer registry 等其他分配不在本包“无表分配”结论内。

## 可重复验证入口

主代理统一运行 `cargo test -p am-sdk-support --all-features shared:: -- --nocapture`，记录实际结果和结构字节。新增定向用例：

- 32 个借用占满，第33个拒绝；归还后复借；外部 retain 未释放前不提前复用共享租约。
- 全域 buffer 表4096满时，新共享 Lease 登记失败，初始索引为空、local leases归零、reap后共享 outstanding归零；释放buffer槽后可重新借用。
- 4 个活映射拒第5个，死 Weak 清理后可复用原槽。
- 旧8个消费者身份整批更换为新8个，进程缓存一直不超过8且保留全部新身份。
- 无法打开的进程保留消费者与借用，只有显式 lease 归还意向释放对应租约。

主代理复测 `shared::` 为 **10 通过、0 失败**，最终日志为主树 `.local/evidence/r3-alias-storage-20261027/shared-worker-tests-final.log`。首轮全 target、全 feature 的受影响包 clippy 通过；首轮后仅修测试 fixture，生产实现未变。main 整合、最终双 DLL、跨进程共享帧回归、受控来源清单及 OpenSpec 同步仍由主代理集成记录。未验硬件及 4h/24h 持续运行不因本包关闭。

首轮主代理定向测试为 8 通过、2 失败：新增借用 fixture 未填写必需关联有效位及无时钟的 `i64::MIN`，正确被 `validate_desc` 拒绝为 `INVALID_ARGUMENT(3)`。已仅修合成输入，未改实现或放松断言；原轮日志保留在主树 `.local/evidence/r3-alias-storage-20261027/shared-worker-tests.log`。实测 Windows x64 内联表字节：Ledger **160 B**、initial **768 B**、verified **320 B**；这些是索引本体，不含锁/Arc/映射/进程句柄指向的系统资源。

## 主线整合补充

上述隔离待验状态已由主线整合复核更新，最终双DLL、329项工作区及跨进程/真实CPU推理见[整合记录](sdk-r3-foundation-integration-20261027.md)。原隔离日志与失败轮不覆盖。
