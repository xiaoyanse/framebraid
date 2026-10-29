# RTSP 探测修复验证（2026-10-29）

基线 656c8ac8e64c47c4d73562fdc2bd5a92f5d5e019。SDP 已知非 H.264 无需尺寸即报告不支持；未知编码或 H.264 缺尺寸时，流信息探测临时使用 256 KiB、500 ms 分析预算，结束恢复打开参数。C ABI 无变化。

本机随机回环端口的 RTSP fixture 返回无尺寸 MJPEG SDP，验证 codec ID 7 与探测阶段停止信号响应，测试耗时 0.38 秒；本地 MJPEG 文件探测通过。分类单测、am-camera clippy、source-fingerprint 测试、check-boundary（240 文件）通过。未连接现场设备。FFmpeg 回调覆盖打开及探测 IO；系统 DNS 解析和独立 TCP 预连接不受该回调控制，前者无本包保证的上限、后者最多等待 5 秒且不能立即取消，不将整体连接记为有界取消验收。

固定基线已有 3 项 R7 修改的指纹登记滞后，已逐项核对 656c8ac 原始 blob 和 fb254bf 变更，原登记与实际摘要留在 sdk-source-revisions.json；http 旧登记来源未明。未更改原始清单或验证算法。
