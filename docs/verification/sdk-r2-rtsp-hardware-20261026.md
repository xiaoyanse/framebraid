# R2/H2 RTSP硬件策略、源报告与像素故障隔离阶段记录

日期：2026-10-26。评审基线：`b065bf10181c74ace5c8850672a5d75fc82e2869`。
本次为main本地阶段保存，不是R2/H2整包性能验收或发布；AM-RT未修改、未升级。

## 实现与合同

- 新增兼容导出`amc_source_set_decode_policy`，复用PREFER_HW=0/SOFTWARE_ONLY=1。策略属于共享连接，单拥有者首次像素订阅前可变，同值幂等。reuse领取与设置同registry锁，像素订阅发布与冻结同decode锁；无回调push也冻结，注销不解锁，ISAPI拒绝。
- 原`amc_get_decode_hardware_report`接source，查询只复制有界快照，初始化选择与真实执行分开。source排空期间报closing、完成后才报closed，均隐藏实际后端/选择，不观察其他共享拥有者后续活动。C头/Python/C#由同一生成链同步，共124函数、74结构/547偏移。新r7保留r6原件与其9工件Git字节核验；旧布局、枚举、1.0初始化不变，未发布的1.1开发绑定仍须配同提交DLL。
- 仅像素消费者触发共用有界选择器，H264/HEVC从真实随机访问点开始。CPU启动/恢复送包前按已知合法尺寸预留RGBA工作额度，实际回读/外部帧另按原Charge核定。每源最多一份选择报告，保守控制预留8KiB，query临时32KiB；不把驱动/DPB等同托管内存硬限。
- 原生send/receive/回读失败只撤销像素decoder、清像素队列与映射，并通知像素订阅singleSub事件；新关键帧恢复CPU，不重放旧RTSP包。负结果只保留当前codec/参数/尺寸键一项，最多30秒，到期仅在既有重建点重试；不为探测打断健康CPU。初始化资源失败有1秒退避，SDK last_status与原生last_failure分开。
- 审查纠正：后端失败不等于视场/压缩配置变化，使用独立decode_generation诊断值，不改压缩config_generation、源代次、网络会话或压缩队列。真正码流/尺寸变化沿原合同。旧外部帧不可变、仍按原PTS有效。MJPEG报告image-cpu/effective软件，并清除上个编码的选择快照。

## 实测与范围

| 检查 | 结果及限制 |
|---|---|
| 最终Release真实RTSP硬件策略 | 合成H264/HEVC实际ffmpeg-d3d11va，Intel Arc 140T；每类16压缩包/6真实RGBA帧；MJPEG同样16包/6帧，actual=image-cpu |
| 最终Release软件对照 | 三类均通过；H264/HEVC selected=null、attempts为空、actual=ffmpeg-cpu |
| 纯压缩/慢消费者 | 像素订阅前idle、selection=null且无活动像素decoder；慢压缩消费者从关键帧恢复；不等同驱动阻塞下延迟隔离 |
| Rext真实故障与CPU恢复 | 显式原生用例真实触发HEVC Rext硬件错误；清像素decoder/映射，压缩原包仍在，源/网络/压缩配置代次不变，decode_generation=2；下一随机访问帧CPU真实输出，保留原失败与选择 |
| 策略/生命周期 | 两拥有者拒绝改、同值幂等；无回调push冻结、注销后仍冻结、ISAPI拒绝；关闭一个拥有者不显示其他活动；回调内设置拒绝 |
| C/C++17 | GCC/G++ -Wall -Wextra -Werror真实调用新函数，旧1.0初始化、策略、订阅冻结、closed快照、关闭/销毁通过 |
| C#/Python | 高层源策略/报告、冻结与关闭实调通过；32结构/228字段偏移、6所有权单测与C#示例编译通过，不扩大为完整真实业务验收 |
| 直接入口回归 | 最终Release旧1.0初始化/未来ABI拒绝、probe/快照、直接硬解/CPU恢复与缓冲边界脚本通过 |
| workspace | 明确设置配套FFmpeg目录后201通过、9显式项默认忽略；硬件故障与量化单独执行。首次漏设运行库目录造成6个旧媒体用例失败，保留原日志；补环境后全量通过，没有删除或忽略失败断言 |

JSON测试检查最低字段合同，不是通用Schema引擎。网络合成输入256×144/12FPS、像素上限4FPS；尚未量化5/10/30FPS网络有效采样率或全部profile/位深/其他GPU。

## 量化边界与结果

Release当前候选两种策略对照，1080p相同单IDR反复输入，3轮交替顺序；每组预热20帧、计时120帧且全部交付，逐帧核PTS/received。计时边界为send+receive+RGBA回读/转换+生产fanout，pull/内容检查在计时外。最终独立量化前确认本轮SDK RTSP服务/发布进程退出；较早Debug及可能重叠诊断的日志保留，不用于本表。

| 轮次 | 实际后端 | 初始化ms | 单帧中位ms | P95 ms | 交付帧 |
|---|---|---:|---:|---:|---:|
| 1 | ffmpeg-cpu | 6.392 | 2.650 | 2.747 | 120 |
| 1 | ffmpeg-d3d11va | 122.387 | 6.064 | 6.448 | 120 |
| 2 | ffmpeg-d3d11va | 71.746 | 5.931 | 6.312 | 120 |
| 2 | ffmpeg-cpu | 0.270 | 2.642 | 2.752 | 120 |
| 3 | ffmpeg-cpu | 0.277 | 2.647 | 2.794 | 120 |
| 3 | ffmpeg-d3d11va | 70.513 | 5.913 | 7.339 | 120 |

这不是历史二进制改前/改后、摄像机至前端延迟、P/B混合吞吐、CPU占用或显存测量。样本不足不报P99；不据交付计数宣称跨硬件全平面逐字节相同。关闭后托管帧字节回零，日志budget_bytes_after为存活decoder额度而非RSS/显存。

**H2-STREAM-01：本机硬件RGBA整段P95明显高于CPU，尚未归因子阶段，不通过性能验收，也不承诺自动找到最快设备。** 保存功能阶段，保留软件策略，继续分项量化后优化；不放宽5%警戒或删除不利样本。R2-SAMPLE-02原告警保持开放。

## 自查与未验

按三组审查：源/订阅锁序与生命周期；原生选择/额度/恢复；ABI/绑定/冻结/文档。已纠正无回调push未冻结、故障误改压缩配置代次、MJPEG旧报告残留及排空前误报closed。报告无凭据、URL或运行库路径；原生调用仍是软预算，不强杀驱动。没有新增释放后使用或无界缓存阻断项。

保留警告与任务：

1. H2-STREAM-01硬件RGBA路径较慢，未通过性能目标。
2. H2-STREAM-02原共享读流线程仍同步执行初始化/像素解码，慢驱动可能延迟压缩读取。当前仅证明故障不重连/不清队列，不证明零延迟影响。R3需有界像素执行隔离与压力对照，禁止无上限加线程。
3. H2.3源参数重建仍重新选择，底层设备复用原语待装配；完整额度、关闭竞争、尺寸、位深、其他GPU/无设备机器矩阵未验。
4. H2/H5及R2～R5余项、4h/24h未完成；H2.3b仅关闭基本接线，旧v0.1.0和公开支持范围不变。

`.Codex/memory/patterns`不存在，不虚报检查。既有consume长函数与native构造的职责拆分随R3有界路径收口，不为行数堆叠代码。

## 证据与回收

- `.local/tests/h2-rtsp-final-{hardware,software}/result.json`与各编码日志；随机回环端口、MediaMTX自产流，finally只停止本轮进程，没有连接现场相机。
- `.local/evidence/h2-rtsp-native-recovery-final-20261026.log`、`h2-rtsp-policy-release-quant-idle-20261026.log`及`h2-rtsp-policy-summary-20261026.json`。
- 同前缀workspace首次/最终、clippy、生成/来源/冻结、C/C++/C#/Python日志；`.local/tests/h2-rtsp-language`保留最小调用源和结果。
- `.local/evidence/h2-rtsp-manifest-20261026.json`绑定源码、Release DLL、运行库、冻结、工具、合成输入与证据，不把版本文本当完整源码树哈希。

来源179文件、9冻结工件、生成链和五包OpenSpec严格校验通过；格式/严格clippy通过。本轮服务和测试进程已结束。临时目录`.local/tests/h2-workspace-tmp`的清理被自动审批以blocked by policy拒绝；命令未执行，目录保留，没有换方式绕过，不宣称清理完成。后续关闭态回归的隔离小根`.local/tests/h2-workspace-final2-tmp`也随验证材料保留，未重复尝试删除原目录。其他小型证据及测试工具保留。

外部.gitignore改动保留、不提交。本地阶段提交不等于验收全部优化，不推送/打标签/发布。
