//! 相机接入、编码、解码与有界分发核心；无任务、工位、报警或凭据存储依赖。
pub mod credentials;
pub mod device_addresses;
pub mod frame_budget;
pub mod media;

/// 公共媒体错误；宿主自行映射到业务错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
