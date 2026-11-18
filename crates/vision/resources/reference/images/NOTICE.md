# 参考图来源登记（NOTICE）

本目录图片为 ABI 合同（docs/design/dual-dll-abi.md §4.2 参考结果与容差表）要求的档案参考图，仅用于 SDK 验收对照测试。

| 文件 | SHA-256 | 来源 | 许可事实 |
|---|---|---|---|
| bus.jpg | c02019c4979c191eb739ddd944445ef408dad5679acab6fd520ef9d434bfbc63 | https://github.com/ultralytics/assets/releases/download/v0.0.0/bus.jpg | ultralytics assets 仓库生态为 AGPL-3.0 系许可；本图按原字节入库仅作测试参考图，许可风险由决策人于 2026-11-18 直批承接，如实标注不洗白 |
| zidane.jpg | 16d73869e3267a7d4ed00de8e860833bd1657c1b252e94c0c348277adc7b6edb | https://github.com/ultralytics/assets/releases/download/v0.0.0/zidane.jpg | 同上（AGPL-3.0 生态，测试参考用途，决策人承接） |
| boats.jpg | 8c5ada657cf8110a9f8aaac954c1dd96cde0187315b581276c32b0d1863e756f | https://github.com/ultralytics/assets/releases/download/v0.0.0/boats.jpg | 同上（AGPL-3.0 生态，OBB 航拍示例，ultralytics 官方 docs/en/tasks/obb.md predict 示例引用源，决策人承接） |

- detect-yolo11n-nms-v1、segment-yolo11n-nms-v1、classify-yolo11n-prob-v1 三档案共用 bus.jpg（ultralytics 官方 docs detect/segment/classify 任务页 predict 示例图，逐页引用 https://ultralytics.com/images/bus.jpg，本仓取其 assets 仓库原件）。
- pose-yolo26-e2e-coco17-v1 用 zidane.jpg（任务书指定官方示例图）。
- obb-yolo11n-nms-v1 用 boats.jpg（官方 OBB 文档示例图）。
- 下载时间 2026-11-19；SHA-256 为入库原件实测。
