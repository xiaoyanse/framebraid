//! SDK 网络原语：不依赖业务设备、凭据持久化或宿主全局运行库。
mod digest;
pub(crate) mod http;
mod nal;
mod quiet;
mod rtsp;
#[cfg(test)]
mod tests;

pub use http::{capture, decode_image};
pub use rtsp::{Packet, Reader, StreamInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoError {
    Invalid,
    Timeout,
    Auth,
    Network,
    Unsupported,
    Resource,
    Closed,
}

pub struct Image {
    pub encoded: Vec<u8>,
    pub rgba: Option<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

pub fn validate_url(url: &str, isapi: bool) -> Result<(), IoError> {
    if url.len() > 16 * 1024 || url.chars().any(char::is_control) {
        return Err(IoError::Invalid);
    }
    let parsed = reqwest::Url::parse(url).map_err(|_| IoError::Invalid)?;
    let valid_scheme = if isapi {
        matches!(parsed.scheme(), "http" | "https")
    } else {
        matches!(parsed.scheme(), "rtsp" | "rtsps")
    };
    let authority = url
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or(""));
    if !valid_scheme
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
        || parsed.port() == Some(0)
        || authority.is_some_and(|part| part.contains('@'))
    {
        return Err(IoError::Invalid);
    }
    Ok(())
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn rejects_unsafe_urls() {
        for url in [
            "file:///etc/passwd",
            "http://u:p@localhost/a",
            "http://localhost/a#b",
        ] {
            assert_eq!(validate_url(url, true), Err(IoError::Invalid));
        }
        assert!(validate_url("http://[::1]:8080/ISAPI/a?x=1", true).is_ok());
        assert!(validate_url("rtsp://localhost:8554/live", false).is_ok());
        assert_eq!(
            validate_url("http://localhost/a", false),
            Err(IoError::Invalid)
        );
    }
}
