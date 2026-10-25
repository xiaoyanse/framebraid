# Security / 安全问题报告

## Supported scope / 支持范围

Version 0.1.0 is a developer preview for Windows x64 CPU. No response-time SLA
or safety certification is declared. Maintainer: [xiaoyanse](https://github.com/xiaoyanse).

0.1.0 是 Windows x64 CPU 开发者预览版，不承诺响应时限或安全认证。

## Private reporting / 私密报告

Use [GitHub private vulnerability reporting](https://github.com/xiaoyanse/framebraid/security/advisories/new).
This feature is enabled. Do not disclose exploitable vulnerabilities, credentials,
private stream URLs or customer images in public issues or pull requests.

请通过上述 GitHub 私密漏洞报告入口提交，已启用。不要在公开 Issue/PR 披露可利用漏洞、
凭据、私有取流地址或客户图片。

Include affected SDK/ABI, platform/runtime/backend, impact and a sanitized minimal
reproduction, then coordinate disclosure privately.

提供受影响版本、平台、依赖、影响及脱敏最小复现，私下协商披露。

## Integration practices / 接入建议

- Treat streams, models and metadata as untrusted input; avoid privileged execution of untrusted models/native libraries.
- Use explicit runtime directories and trusted sources; preserve error detail without secrets.
- Shared frames assume trusted local processes, not hostile-tenant isolation.
- Bound queues/resources, respect ownership and drain before unload.
- The host owns network policies, credential persistence, user authorization and workload isolation.

流、模型与元数据按不可信输入处理，限制权限并核运行库来源。共享帧不代替租户隔离，宿主负责网络、凭据、授权及负载隔离；遵守资源/拥有权与退出约定。

See [support boundaries](docs/SUPPORT.md). Local tests are not security certification.
