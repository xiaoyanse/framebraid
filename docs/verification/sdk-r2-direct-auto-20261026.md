# R2 H2.3a 直接DLL自动硬解与CPU恢复（2026-10-26）

源基线`ed9c5c1`，Windows x64、本机Intel Arc 140T及匹配FFmpeg新运行库。仅本地合成输入，不连接设备、不改AM-RT，不替换公开制品。完整合同见[原生硬件](../design/native-hardware.md)，既有C ABI枚举/布局/所有权不变，未修改生成器或冻结文件。

## 已实现

- create/reset显式映射C `PREFER_HW=0`/`SOFTWARE_ONLY=1`到共用Rust选择工厂；选择预算1.5秒，原生调用间检查。参数集重建复用已选设备；receive不反复探测。
- 硬件初始化但尚未出帧时后端NULL、hw_accelerated=0；真实输出才报告ffmpeg-d3d11va/dxva2/cpu，关闭后仍可查询最后信息。配置偏好不冒充实际执行。
- send、receive、回读、drain或参数重建硬件错误时，先预留合法尺寸下的CPU输出和NV12规范化暂存；额度不足明确拒绝，不递增代次。预留原子转交外部帧，不重复计费，仍检查外部字节/帧数上限。
- 成功恢复递增源/配置代次并等待随机访问点，保留draining意图；当前decoder不循环重试硬件，显式reset才重新选择。只有尚未发布、仅接受过一张画面的启动包允许保留并CPU重试一次；在途槽最多一份，字节额度覆盖寿命，不挤进已满输入队列或重放其他已接受画面。
- 已输出Vec和原始PTS/received独立；复制失败前不标记该画面已交付。同步故障诊断标记camera.decoder.hardwareFallback；后续其他错误可覆盖last_error，不把它当完整历史事件报告。

原生DPB、驱动显存仍不属于SDK托管字节硬限。完整适配器身份/初始化分层原因的C查询、RTSP像素入口及H5仍未交付，H2.1/H2.3/H2.4保持开放。未增加无实际装配的语言API，不用本阶段声称整包硬件支持完成。

## 验证

工具`scripts/sdk/test-decoder-hardware.py`调用真实Release DLL（可加`--software`作同构建CPU对照）。本机合成1080p：

| 码流 | 实际后端 | 源代次 | PTS | 结果 |
|---|---|---|---|---|
| H.264 | D3D11VA | 1 | 123 | 硬解/回读/输出/排空/关闭查询通过 |
| HEVC Main | D3D11VA | 1 | 123 | 同上 |
| HEVC Main10 | D3D11VA | 1 | 123 | 同上 |
| HEVC Rext（本机硬件不支持） | CPU | 2 | 123 | 真实send初始化失败后自动CPU恢复、正常排空 |

显式Rust故障注入测试`native_auto_fallback_reserves_budget_replays_only_unpublished_startup_and_keeps_old_frames`在真实设备/解码基础上，仅替换测试API的回读函数：

- RGBA/RGB/BGR/NV12 × 首次出帧前失败/已出帧后失败，共8场景。启动包只输出一次；已输出后失败的输入不重放，真实非随机访问AU被丢弃，后续新IDR才恢复。
- 首次失败前drain仍完成CPU启动恢复；原PTS保留，新代次为2。
- 旧GPU输出在CPU恢复后仍可读；CPU输出跨显式reset和新硬件输出仍不变。reset后新硬件代次3，实际执行重新验证。
- 预算不足拒绝恢复且保留原代次；额度转交总字节不增加、外部限额拒绝不吃掉原预留。关闭后域总字节/外部字节/外部帧数均为0。

CPU现有脚本回归：`test-decoder.py`含H.264/HEVC B帧、PTS、跨库共享及reset旧帧；`test-camera-pixels.py`含4格式、缩放和关闭额度，均通过。普通FFI回归新增独立预算转交测试（无GPU依赖），外部持帧计数拒绝、原预留保持、释放总额度与Context.buffers归零均有断言；2项显式原生测试默认忽略，上述新原生测试已显式执行。strict clippy/fmt及来源/ABI/生成/OpenSpec检查随阶段记录；未运行的矩阵不因此勾选。

新增普通测试初版使用全局域时，其他并行测试撞到既有context数量上限，出现ResourceExhausted。已改为独立Domain的预算单测，不提升生产上限或串行掩盖竞争；最终普通回归12通过/2显式项默认忽略，严格clippy通过。

来源边界174个核心文件、ABI冻结8项、生成121函数、五个OpenSpec change严格校验、fmt与diff空白检查均通过。没有改写历史来源清单或ABI冻结摘要。

## 量化与限制

同一Release DLL、同输入、CPU控制与PreferHardware三轮交替，记录create与首个receive耗时；**这是冷启动短测，不是持续吞吐或历史二进制A/B**。三轮H.264 CPU create约2.7～3.4ms/receive4.4～4.6ms，硬件create约122～129ms/receive43～46ms；HEVC硬件create约64～69ms/receive约37～44ms，同构建CPU明显更快。初始化和首次硬件表面建立有成本，不能声称自动选择最快设备或端到端性能提升。Rext回退的RGBA SHA-256与同构建CPU逐轮完全一致。调用方可明确选SOFTWARE_ONLY；持续流有效交付率、P95和CPU/显存矩阵留H2.4验收。

证据`.local/evidence/h2-dll-auto-{native,cpu}-{1,2,3}-20261026.json`及注入日志、CPU回归JSON。最终清单`h2-dll-auto-manifest-20261026.json`绑定源文件、Release DLL、样本和运行库；本地证据不外发。

## 提交前审查与未验

按正确性、生命周期、边界、安全和性能自查：无新增阻断。生产receive/recovery拆为独立模块，主文件保持800行内；较长验收测试顺序执行，用真实输出、故障注入和域出账校验，不以实现镜像断言代替结果。未证明驱动显存无增长，不将单次零域额度当长跑资源证明。

未验：新入口DXVA2/NVIDIA全矩阵、P/B帧硬解对拍、动态尺寸/参数与关闭竞争、4h/24h长跑、RTSP自动恢复、完整C设备诊断、H5推理选择。外部`.gitignore`与`.omc`保留、不提交；没有启动前后端或现场设备。下一步继续RTSP与H5，不改宿主固定依赖。
