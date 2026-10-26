# Spec Delta

## Purpose

为 FrameBraid 的全链路有界压力和故障恢复提供可重复、可观测且受资源与兼容边界约束的验收依据，区分已有实现、改造效果与未验证范围。

## ADDED Requirements

### Requirement: End-to-end bounded pressure

SDK MUST 约束所有内部队列和中转暂存的数量、字节及线程，外部持有独立统计，不用无限重试消化过载。

#### Scenario: Slow consumer under overload

- **WHEN** 消费速度低于输入且通知接收停滞
- **THEN** 预算内拒绝/丢弃有明确原因，健康消费者持续服务并满足测前目标卡预算，压力解除后恢复

#### Scenario: Native pixel work blocks

- **WHEN** 一源的像素初始化、解码或回读原生调用暂时阻塞
- **THEN** 压缩分发不在同一执行线程等待该调用；像素worker先取得现有decoder额度且不创建替代线程，队列数量/字节有界，溢出后旧在途结果不发布并等待真实关键帧恢复，关闭在原生调用返回前保持Closing

#### Scenario: Inference classes share an executor

- **WHEN** 一个 Context 的实时输入尚未归还，另一类或另一 Context 在自身字节预算内提交
- **THEN** 分别核分类暂存、本 Context 合计及执行器总额，不将其他分类或 Context 用量套入当前类较小上限；超额拒绝不留账、不产生请求 ID，归还屏障完成后才释放原占用

#### Scenario: ISAPI sampling subscriptions change repeatedly

- **WHEN** 一个慢频率消费者持续运行，其他频率订阅反复开始、取图并排空，或停止与HTTP结果提交重叠
- **THEN** 最近图片缓存只保留活跃源×频率，最后同频订阅排空即释放其缓存；晚提交不能复活退役键、旧帧不能覆盖新帧，健康抓图不因历史频率缓存耗尽预算，实际读取中的外部引用仍保持到release

#### Scenario: Standalone decoder rejects new input under pressure

- **WHEN** 有效独立解码器因队列数量/字节或Context字节预算而拒绝新提交
- **THEN** 队列占用不变、拒绝累计增加一次；成功入队增加一次接受累计，参数错误和reset不伪造拒绝或清除历史计数，其他Context不受影响

#### Scenario: Media subscription metadata reaches the control limit

- **WHEN** 订阅对象、最大引用队列或缩放预设容量超出Context控制预算
- **THEN** 构造明确拒绝且不留下占用；被接受的对象持有控制额度至最后引用释放，载荷额度独立，不因关闭或清队列提前归还仍常驻的元数据

#### Scenario: RTSP framing requires new SDK buffers

- **WHEN** RTSP配置或包转换需要新的参数集、临时缓冲和输出载荷，或探测打开同类流
- **THEN** 在分配这些SDK缓冲前先申请可核对的工作区额度，成功后保留实际配置/载荷额度并归还余量；错误关闭Reader后才释放仍覆盖其缓冲的额度，不扩大原生库内存承诺

#### Scenario: Inference notification has less than one entry of control bytes

- **WHEN** 尚无预付存储且控制剩余额度不足一个包含其拥有权许可的完整通知条目
- **THEN** 明确拒绝注册或新通知，丢弃计数明确；已接受通知持有预付条目存储及活项额度直到消费/注销/停止释放，不将通知丢弃改成推理失败

#### Scenario: Inference notification queue retains spare storage

- **WHEN** 通知队列留有空闲槽、反复注册或在忙回调期间停止
- **THEN** 队列分配前预留控制存储额度、固定容量且空闲槽不提前退账；活通知项数和关闭屏障独立保留而不重复计存储字节，排空后存储和许可一并归还；预算不足的首次注册不残留工作线程

#### Scenario: Decoder configuration and parameter caches consume domain bytes

- **WHEN** 创建/重置独立解码器、更新参数集或生成原生重配置的extra副本
- **THEN** 在相关SDK复制前预留域字节、缓存和临时副本持有到真正释放；空配置重置共享不可变配置，构造或准入失败不改变旧配置/代次及外持帧，NAL检查不靠额外列表暂存且保留既有接受/拒绝范围

#### Scenario: Source event storage remains allocated between callbacks

- **WHEN** 源事件回调停滞、队列有空闲槽，或注册失败/注销/停止
- **THEN** 事件存储分配前预留且运行期不扩容，空闲槽持续计费；排队及忙项受固定槽数和64项硬限，满时仅丢待处理旧提示并明确计数，不能覆盖忙项；排空后归还存储，注册失败无新工作线程，视频消费者不依赖提示交付

#### Scenario: Rust H264Hub subscriber stalls or closes

- **WHEN** Rust辅助分发的客户端停滞、视频和叠层共同占用队列，或关闭与等待/投递重叠
- **THEN** 单订阅36项及8 MiB载荷有界，GOP最多35包并受字节限制；叠层先让位视频，受损参考链等待新关键帧，关闭只交付一次终态且不可复活，健康订阅不被慢订阅的清队列或关闭影响

### Requirement: Generation-safe recovery and draining

恢复和关闭 SHALL 保留正确源代次与请求终态，重建参考链后才交付，关闭排空后不再回调。

#### Scenario: Disconnect during outstanding work

- **WHEN** 断流重连与在途请求或关闭重叠
- **THEN** 旧代次结果不冒充新结果，租约读取结束后释放，不出现迟到回调


### Requirement: Recovery and stability have scenario budgets

故障验收 SHALL 按目标卡分别记录恢复首个正确新鲜输出、恢复持续达标窗口、健康消费者服务和稳定段资源趋势；不得用故障期间低吞吐或首尾RSS差替代恢复/稳定性结论。

#### Scenario: A slow decoder shares a process with compressed consumers

- **WHEN** 原生解码/回读变慢且健康压缩消费者仍在读取
- **THEN** 按测前响应/交付预算验证隔离效果与队列/线程上限，不以“不重连”或“延迟绝对零影响”替代实测要求
