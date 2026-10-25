# R2/H2 原生硬解基础阶段记录（2026-10-26）

范围：FrameBraid main，评审基线`6134e1c7ed0ff8bd860dbb2775bf0aa979280f84`。仅SDK与本地合成数据；未修改AM-RT、未连接相机、未替换发布资产。本阶段为H2.0，不等于R2/H2整包完成。

## 已实现与实测

- 显式DXGI枚举、D3D9序号/LUID映射、D3D11实际设备LUID核对；最多8适配器、16次探测尝试、同时1轮。预算为调用间软预算。
- 新增显式Rust设备解码与CPU像素回读；默认CPU/C ABI保持原行为。只在收到真实硬件帧后报告后端。稳定Box回调、独立设备引用、待输出/回读帧在关闭时释放。
- 本机Intel Arc 140T与NVIDIA RTX 5060 Laptop的D3D11初始化成功；探测6个原生引用创建/归还为6/6，注入“失败但返回非空引用”仍6/6。多条同名Intel适配器按独立LUID保存，不按名称臆断去重。NVIDIA无D3D9 LUID映射时DXVA2明确拒绝，不盲用DXGI序号。
- 1920×1080合成H.264和H.265 Main，每条成功路径重复单AU12次，完整输出PTS 0～11。Intel与NVIDIA D3D11、Intel映射DXVA2实际输出硬件帧；YUV首帧对拍CPU的最大/平均误差均0。每条成功路径验证小输出预算拒绝，并注入一次回读错误后重试同一待输出帧，PTS不丢。

证据位于`.local/evidence/h2-*20261026.json`；运行库身份、样本/工具摘要与命令在`h2-foundation-manifest-20261026.json`。新运行库仅在`.local/runtimes/ffmpeg-h2-20261026`，旧包原字节保留。

## 排查出的两个真实边界

1. 旧FFmpeg包配置只有`h264_d3d11va/hevc_d3d11va`旧AD_HOC入口；虽然D3D11设备初始化成功，`avcodec_get_hw_config`没有device type=7且methods含HW_DEVICE_CTX的项。新构建增加`h264_d3d11va2/hevc_d3d11va2`，并在构建后逐编码实查配置，不以设备枚举代替。另去除阻止首次生成version script的make `-W`参数，干净构建和续建均采用正常依赖生成；保留原FFmpeg源码与LGPL材料。
2. 首个HEVC样本为Rext profile，被硬件拒绝。另生成并用ffprobe确认Main profile后，两块显卡均成功。不能将不支持profile直接报成“没有显卡”，自动回退的细化诊断仍须H2后续实现。

RGBA观察：H.264平均绝对差0.591、最大91；H.265 Main平均0.551、最大93。转换前YUV完全一致，差异发生于平面YUV与NV12输入到swscale的转换路径。本次没有放宽解码内容验收来掩盖差异，也不宣称CPU/HW RGBA逐字节一致；后续需按像素/模型容差审查一致转换策略。示例含观测拷贝与故障注入，其耗时不能当性能收益。单AU不证明P/B帧与生产视频通过。

## 验证入口

- `python -m unittest discover -s scripts/sdk -p test_ffmpeg_hardware_config.py`：4项通过，含旧AD_HOC误配置拒绝及HEVC独立检查。
- `cargo test -p am-camera --lib`：设置本地`AM_RT_FFMPEG_DIR`后47通过、1显式样本测试单列；另以256×144红色H.264/HEVC样本显式运行该测试，1项通过。第一次未设置路径的6项失败原因为`media.ffmpegDirMissing`；按当前候选运行库重跑通过，不当成功略过。
- 硬件基础测试3项通过；`cargo clippy -p am-camera --all-targets -p framebraid-sdk --examples -- -D warnings`、fmt通过；ABI冻结8项/生成121函数检查、来源165文件检查、OpenSpec五包严格校验通过。
- 原CPU路径合成输入FNV仍为`8816365bfed01b95`，与R1一致。该10帧检查只证明输出未漂移，性能三轮对比仍未完成。

自查：未发现本阶段阻断项；回调地址、设备/帧引用、System32加载、D3D9映射及实际D3D11身份已核。警告项为RGBA转换路径差异、原生调用软超时和尚未接入全局预算，已明确保留H2后续任务；本阶段不启用自动路径。较长的COM接线和合成验收示例按职责隔离，不混入业务或通用C头。

## 未完成与交接顺序

H2.1～H2.4保持未勾选：C ABI诊断演进、自动选择/回退、RTSP像素入口、回读全局预算、P/B帧/profile/位深/尺寸变化、有界恢复/重试、关闭竞争、CPU/显存与三轮端到端量化均未整体验收。推理H5、R2热路径、R3故障长跑、R4模型/语言、R5组批与24小时验证继续，目标不缩小。

公开Windows CPU支持范围未升级；Rust新原语供后续接线，C#/Python现有入口不因此自动硬解。外部`.gitignore`改动保留，不纳入本次提交。
