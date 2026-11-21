//! 可独立使用的数值模型会话；不读取宿主配置、不运行规则或报警。
mod batch;
pub mod batching;
pub mod cascade;
pub mod device;
pub mod encoded;
mod model;
mod onnx;
pub mod pixels;
mod prepare;
pub mod runtime;
pub mod selection;
mod session;
pub mod tracker;
pub mod vocab;
pub use batch::validate_count as validate_batch_count;
pub use model::{
    byte_count, validate_input_infos, DeviceTensor, Model, ModelSource, TensorData, TensorInfo,
};
// OV remote 直传帧产物（ABI 1.8 GPU 帧输入 OV 臂）：FFI 层 Input 持有跨调度
// 线程移动；构造/执行经 Model::capture_ov_remote / run_ov_remote。
// IntelTestDevice/intel_nv12_texture_upload 为 e2e 测试辅助
// （interop::nvidia_nv12_texture 先例）。
pub use crate::engine::ov_remote::{
    intel_nv12_texture_upload, intel_test_device, IntelTestDevice, OvRemoteFrame,
};
// dtype→ORT 元素类型映射仅供 crate 内输入构造共享（GPU 绑定与 run_ref 同构）。
pub(crate) use model::element;
pub use prepare::{prepare, ImageTransform, Preprocess};
pub use vocab::{encode as vocab_encode, VocabEmbedding};
#[cfg(test)]
mod batch_tests;
#[cfg(test)]
mod batching_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod tests;

pub use runtime::initialize;
