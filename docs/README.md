# Documentation / 文档入口

[English README](../README.md) · [中文 README](../README.zh-CN.md)

| Need / 需求 | Document / 文档 |
| --- | --- |
| Build and run / 构建运行 | [English](GETTING_STARTED.md) · [中文](GETTING_STARTED.zh-CN.md) |
| Prebuilt Windows package / Windows 预编译包 | [Prebuilt integration / 接入](USE_PREBUILT.md) |
| Local artifacts and host upgrades / 目录与宿主升级 | [Local layout / 两仓维护](LOCAL_LAYOUT.md) |
| Source ownership / 源码维护源 | [Maintenance boundary / 维护边界](SOURCE_MAINTENANCE.md) |
| Full lifecycle and language integration / 完整生命周期及接入 | [Integration / 接入说明](../sdk/INTEGRATION.md) |
| Runnable examples / 可运行示例 | [Examples / 示例](../sdk/examples/README.md) |
| Platform/feature limits / 平台与能力限制 | [Support / 支持边界](SUPPORT.md) |
| Implementation goals and tasks / 实施目标与任务 | [SDK OpenSpec](../openspec/README.md) |
| Future priorities / 后续优化路线 | [Roadmap / 分阶段路线图](ROADMAP.md) — prioritized performance work, reference index and quantitative gates / 性能优先排期、参考索引与量化验收；尚未实施 |
| Current validation / 当前验证 | [Local Release status / 本地 Release 状态](RELEASE_STATUS.md) |
| Charts and numeric observations / 图表及数值观察 | [Test observations / 测试资料](BENCHMARKS.md) |
| Reproducible SDK performance tools / 可复现基准工具 | [Performance measurement / 性能测量](PERFORMANCE.md) — local tools, boundaries and current limitations / 本地工具、计时边界与未验项 |
| Visual identity / 视觉标识 | [Selected logo and banner / 已选 Logo 与横幅](../assets/brand/README.md) |
| Future publication / 后续发布 | [Release checklist / 发布清单](RELEASE_CHECKLIST.md) |
| Contribute/security / 贡献与安全 | [Contributing](../CONTRIBUTING.md) · [Security](../SECURITY.md) |
| Licensing/materials / 许可材料 | [NOTICE](../NOTICE) · [Third-party materials](../sdk/third-party/README.md) |

## Reference contracts / 接口参考

- [Media C API](../sdk/include/am_camera.h), [Vision C API](../sdk/include/am_vision.h), [shared types](../sdk/include/am_sdk_types.h).
- [ABI design](design/dual-dll-abi.md) and [type contracts](design/dual-dll-abi-types.md).
- [Local ABI 1.1 extension](design/dual-sdk-abi-freeze-v1.1.md) and [probe/report verification](verification/sdk-r2-abi11-hardware-20261026.md); published v0.1.0 remains ABI 1.0.
- [Delivery boundaries](design/dual-dll-sdk.md).
- [Native hardware discovery / 原生硬件发现](design/native-hardware.md) — local Rust work, separate from frozen C ABI and public CPU assets.
- [Shared frames](design/dual-sdk-shared-frames.md) and [layout](design/dual-sdk-shared-layout-v1.md).
- [Model profiles](../crates/vision/resources/profiles.json).

Design documents preserve earlier technical decisions, host-project paths, review states and future plans. They are not evidence that every described feature has been accepted. Use exported interfaces and implementation for actual capabilities, and validation records for tested scope. Report contradictions rather than treating planned features as implemented.

设计文档保留历史决策、宿主路径、评审状态和后续规划，不证明其描述都已经验收，也不要求安装原业务项目。实际能力核接口与实现，实测范围核验证记录，冲突登记复查。

## Layout and provenance / 目录与来源

The source checkout uses `sdk/include`, `sdk/bindings` and `sdk/examples`. Older flat archives are a different layout. Set `SDK_ROOT` to the actual directory containing these three folders.

当前源码仓库保留 `sdk/` 层级，旧平铺包不混用路径。

`sdk-source-manifest.json` records the original extraction, not a continuously regenerated release manifest. Documentation can evolve without rewriting provenance. Updating core fingerprints requires an explicit source-baseline review. Future releases need separate source commits and artifact checksums.

来源清单记录历史提取；文档演进不回写历史。核心指纹升级须审来源基线，正式发布另建版本与制品清单。

Local unpublished r7: [RTSP hardware verification](verification/sdk-r2-rtsp-hardware-20261026.md) covers shared policy, actual D3D11VA and CPU/MJPEG controls. Hardware availability is separate from speed; reader isolation, full hardware/pressure/soak acceptance and H5 remain open. Published v0.1.0 is unchanged. / 本地候选与旧公开CPU制品区分，H2整体未关闭。
