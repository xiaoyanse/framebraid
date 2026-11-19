//! 多引擎并列抽象：ORT 会话与 OpenVINO 原生引擎共用 Model 输入输出合同。
pub(crate) mod openvino;
pub(crate) mod ov_remote;
pub(crate) mod ov_sys;

/// Model 持有的执行引擎；ORT 分支沿用原 session.rs 路径，行为零改动。
/// Ov 分支为 Arc：remote 直传帧产物（ov_remote::OvRemoteFrame）持引擎引用
/// 保活——帧可能存活到会话关闭之后，tensors→remote 编译→core 的释放序由
/// 引用计数静态保证。
pub(crate) enum ModelEngine {
    Ort(ort::session::Session),
    Ov(std::sync::Arc<openvino::OpenvinoEngine>),
}
