# Tasks

- [x] 3.1 合同：dual-dll-abi.md 新增 Vision GPU 帧输入章节（frame_kind 协商、能力位、搬运审计计数器、回退矩阵）+ gpu-pipeline.md L4 落地记录
- [x] 3.2 生成器模型更新 + 同步 C 头/Rust ABI/Python/C# 绑定，--check 全绿（rust-abi 布局基线复用改造：本机 gcc 环境故障下的逃生口，手工布局经 Rust layout_tests 83 项独立验证）
- [x] 3.3 CUDA 互操作纹理注册：cudaGraphicsD3D11RegisterResource + map/unmap 生命周期（借用期内注册、归还前 unmap；NVIDIA 真纹理实测通过；修复 D3D11 库句柄生命周期导致的 vtable 悬空崩溃）
- [x] 3.4 GPU 预处理 kernel：NV12→RGB NCHW + letterbox（手写 PTX 零工具链依赖：nvcc 缺 host 编译器、NVRTC 违反合同，ptxas 本地校验+驱动 JIT 加载）；真实 GPU 对拍 CPU 参考实现全像素一致（修复通道地址算术/G 符号/pad 常量/双 clamp 越界，compute-sanitizer 定位）
- [ ] 3.5 ORT IOBinding 绑定：GPU 输入张量直达 CUDA EP（复用 M1 预研验证路径）；输出按现有 result 路径
- [ ] 3.6 amv GPU 帧输入接线：infer 输入接受 GPU 帧（同进程纹理指针直传）+ frame_kind 协商 + 回退矩阵（GPU 不可用回 CPU 像素，单帧回退计数）
- [ ] 3.7 单测：协商/回退/审计计数/借用期内 unmap/SOFTWARE_ONLY
- [ ] 3.8 端到端实测：M2a 合成 H.264→D3D11VA→GPU 帧→CUDA 预处理→推理，深度对拍 CPU 路径（检测框/置信度一致）+ 搬运审计数字（每帧拷贝次数对比）
- [ ] 3.9 验证记录 + fmt/clippy/workspace 回归 + 冻结/边界检查 + 本地提交
