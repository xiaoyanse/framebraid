# Tasks

- [x] 3.1 合同：dual-dll-abi.md 新增 Vision GPU 帧输入章节（frame_kind 协商、能力位、搬运审计计数器、回退矩阵）+ gpu-pipeline.md L4 落地记录
- [x] 3.2 生成器模型更新 + 同步 C 头/Rust ABI/Python/C# 绑定，--check 全绿（rust-abi 布局基线复用改造：本机 gcc 环境故障下的逃生口，手工布局经 Rust layout_tests 83 项独立验证）
- [x] 3.3 CUDA 互操作纹理注册：cudaGraphicsD3D11RegisterResource + map/unmap 生命周期（借用期内注册、归还前 unmap；NVIDIA 真纹理实测通过；修复 D3D11 库句柄生命周期导致的 vtable 悬空崩溃）
- [x] 3.4 GPU 预处理 kernel：NV12→RGB NCHW + letterbox（手写 PTX 零工具链依赖：nvcc 缺 host 编译器、NVRTC 违反合同，ptxas 本地校验+驱动 JIT 加载）；真实 GPU 对拍 CPU 参考实现全像素一致（修复通道地址算术/G 符号/pad 常量/双 clamp 越界，compute-sanitizer 定位）
- [x] 3.5 ORT IOBinding 绑定：CUDA allocator device tensor + bind_input + run_binding 封装（InferenceInput 组合生命周期）；真模型对拍验证并入 3.8 端到端
- [x] 3.6 amv GPU 帧输入接线：AM_IMG_GPU_FRAME 分支（仅 PROFILE 模式，raw 自定义预处理回退）+ 显式 notSupported 矩阵（非 CUDA 后端/管线不可用不静默）+ run_gpu 全链（互操作→D2D→kernel→IOBinding→decode 同 CPU 路径）；异步 GPU 输入后续批次
- [x] 3.7 单测：plan GPU 分支校验 + map 计数生命周期（3.3 实测）+ SOFTWARE_ONLY（M2a 侧）+ notSupported 矩阵（3.6 代码路径）；审计计数断言在 e2e
- [ ] 3.8 端到端实测：M2a 合成 H.264→D3D11VA→GPU 帧→CUDA 预处理→推理，深度对拍 CPU 路径（检测框/置信度一致）+ 搬运审计数字（每帧拷贝次数对比）
- [ ] 3.9 验证记录 + fmt/clippy/workspace 回归 + 冻结/边界检查 + 本地提交
