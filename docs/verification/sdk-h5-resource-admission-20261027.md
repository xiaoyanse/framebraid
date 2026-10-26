# H5 逐候选资源准入与显式回退

## 测前目标卡

基线`3741178`，本地Release为上一包制品（SHA见原始记录）；候选为本记录同提交。MET-2 M-SELECT/S3，主目标是在既定允许列表/strict及线程硬限内，使可运行的CPU候选不被未选中的OpenVINO最大预留拦住。不是模型推理提速。

- 固定CPU版ORT、Identity模型、执行器2线程、总预算3、每模型intra=2。OpenVINO需额外配置线程，不能在此额度内准入；CPU可以。
- 三轮加载`[OpenVINO:CPU, CPU]`：基线明确拒绝，候选允许时转CPU并真实Run；strict或未列CPU仍拒绝。并发第二个CPU模型超额须拒绝，关闭后重试须恢复，不扩大预算或重放请求。
- 测量输入/输出、尝试阶段、实际后端、threads_active/peak与加载耗时。成功率和预算为硬门；失败加载与成功加载不比“提速”。新旧加载路径同功能回归另核，不重跑无关媒体压力。
- 资源准入在候选开始、原生构造之前完成，失败原因必须进入选择报告；模型/文件/档案错误不能以回退掩盖。实际加速EP忙/显存不足和生产长跑另验，不用SDK线程拒绝代替硬件故障。
- 程序`scripts/sdk/test-selection-admission.py`，基线带`--baseline`；私有原始证据`.local/evidence/h5-resource-admission-20261027/`。本阶段不改C签名/布局/能力位和既有legacy加载行为，Rust兼容新增准入帮助入口。

## 结果

本地策略/资源准入与CPU故障收口阶段通过；真实加速与长期耐压未验。


同一总预算3/执行器2/intra2，改前3/3拒绝（resourceExhausted），改后3/3实际CPU推理且Identity输出48字节逐位一致，threads_active/peak均为3。strict、未列CPU和第二CPU模型超额仍拒绝；关闭后下一模型准入恢复。加载耗时改前为[2.025, 0.6666, 0.4471]ms、改后[121.1183, 3.148, 2.709]ms；两者失败/成功不同，首轮有冷起成本，不作提速比较。主收益是可用性纠错，不是吞吐或GPU卸载。

越界Gather真实ORT Run错误返回稳定SDK错误和原生code=2，两个间隔读取的请求终态及finished时间一致，模型native_failures=1/native_runs=0，无自动重放/切模型。显式关闭并新建不同model_uid后正常模型Run=1且failure=0；无效ONNX在候选准入前停止，不以换设备掩盖。

对同一候选执行旧初始化失败/坏库/已加载外部库3种独立进程恢复控制，显式retry受活context约束且恢复后CPU可运行；旧ordered入口/查询/CPU Pose回归通过。纯单元测试55通过、6项显式原生测试默认忽略；本轮CPU真实数据另验，不代替CUDA/OpenVINO实机。受影响两包clippy严格检查/fmt通过。C ABI签名、布局、能力、冻结及4组生成检查保持；只兼容新增Rust高级准入方法。

源码基线3741178；基线DLL SHA-256=12a2b575ca8d8e5fb93e765e735aeda55a458838e59615a6353d295a8872be7f，候选=6c4bd8b1adfdba3ff381a3f022a8c66db591ca42b62f5fa9e638a33e41f1c4ac。模型/运行库沿用上包显式CPU材料，私有result.json、stdout/stderr和恢复控制保留。先前中间候选输出不用于最终制品身份。

审查：同一执行器/模型许可内短锁调整配额，拒绝时不改变原额，溢出拒绝；原生构造返回并释放失败会话后才尝试下个候选。许可覆盖成功会话寿命，drop一次归还；计数与关闭逻辑未旁路。旧load行为独立保留，Rustload/load_selected默认兼容。未发现本阶段阻断项；真实显存不足/设备失效和长跑仍开放。

同步ABI行为说明、Rust帮助接口说明、路线/状态/CHANGELOG和OpenSpec。README中英文/能力范围/平台承诺、绑定/示例和第三方依赖无需改：没有新增C出口/支持档案/配置或发行，只修新选择的资源准入；旧二进制不更新。本轮源码来源覆盖清单同步，原提取清单/ABI历史不覆盖。工作区外部.gitignore与AGENTS.md保留，不入本提交。
