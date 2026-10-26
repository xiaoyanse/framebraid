# R2采样编码与尺寸覆盖

核对基线`2d44efa`，沿用[测前卡](sdk-r2-codec-sampling-target-20261026.md)。本包只改观察器与cfg(test)验证，无生产实现/公共API/ABI改动，无AM-RT升级或发布。

## 实测范围与结果

复用上阶段SHA `03c976a6d8009ee7992542ae8fc8c2873a8f347d1ebca2ee5eeb4b0b66d671f9`的Release DLL及同一FFmpeg，合成1080p30，随机本机TCP端口。每编码一轮五个窗口，2s预热/4s计数，输入/制品/运行库/观察器/目标卡指纹和逐帧原数据保留。这是现有实现的扩展验收，不是新旧A/B或性能提升结论；H.264三组A/B仍见[上阶段](sdk-r2-cadence-20261026.md)。

| 编码 | 5档交付 | 10档交付 | 30档交付 | 混合5/10/30交付 | 接收→消费P95最高 |
| --- | --- | --- | --- | --- | --- |
| HEVC Main / yuv420p / GOP30 / B帧 | 20 | 40 | 110 | 19 / 38 / 112 | 208.85ms |
| RTP/JPEG / yuvj420p / 标准Huffman | 20 | 37 | 110 | 18 / 33 / 96 | 67.82ms |

固定4秒窗最低16/32/96帧，全部达到；MJPEG混合30档恰为96，不把23.9968FPS的墙钟尾差误判不足，也不宣传稳定30FPS。所有窗口压缩供给≥27包/s（HEVC约28.49～29.99，JPEG约29.25～31.50，包数不等同画面FPS），P95≤250ms，无PTS倒序/重复身份/格式错误，停源后外部持有均0；纯压缩未启动解码。CPU-s保存在raw，只有一轮，不给新增CPU节省或统计显著性结论。未证明任意现场码流能达到这些值。

## 尺寸、严格间隔与机制

实际生产worker、FFmpeg/图片解码器及真实时钟，H.264/HEVC/MJPEG三种合成独立随机访问画面，交替25/42ms共90次输入，5/10/30/30四消费者；中途模拟Gate先把配置变为2，再从256×144切到128×96。切换前后分别核ceil(1e9/fps)最短间隔（配置变化允许按既有合同重置节拍），不以消费端间隔证明选择间隔。每消费者都实际收到两种维度，源代次仍1，配置/PTS对应，旧在途帧不能贴新配置；关闭后预算归还。三项测试通过，完整raw为`live-selection-with-stream-info.log`，不把独立随机访问画面称为真实GOP吞吐。

本包H.264/HEVC共用的生产emit转换准入和fanout已扩展到尺寸变化；原H.264 90帧5/10混合的转换90→30、含30订阅90→90、无消费者90→0及输出全指纹对拍仍绑定[原生机制记录](sdk-r2-sampling-20261026.md)与上阶段重跑。MJPEG保留头部尺寸检查、只在到期才完整物化；上述网络和worker测试证明对应输出，不把frames_decoded（JPEG与原生语义不同）当跨编码转换计数，也不推断相同比例的CPU收益。无像素消费者由三编码纯压缩无decoder与既有准入单测覆盖。

普通库25通过，10项显式环境项默认忽略（上述3项另行实际执行）；观察器4项单测覆盖误配编码/尺寸、缺视频以及持续生产时有界预热排空/无对象不release。严格clippy/fmt通过。H2完整设备/profile/位深/故障、R2 1.4、1.5和1.6、R3/R4/R5仍开放。本轮关闭R2 1.2的转换准入/采样功能范围；4h/24h并未执行，仍由对应独立任务负责，不以短测代替。

## 失败备料与可复现性

第一次MJPEG样本为FFmpeg默认optimized Huffman；实际publisher日志报`RFC 2435 requires standard Huffman tables for jpeg`，没有向SDK提供有效输入，订阅metadata超时。这次失败保留在`.local/evidence/r2-codec-sampling-mjpeg-20261026`和对应publisher日志，不能记为SDK解码成功或SDK故障。随后按已有传输测试的`-huffman default -force_duplicated_matrix 1 -pix_fmt yuvj420p`另生成新文件、全新目录重测，没有覆盖失败输入指纹或降低尺寸/阈值。

备料：以R1固定testsrc2 1080p30源为输入，HEVC使用libx265 ultrafast、pools=2/frame-threads=1/bframes=2/keyint=30/scenecut=0/repeat-headers=1、6秒；JPEG q=5/threads=2及上述RTP参数、3秒。备料/ffprobe不进入测量窗口。观察器新增`--codec`和`--expected-size`，默认仍H.264/1920×1080；开流前ffprobe拒绝编码/尺寸误配并记录profile/pixel format/标称帧率。声明30不代表实际输入保证，供给以窗口观测为准。

原始证据：`.local/evidence/r2-codec-sampling-{hevc|mjpeg-rtp}-20261026`，机制/构建日志、失败索引与来源清单在`r2-codec-sampling-20261026`。本轮没有修改SDK生产路径，所以不再重建DLL制造新候选；测试源码修订仍通过来源覆盖清单登记。服务结束后退出。最终合成输入供尚未结束的R2长跑继续复用，任务收口时清理；外部`.gitignore`不纳入提交。

清理限制：删除废弃样本`E:/projects/framebraid/.local/tests/r2-codec-sampling-inputs/source-mjpeg.avi`（约6.15MiB）的命令被自动审批以`blocked by policy`拒绝，整条命令未执行，文件仍保留；没有重试或换手段删除。必要指纹/失败日志保留，后续由用户处理或明确解除阻断后再维护。

收口：194项来源/独立边界、10件ABI冻结和五包OpenSpec严格校验通过；R2实际任务状态10/17。按测试场景/代次与配置、观察器输入/有界排空、证据与来源三块自查，未发现新增阻断；未把当前轻载边界当重载保证。签名、支持范围、示例调用与中英文README无新增内容，不重生成未变ABI。

2026-10-26清理复核：用户明确要求验证命令分离规则后，先单独只读核绝对路径/链接/进程，再单独执行原生PowerShell删除，最后另行核验；上述废弃JPEG已删除。另16个闲置测试工作目录先按原字节归档、逐文件SHA-256核对后删除，结果路径均不存在。必要对照DLL和后续测试输入仍在，不动代码或另一团队数据。历史拒绝记录保留，当前清理已完成；详见[清理验证](local-cleanup-command-split-20261026.md)。
