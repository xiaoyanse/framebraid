//! 多引擎并列抽象：ORT 会话与 OpenVINO 原生引擎共用 Model 输入输出合同。
pub(crate) mod openvino;
pub(crate) mod ov_sys;

/// Model 持有的执行引擎；ORT 分支沿用原 session.rs 路径，行为零改动。
pub(crate) enum ModelEngine {
    Ort(ort::session::Session),
    Ov(openvino::OpenvinoEngine),
}
