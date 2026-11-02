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

/// FILE 源路径校验失败详情（P1-8 §7 编码约定：路径 + OS 错误码 + 原因）。
pub struct FileCheckError {
    pub path: String,
    pub os_error: Option<i32>,
    pub reason: &'static str,
}
impl FileCheckError {
    pub fn detail(&self) -> String {
        match self.os_error {
            Some(code) => {
                format!(
                    "path={:?}, os_error={}, reason={}",
                    self.path, code, self.reason
                )
            }
            None => format!("path={:?}, reason={}", self.path, self.reason),
        }
    }
}

/// FILE 源路径校验（P1-8 阶段 1）：剥 `file://` 前缀，存在性/可读性探测，
/// 返回规范化路径。空路径/控制字符/不存在/非普通文件/不可读分别拒绝并带详情。
pub fn validate_file_path(url: &str) -> Result<String, FileCheckError> {
    let path = url
        .strip_prefix("file:///")
        .or_else(|| url.strip_prefix("file://"))
        .unwrap_or(url);
    let fail = |os_error: Option<i32>, reason: &'static str| FileCheckError {
        path: path.to_string(),
        os_error,
        reason,
    };
    if path.is_empty() || path.len() > 16 * 1024 || path.chars().any(char::is_control) {
        return Err(fail(None, "invalid path"));
    }
    let metadata = std::fs::metadata(path)
        .map_err(|error| fail(error.raw_os_error(), "metadata probe failed"))?;
    if !metadata.is_file() {
        return Err(fail(None, "not a regular file"));
    }
    std::fs::File::open(path).map_err(|error| fail(error.raw_os_error(), "not readable"))?;
    Ok(path.to_string())
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
