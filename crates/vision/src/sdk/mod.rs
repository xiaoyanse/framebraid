//! 可独立使用的数值模型会话；不读取宿主配置、不运行规则或报警。
mod batch;
pub mod encoded;
mod model;
mod onnx;
pub mod pixels;
mod prepare;
pub mod runtime;
pub mod selection;
mod session;
pub use batch::validate_count as validate_batch_count;
pub use model::{byte_count, validate_input_infos, Model, ModelSource, TensorData, TensorInfo};
pub use prepare::{prepare, ImageTransform, Preprocess};
#[cfg(test)]
mod batch_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod tests;

pub use runtime::initialize;
