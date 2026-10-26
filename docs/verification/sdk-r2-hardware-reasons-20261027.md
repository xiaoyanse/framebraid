# R2 硬解自动选择失败原因验证（2026-10-27）

基线为本地提交 `c78bf27`，本阶段只修改 FrameBraid，不发布，也不切换 AM-RT。自动选择此前将部分设备初始化原因压成通用输入错误；现保留编译不支持、适配器身份不匹配和原生设备初始化错误码，并维持既有 CPU 回退、资源许可与 C ABI 布局。JSON 原因可能从通用 Decoder 分类变为现有 Inventory 分类，调用方应接受未知分类及字段缺失。

## 验证证据

- 定向注入：未编译后端返回 `NotCompiled`，设备初始化返回 `DeviceInit(-12)`，均验证 CPU 回退；本机实际适配器 LUID 不匹配返回 `AdapterIdentityMismatch`。测试不操作现场相机。
- `am-camera` 包测试 66 通过、3 忽略，额外真实适配器身份测试单独通过；`am-camera-ffi` 83 通过、11 忽略；NAL 集成测试 2 通过。受影响包 clippy 和 Release DLL 构建通过；忽略项不记作通过。
- Release DLL 查询与直接解码：本机 Intel Arc 对 H.264 验证 D3D11VA 实际输出，H.265 Rext 样本硬件不兼容后恢复 CPU；查询长度、ABI 兼容与缓冲哨兵检查通过。
- 隔离 MediaMTX 合成 RTSP 重试通过：H.264/H.265 均实际 D3D11VA，MJPEG 为 CPU，压缩独占无解码、慢订阅重同步与帧交付检查通过。首次运行因发布端未就绪返回 404；第二次通过，不把首次失败隐去，也不将其认定为已定位的 SDK 故障。

## 边界和剩余

结果证明本机指定样本及注入场景，不能推断全部设备、码流或 GPU 资源收益。自动选择在无适配器时不输出完整逐后端编译清单，五层查询合同 H2.1 仍开放；低层 Rust `create_on_device` 仍返回原有通用 `DecodeError`。RTSP 驱动阻塞、关闭竞争及 MET-2 CPU 卸载与 GPU 资源取舍等定向验证仍由后续任务完成。相关原始日志在 `.local/evidence/r2-hardware-reasons-20261027`，RTSP 合成结果在 `.local/tests/r2-hardware-reasons-rtsp-retry-20261027`。
