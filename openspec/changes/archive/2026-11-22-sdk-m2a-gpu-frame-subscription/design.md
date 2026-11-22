# Design: M2a — GPU 帧订阅与显存帧池

## GPU 帧对象（L1）

不透明句柄 `amc_gpu_frame_t`（对象句柄制，同 amc_shared_pool_t 风格）：
- 元数据：宽/高/像素格式（D3D 格式枚举映射）/subresource 索引/时间戳/帧身份（复用 am_frame_assoc_t）
- 内部持有：ID3D11Texture2D 引用 + 共享 NT 句柄（创建时带 D3D11_RESOURCE_MISC_SHARED_NTHANDLE，FFmpeg hwcontext 默认已带 MISC_SHARED；若个别纹理缺标志则该帧降级回读并在元数据标注 fallback=1，不失败）
- 租约：借出/归还语义与共享帧池一致（跨 DLL 借用经 ABI 句柄传递）

## 订阅模式

`am_frame_sub_desc_t` 新增 `frame_kind`（默认=像素，兼容旧行为；GPU=GPU 帧交付）。GPU 订阅拉取返回 GPU 帧句柄。硬解策略沿用现有 SOFTWARE_ONLY/PREFER_HW——GPU 帧仅在硬解实际激活时可用（SOFTWARE_ONLY 下 GPU 订阅按合同报不支持或降级像素，合同定死：报 unsupported，不静默降级）。

## 显存帧池

- 复用共享池骨架（sdk-support/shared）按资源类目分账：新增 GPU 帧池上限（池数/帧数/显存字节），计费进既有 Charge 体系新类目（gpu_frame_bytes）
- 池满/预算不足：拒绝（与 CPU 池同语义），不覆写活帧
- 关闭：排空 GPU 借出后释放（与共享池 1.5b 语义对齐）

## ABI 面（兼容新增）

- `amc_gpu_frame_pool_create` / `amc_gpu_frame_borrow` / `amc_gpu_frame_return` / `amc_gpu_frame_release`（对象销毁）
- GPU 帧视图查询（元数据 + 共享 NT 句柄值，供 Vision DLL 互操作；同进程约束注明）
- 能力位：AMC_CAP_GPU_FRAMES
- 订阅结构体：尾部追加 frame_kind 字段（struct_size 版本协商）

## 搬运审计挂钩

池/订阅路径预留计数器（gpu_frames_delivered / cpu_fallbacks / bytes_transferred_est），M2c 验收时经统计接口读出。

## 不做（边界）

- Vision 侧接线（IOBinding/CUDA 互操作消费）= M2b
- 不做跨进程（同进程跨 DLL 先行）
- 不做编码器（F/⑤ 另立）
