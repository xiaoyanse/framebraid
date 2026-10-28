//! ORT IOBinding GPU 输入（M2b 3.5）：CUDA device 张量直达 CUDA EP。
//! 预处理 kernel 直接写入 ORT 分配的 device tensor（data_ptr），消除
//! host→device 往返；输出仍走默认 CPU 绑定（现有 result 路径不变）。
use crate::Error;
use ort::{
    memory::{AllocationDevice, Allocator, AllocatorType, MemoryInfo, MemoryType},
    session::{IoBinding, Session},
    value::Tensor,
};

/// 输入张量与 IOBinding 的组合生命周期：tensor 按引用绑定，run_binding
/// 完成前整体必须存活（结构体一并持有，调用方保管到推理结束）。
pub struct InferenceInput {
    pub binding: IoBinding,
    tensor: Tensor<f32>,
}

impl InferenceInput {
    /// 在 session 的 CUDA device 上分配 [1, 3, h, w] f32 输入张量并绑定。
    pub fn new(session: &Session, input_name: &str, h: usize, w: usize) -> Result<Self, Error> {
        let memory = MemoryInfo::new(
            AllocationDevice::CUDA,
            0,
            AllocatorType::Device,
            MemoryType::Default,
        )
        .map_err(|e| Error(format!("CUDA MemoryInfo 构造失败|{e}")))?;
        let allocator = Allocator::new(session, memory)
            .map_err(|e| Error(format!("CUDA allocator 获取失败|{e}")))?;
        let tensor = Tensor::<f32>::new(&allocator, [1usize, 3, h, w])
            .map_err(|e| Error(format!("CUDA 输入张量分配失败|{e}")))?;
        let mut binding = session
            .create_binding()
            .map_err(|e| Error(format!("IOBinding 创建失败|{e}")))?;
        binding
            .bind_input(input_name, &tensor)
            .map_err(|e| Error(format!("GPU 输入绑定失败|{e}")))?;
        Ok(Self { binding, tensor })
    }

    /// device 数据指针（预处理 kernel 直接写入；借用期内不得释放）。
    pub fn device_ptr(&self) -> u64 {
        self.tensor.data_ptr() as u64
    }
}
