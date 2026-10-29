//! MJPEG over HTTP 流读取器（R7）：长连接 + Digest 复用 + boundary 分帧。
//! 双超时（合同）：无数据（读超时/对端黑洞）与无 boundary 进展（恶意无界流，
//! 经解析器 bytes_without_progress 配合时间窗判定）；连续超限由解析器断连
//! 信号触发重建（调用方退避重连）。
use crate::credentials::Secret;
use std::collections::VecDeque;
use std::io::Read;
use std::time::{Duration, Instant};

use super::multipart::{parse_boundary, MultipartParser, Output, StreamFormatError};
use super::sdk_io::http::{network, remaining};
use super::sdk_io::IoError;

/// 无 boundary 进展判定窗（窗内累计字节超阈值即断流重连）。
const NO_PROGRESS_WINDOW: Duration = Duration::from_secs(15);
const NO_PROGRESS_BYTES: usize = 8 * 1024 * 1024;
/// 单次读块大小。
const CHUNK: usize = 64 * 1024;

pub struct MjpegStream {
    response: reqwest::blocking::Response,
    parser: MultipartParser,
    /// feed 一次可产出多帧；逐次返回，未交付帧暂存于此。
    pending: VecDeque<Output>,
    progress_deadline: Instant,
    byte_budget: usize,
    /// Content-Length 谎报累计（诊断导出）。
    false_content_length: u64,
    discarded: u64,
}

impl MjpegStream {
    /// 建连：Digest 认证复用 http 快路径（challenge 缓存命中时单事务）；
    /// 非 multipart 响应报 Unsupported。
    pub fn open(url: &str, secret: &Secret, timeout: Duration) -> Result<Self, IoError> {
        super::sdk_io::validate_url(url, true)?;
        if timeout.is_zero() {
            return Err(IoError::Invalid);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(IoError::Invalid)?;
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .build()
            .map_err(network)?;
        // 复用 Digest challenge 缓存：直接授权请求（快路径）；401 回退由
        // http 模块快路径语义承担（此处简化为一次匿名探测 + 授权重发，
        // nonce 复用场景下后续建连单事务化由 challenge 缓存达成）。
        let mut response = client
            .get(url)
            .timeout(remaining(deadline)?)
            .send()
            .map_err(network)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            response =
                super::sdk_io::http::authorized_retry(url, secret, &client, response, deadline)?;
        }
        if matches!(response.status().as_u16(), 401 | 403) {
            return Err(IoError::Auth);
        }
        if !response.status().is_success() {
            return Err(IoError::Network);
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let boundary = parse_boundary(&content_type).map_err(format_error)?;
        Ok(Self {
            response,
            parser: MultipartParser::new(&boundary),
            pending: VecDeque::new(),
            progress_deadline: Instant::now() + NO_PROGRESS_WINDOW,
            byte_budget: NO_PROGRESS_BYTES,
            false_content_length: 0,
            discarded: 0,
        })
    }

    /// 下一帧：内部循环读块喂解析器；Ok(None)=流正常收尾（close 分隔符）。
    /// 双超时/超限断连返回 Timeout/Unsupported（调用方退避重建）。
    pub fn next_frame(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, IoError> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(IoError::Invalid)?;
        let mut chunk = [0u8; CHUNK];
        loop {
            // 先交付暂存帧；一次 feed 可产出多帧，返回第一帧其余入队等待。
            if let Some(output) = self.pending.pop_front() {
                match output {
                    Output::Frame(frame) => {
                        self.progress_reset();
                        return Ok(Some(frame));
                    }
                    Output::Discarded => self.discarded += 1,
                    Output::FalseContentLength => self.false_content_length += 1,
                    Output::Closed => return Ok(None),
                }
            }
            remaining(deadline)?;
            // 读超时（reqwest read timeout）→ NoData 语义。
            let count = self.response.read(&mut chunk).map_err(|e| {
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.get_ref()
                        .and_then(|e| e.downcast_ref::<reqwest::Error>())
                        .is_some_and(reqwest::Error::is_timeout)
                {
                    IoError::Timeout
                } else {
                    IoError::Network
                }
            })?;
            if count == 0 {
                // 对端关流：视作断连（网络错误，调用方重连）。
                return Err(IoError::Network);
            }
            self.pending.extend(self.parser.feed(&chunk[..count]));
            // 双超时判定：无 boundary 进展（时间窗 + 字节阈值）。
            if Instant::now() >= self.progress_deadline
                || self.parser.bytes_without_progress() > self.byte_budget
            {
                return Err(IoError::Timeout);
            }
            if self.parser.should_disconnect() {
                // 连续超限（恶意流）：断连由调用方重建。
                return Err(IoError::Unsupported);
            }
        }
    }

    fn progress_reset(&mut self) {
        self.progress_deadline = Instant::now() + NO_PROGRESS_WINDOW;
    }

    pub fn discarded(&self) -> u64 {
        self.discarded
    }

    pub fn false_content_length(&self) -> u64 {
        self.false_content_length
    }
}

fn format_error(error: StreamFormatError) -> IoError {
    let _ = error;
    IoError::Unsupported
}
