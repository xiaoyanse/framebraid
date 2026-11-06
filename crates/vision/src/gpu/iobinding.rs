//! ORT IOBinding GPU 输入（M2b 3.5）：CUDA device 张量直达 CUDA EP。
//! 预处理 kernel 直接写入 ORT 分配的 device tensor（data_ptr），消除
//! host→device 往返；输出仍走默认 CPU 绑定（现有 result 路径不变）。
use crate::Error;
use ort::{
    memory::{AllocationDevice, Allocator, AllocatorType, MemoryInfo, MemoryType},
    session::{IoBinding, Session},
    value::{DynTensor, Tensor},
};

/// 输入张量与 IOBinding 的组合生命周期：tensor 按引用绑定，run_binding
/// 完成前整体必须存活（结构体一并持有，调用方保管到推理结束）。
/// 保活的 MemoryInfo/Allocator 与绑定共存亡：ORT IoBinding 内部按裸指针保留
/// 输出绑定的 OrtMemoryInfo，提前 Release 会令后续 run_binding/析构读写悬垂
/// 指针，堆腐坏表现为随机延迟段错误（M2b P0，实证见 fix 验证记录）。
/// 字段声明序即 Drop 序：binding 先析构（此刻上述指针均存活）。
pub struct InferenceInput {
    pub binding: IoBinding,
    tensor: Tensor<f32>,
    /// 词汇注入张量（批 B）：CPU 内存 OrtValue，保活到 run_binding 完成
    ///（binding 的 held_inputs 亦 AddRef，双保险）。
    vocab: Option<DynTensor>,
    _output_info: MemoryInfo<'static>,
    _allocator: Allocator,
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
        // run_binding 要求至少一个输出绑定（ORT："At least one output should be
        // requested"）；输出绑定到 CPU（提取路径与 CPU 推理一致）。
        let cpu = MemoryInfo::default();
        for output in session.outputs() {
            binding
                .bind_output_to_device(output.name(), &cpu)
                .map_err(|e| Error(format!("GPU 输出绑定失败|{e}")))?;
        }
        Ok(Self {
            binding,
            tensor,
            vocab: None,
            _output_info: cpu,
            _allocator: allocator,
        })
    }

    /// 绑定词汇注入张量到第二输入（批 B）：嵌入快照按 CPU 内存绑定——ORT
    /// IoBinding 支持混合 device/CPU 输入；首版无设备侧缓存，每次注入的
    /// H2D 搬运由 ORT 异步执行，调用方须在 run_binding 前
    /// synchronize_inputs()（见 execution.rs 批 B 注释）。数据复制进
    /// OrtValue 拥有的内存（TensorData 借用期不延伸到 run_binding，构造与
    /// run_ref 的 CPU 输入同构）；dtype/shape 取注入张量声明，失配由 ORT
    /// 执行期拒绝。
    pub fn bind_vocab(&mut self, tensor: &crate::sdk::TensorData) -> Result<(), Error> {
        let mut value = DynTensor::new(
            &Allocator::default(),
            crate::sdk::element(tensor.info.dtype)?,
            tensor.info.shape.clone(),
        )
        .map_err(|e| Error(format!("vision.inferenceFailed|vocab CPU 张量构造失败|{e}")))?;
        // 拥有者按 u64 对齐；字节长度已由 TensorData 按声明核对。
        unsafe {
            std::ptr::copy_nonoverlapping(
                tensor.bytes().as_ptr(),
                value.data_ptr_mut().cast(),
                tensor.bytes().len(),
            );
        }
        self.binding
            .bind_input(tensor.info.name.as_str(), &value)
            .map_err(|e| Error(format!("vision.inferenceFailed|vocab 输入绑定失败|{e}")))?;
        self.vocab = Some(value);
        Ok(())
    }

    /// device 数据指针（预处理 kernel 直接写入；借用期内不得释放）。
    pub fn device_ptr(&self) -> u64 {
        self.tensor.data_ptr() as u64
    }
}
