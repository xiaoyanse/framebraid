# CPU共享帧传输协议1：实现布局与限制

> FrameBraid source reference: historical host reports/OpenSpec are not included. For actual validation and current support, see [documentation](../README.md). Historical status text is preserved; this note does not change ABI semantics.

2026-10-25，批6阶段实施。公共C ABI保持发布前r2不变；本文件锁定内部共享段格式。实现入口为`crates/sdk-support/src/shared/`，两个DLL链接同份源码，不增加第三个DLL。完整合同仍见[共享帧合同](dual-sdk-shared-frames.md)。

## 分段与字段

所有整数显式小端；没有结构padding、进程地址、Rust对象或原生锁布局。控制段固定65536字节，像素段等于slot_count×max_frame_bytes。内核对象后缀为ctrl/data/lock，随机标签32字节；命名和权限按共享帧合同。

| 控制段偏移 | 字节数 | 内容 |
| --- | --- | --- |
| 0 | 8 | ASCII AMRTSFP1 |
| 8 / 12 | 各4 | 传输版本1 / 固定头长度512 |
| 16 / 24 | 各8 | 控制段长度 / 像素段长度 |
| 32 / 40 / 48 | 各8 | pool_id / owner PID / 进程创建FILETIME |
| 56 / 60 / 64 / 68 / 72 | 各4 | 池代次 / 槽数 / 每槽字节 / 消费者上限 / 租约上限 |
| 76 / 80 / 84 / 88 | 各4 | 丢旧策略 / 发布等待 / 故障位 / 关闭状态 |
| 96 | 32 | identity_tag |
| 128 / 136 | 各8 | 最后发布号 / 最后借用号，非零递增，溢出拒绝 |
| 144 / 152 / 160 / 168 | 各8 | 发布数 / 淘汰旧槽数 / 满池拒绝数 / 等待超时数 |
| 176 | 8 | 关闭意向邮箱，原子写；实际状态在命名锁内转换 |
| 512 | 每槽512 | 最多4槽：+0发布号8字节，+8有效长度4字节，+12租约数4字节，+32帧描述172字节 |
| 4096 | 每项64 | 最多8消费者：+0随机身份8字节，+8 PID8字节，+16创建FILETIME8字节，+24退出意向邮箱8字节 |
| 8192 | 每项64 | 最多32租约：+0借用号8字节，+8消费者身份8字节，+16槽索引4字节，+24归还意向邮箱8字节 |

172字节帧描述严格依公共`am_shared_frame_desc_t`字段声明顺序逐字段编码，数组按元素编码，无C对齐空洞。只有上述三个8字节邮箱采用Windows x64自然对齐的原子读写，其余字段均在命名互斥量保护下访问。邮箱不是租约计数器，不能直接使槽可重用。

## 复制与所有权

发布端先校验格式、尺寸、每平面stride/有效长度/区间、时间有效位和源身份；准入成功后同步复制一次到可用共享槽。借用及像素桥接不复制像素。消费者像素映射使用FILE_MAP_READ，控制映射单独读写；实测VirtualQuery为PAGE_READONLY。RGB/BGR/RGBA单平面，NV12双平面采用偶数宽高，色彩范围与矩阵保留。JPEG须先解码，不能直接发布压缩字节。

每个借用生成库内稳定Lease，初始return只撤销初始引用。跨语言retain/release桥接延长同一Lease；最后引用释放仅原子写入对应归还意向，不等待命名锁、不创建线程。发布/借用/统计/关闭在取得命名锁后消费匹配borrow_id的邮箱，再减少槽引用。记录复用前清零邮箱；不会把迟到旧号当成新租约归还。同一borrow的提交与首次return仍须调用方串行。

每ctx账本按随机身份合并重复attach映射；新身份在映射像素前检查累计像素、控制元数据及池数量上限。attach验证头时允许一个固定64KiB临时控制映射，未验证的描述不能触发任意大小像素映射。等待仅占调用线程，每ctx最多16名等待者，不分配每帧线程；轮询间隔1ms，wait=0不等待，最多5000ms，截止后不迟到提交。

## 关闭和异常

发布端关闭意向立即拒绝新的borrow/attach；池仍停在CLOSING直至已登记消费者和租约排空。消费端detach拒绝新借用，允许return与内部release。缓存的进程对象核对PID、创建时间、相同用户与会话，只有其退出信号才可强制回收该消费者。若进程在首次核验前已退出且无法取得原对象，不凭PID查询失败推定退出；保守保留记录与预算，不承诺此分支自动恢复。

abandoned mutex令池故障，后续复用返回internalError；不猜半写状态，不强改为CLOSED。当前安全退化为保留CLOSING及资源，直至拥有进程退出由Windows回收；没有承诺在原故障池内自愈。内核句柄均不继承；现有只读映射不会因发布进程退出而立即失效，新borrow则拒绝。

## 验证范围

Windows x64实际双DLL，四种像素逐字节、只读页保护、跨进程进退、保留引用、预算/等待/身份错误、活慢读者、进程退出和abandoned路径已定向验证，见实施记录 (historical host reference, not included: `../verification/verification-dual-sdk-implementation-20261025.md`)。普通CPU共享帧基础不是媒体解码到共享池、实际异步双模型或四门整包验收；这些仍保持未完成。
