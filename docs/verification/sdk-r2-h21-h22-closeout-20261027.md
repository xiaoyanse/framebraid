# R2 H2.1/H2.2 父项收口核对（2026-10-27）

本记录为既有实现与既有证据的合同对照收口，不新增运行验收；引用的实现与实测均来自此前已提交记录，源码以 `selection.rs` 当前主干为准。核对基于提交 `d446db3` 时点。

## H2.1 硬解策略与查询合同五层状态核对

| 层 | 实现位置 | 合同/报告字段 | 实测证据 |
| --- | --- | --- | --- |
| 编译支持 | `media::hardware::compiled`（`avcodec_get_hw_config` 逐编码实查） | probe `selection.backend_support[].compiled`（null=未检查/false=明确不支持） | [H2.0](sdk-r2-hardware-foundation-20261026.md)：旧包只有 AD_HOC 入口被逐编码实查发现并修正，不以设备枚举代替配置检查 |
| 设备发现 | `hardware::adapters()` DXGI 枚举（≤8 适配器，过滤软件适配器，同名按 LUID 独立） | attempts 的 `adapter_index/adapter_luid`；LUID 无符号不与 CUDA ordinal 混同 | H2.0：多条同名 Intel 适配器按独立 LUID 保存 |
| 依赖 | `system_library` System32 按需加载，失败 `DependencyMissing(库名)` | 经 `SelectionFailure::Inventory` 失败原因携带，不单列字段（合同注明 null/false/未知语义） | H2.0：System32 按需加载已核，CPU DLL 无隐式装载依赖 |
| 初始化 | `Device::create` + `create_inner`，适配器 LUID 身份核对 | attempts 分类（初始化/目标配置/资源/预算/忙）+ `selected`；probe 报告 `actual_backend` 恒 null（设备已释放） | H2.0：Intel Arc 140T 与 RTX 5060 D3D11 初始化成功；NVIDIA 无 D3D9 映射时 DXVA2 明确拒绝 |
| 实际码流解码 | decoder 输出真实硬件帧后 `hardware_backend` | decoder 报告 `actual_backend` + state `initialized/verified/recovered_cpu/closed`；`last_failure` 保留原生错误 | H2.0：合成 H.264/H.265 Main 实际输出硬件帧，YUV 对拍误差 0；ABI 1.1：HEVC Rext 拒绝后 CPU 恢复且保留原生错误 |

无设备分支（[H2.1b](sdk-r2-backend-support-20261027.md)）：无适配器时 `backend_support` 仍分别报告两后端编译支持与目标编码配置，未知不冒充不支持。ABI 1.1 为兼容新增：旧 1.0 冻结原字节保留于 `sdk/abi/history/freeze-v1-r5.json`，合同 §1.1 已注明"现有客户端须忽略未知 JSON 字段"。H2.1 声明的"新增字段或策略先修合同和生成器"在此前 ABI 1.1 交付时已按流程执行（合同先行、生成器同步、C/C#/Python 验证）。

**结论：H2.1 收口。** 五层状态、适配器身份与失败原因在合同与实现中分层齐备，逐项有实测或注入证据；probe 的 attempts 为"首个成功即止"非全量清单（合同已注明），完整源/设备选择属 H2.3 范围。

## H2.2 Windows H.264/H.265 探测与自动选择核对

| H2.2 要求 | 实现位置 | 证据 |
| --- | --- | --- |
| 优先 D3D11VA | `selection.rs` 后端顺序 `[D3d11va, Dxva2]` 按序尝试 | ABI 1.1 实测 H264/HEVC 初始化选中 D3D11VA |
| 构建支持时尝试 DXVA2 | `compiled` + 适配器 `dxva2_index` 双重检查，无映射拒绝并保留原因 | H2.0：Intel 映射 DXVA2 实际输出硬件帧；NVIDIA 无映射明确拒绝 |
| 枚举实际适配器并受预算限制 | `enter_probe` 全局准入（同进程一轮、16 次尝试上限）+ 全程 budget 检查（上限 5s、超时即回退） | H2.0：探测 6 原生引用 6/6；[H2.0b](sdk-r2-hardware-selection-20261026.md)：探测忙与无硬件回退定向测试 |
| 不只凭显卡名称或编译能力判可用 | 实际 `Device::create` + D3D11 LUID 身份核对成功才算选择；同名适配器不去重 | H2.0：身份不匹配注入测试；LUID 独立保存 |

**结论：H2.2 收口。** CUDA/QSV 专用视频后端按任务原文"后续"处理，不在本项范围。

## 边界与未验

本核对不改变任何支持范围承诺；probe/初始化成功不等于实际码流硬解（由 decoder verified 状态判定）。硬件矩阵非强制门槛；本机仅 Intel Arc 140T 与 RTX 5060，其他设备未测不冒充。H2.3/H2.4 的恢复、关键帧、关闭竞争与 MET-2 资源对照仍按父项开放。
