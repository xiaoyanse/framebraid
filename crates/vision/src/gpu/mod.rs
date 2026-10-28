//! GPU 帧直传（M2b）：CUDA 互操作与 GPU 预处理；E1 引擎适配位见合同 §3.3a。
pub mod cuda_sys;
pub mod driver_sys;
pub mod interop;
#[cfg(test)]
mod interop_tests;
#[cfg(test)]
mod preprocess_tests;
