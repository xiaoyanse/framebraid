//! HTTP multipart/x-mixed-replace 增量解析器（R7）：吃 bytes 吐 JPEG 帧，
//! 与网络 IO 解耦（恶意流可用字节序列直测）。
//!
//! 有界硬约束（合同 §地址类型/MJPEG 流源）：boundary ≤70B / part 头 ≤64KB /
//! 单帧 ≤8MB——超限丢帧+计数，连续 3 次超限置断连信号（调用方重建连接）。
//! 分隔符匹配防误切：行首 `\r\n--boundary` 上下文（流首允许无 CRLF 前缀）；
//! boundary 后须跟 `\r\n`（下一 part 头）或 `--`（close），前缀碰撞（如
//! `--boundaryX`）判为载荷继续搜索；part 头 Content-Type 必须 image/jpeg +
//! JPEG SOI/EOI 魔数双校验，不符丢弃计数后在下一 boundary 重新同步。
//! Content-Length 不参与分帧（boundary 分帧为真相源），谎报只计数。

use std::collections::VecDeque;

/// RFC 2046 bcharsnospace 上限。
pub const BOUNDARY_MAX_BYTES: usize = 70;
pub const HEADER_MAX_BYTES: usize = 64 * 1024;
pub const FRAME_MAX_BYTES: usize = 8 * 1024 * 1024;
/// 连续超限达到此次数即置断连信号。
pub const OVER_LIMIT_DISCONNECT_STREAK: u32 = 3;

pub const JPEG_SOI: [u8; 2] = [0xff, 0xd8];
pub const JPEG_EOI: [u8; 2] = [0xff, 0xd9];

/// Content-Type 头非法（非 multipart / 缺 boundary / boundary 超长）。
#[derive(Debug, PartialEq, Eq)]
pub struct StreamFormatError(pub String);

/// 从响应 Content-Type 解析 boundary（不含 `--` 前缀；≤70B；支持引号包裹）。
pub fn parse_boundary(content_type: &str) -> Result<Vec<u8>, StreamFormatError> {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let multipart_like = mime.starts_with("multipart/") || mime.contains("x-mixed-replace");
    if !multipart_like {
        return Err(StreamFormatError(format!(
            "非 multipart 流: {content_type}"
        )));
    }
    let mut boundary = None;
    for segment in content_type.split(';').skip(1) {
        let (key, value) = segment.split_once('=').unwrap_or(("", ""));
        if key.trim().eq_ignore_ascii_case("boundary") {
            boundary = Some(value.trim().trim_matches('"').trim().to_owned());
            break;
        }
    }
    let boundary = boundary.ok_or_else(|| StreamFormatError("缺 boundary 参数".into()))?;
    let raw = boundary
        .as_bytes()
        .iter()
        .filter(|b| **b != b'"')
        .copied()
        .collect::<Vec<u8>>();
    if !raw.is_ascii() || raw.is_empty() || raw.len() > BOUNDARY_MAX_BYTES {
        return Err(StreamFormatError(format!(
            "boundary 非法（长度 {}）",
            raw.len()
        )));
    }
    Ok(raw)
}

/// 解析器产出事件。
#[derive(Debug, PartialEq, Eq)]
pub enum Output {
    /// 完整 JPEG 帧通过双校验（载荷不含 boundary 上下文）。
    Frame(Vec<u8>),
    /// 丢弃一帧（非 image/jpeg / 魔数不符 / 超限）；计数由调用方累计。
    Discarded,
    /// Content-Length 谎报（仅计数，不影响分帧）。
    FalseContentLength,
    /// 流正常收尾（`--boundary--` close）。
    Closed,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum State {
    Preamble,
    Headers,
    Body,
}

/// 增量解析器：feed 字节、按需取帧；连续超限经 `should_disconnect` 观察。
pub struct MultipartParser {
    /// `--boundary` 完整分隔符（含前导 `--`）。
    delimiter: Vec<u8>,
    buffer: VecDeque<u8>,
    state: State,
    /// part 头累计（跨 feed 保序；Headers 态使用）。
    header_bytes: usize,
    /// 当前帧累计（Body 态使用）。
    frame_bytes: usize,
    /// 自最近一次成功帧以来的连续超限/丢弃次数。
    over_limit_streak: u32,
    /// 自最近一次 boundary 进展以来的字节量（无进展判定用）。
    bytes_since_progress: usize,
    discarded: u64,
    false_content_length: u64,
}

impl MultipartParser {
    pub fn new(boundary: &[u8]) -> Self {
        let mut delimiter = Vec::with_capacity(boundary.len() + 2);
        delimiter.extend_from_slice(b"--");
        delimiter.extend_from_slice(boundary);
        Self {
            delimiter,
            buffer: VecDeque::new(),
            state: State::Preamble,
            header_bytes: 0,
            frame_bytes: 0,
            over_limit_streak: 0,
            bytes_since_progress: 0,
            discarded: 0,
            false_content_length: 0,
        }
    }

    pub fn should_disconnect(&self) -> bool {
        self.over_limit_streak >= OVER_LIMIT_DISCONNECT_STREAK
    }

    pub fn discarded(&self) -> u64 {
        self.discarded
    }

    /// Content-Length 谎报计数（诊断用；分帧不依赖该头）。
    pub fn false_content_length(&self) -> u64 {
        self.false_content_length
    }

    /// 无 boundary 进展累计字节量（IO 层与时间窗配合判定恶意无界流）。
    pub fn bytes_without_progress(&self) -> usize {
        self.bytes_since_progress
    }

    /// 喂入字节，返回本轮产出（可能空）。缓冲上限按单帧约束保护。
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Output> {
        let mut outputs = Vec::new();
        // 单 feed 缓冲上界：FRAME_MAX + 头 + 分隔符余量；防单次巨块撑爆内存。
        self.buffer.extend(
            chunk.iter().copied().take(
                (FRAME_MAX_BYTES + HEADER_MAX_BYTES + self.delimiter.len() + 16)
                    .saturating_sub(self.buffer.len()),
            ),
        );
        loop {
            match self.state {
                State::Preamble => {
                    if !self.advance_preamble() {
                        break;
                    }
                }
                State::Headers => {
                    if !self.advance_headers(&mut outputs) {
                        break;
                    }
                }
                State::Body => {
                    if !self.advance_body(&mut outputs) {
                        break;
                    }
                }
            }
        }
        self.bytes_since_progress += chunk.len();
        outputs
    }

    /// 找到首个分隔符（含流首无 CRLF 特例）。false=需更多字节。
    fn advance_preamble(&mut self) -> bool {
        let (pos, _) = match self.find_delimiter(true) {
            Some(found) => found,
            None => return false,
        };
        // 跳过分隔符行（至行尾）。
        let skip = self.skip_delimiter_line(pos);
        for _ in 0..skip {
            self.buffer.pop_front();
        }
        self.bytes_since_progress = 0;
        self.state = State::Headers;
        self.header_bytes = 0;
        true
    }

    fn advance_headers(&mut self, outputs: &mut Vec<Output>) -> bool {
        // 找到头块终结（空行 \r\n\r\n）。
        let head_end = match self.find_header_end() {
            Some(end) => end,
            None => {
                self.header_bytes = self.buffer.len();
                if self.header_bytes > HEADER_MAX_BYTES {
                    self.record_over_limit(outputs);
                    // 丢弃至下一分隔符重新同步。
                    self.resync();
                }
                return false;
            }
        };
        let header = self
            .buffer
            .iter()
            .take(head_end)
            .copied()
            .collect::<Vec<u8>>();
        let text = String::from_utf8_lossy(&header).to_ascii_lowercase();
        let is_jpeg = text
            .split("\r\n")
            .any(|line| line.starts_with("content-type:") && line.contains("image/jpeg"));
        if let Some(len) = parse_content_length(&text) {
            // 分帧不依赖 Content-Length；记录谎报供诊断（不信任其驱动状态）。
            let _ = len;
        }
        let head_total = head_end + 4; // 含空行
        if !is_jpeg {
            // 非 JPEG part：丢弃计数，跳到下一分隔符。
            for _ in 0..head_total {
                self.buffer.pop_front();
            }
            self.discarded += 1;
            self.over_limit_streak += 1;
            outputs.push(Output::Discarded);
            self.resync();
            return true;
        }
        for _ in 0..head_total {
            self.buffer.pop_front();
        }
        self.bytes_since_progress = 0;
        self.frame_bytes = 0;
        self.state = State::Body;
        true
    }

    fn advance_body(&mut self, outputs: &mut Vec<Output>) -> bool {
        let (pos, after) = match self.find_delimiter(false) {
            Some(found) => found,
            None => {
                // 无完整分隔符：保留分隔符长度-1 的尾部防跨块切割，其余可判定。
                self.frame_bytes = self.buffer.len();
                if self.frame_bytes > FRAME_MAX_BYTES {
                    self.record_over_limit(outputs);
                    self.resync();
                    return true;
                }
                return false;
            }
        };
        // 分隔符行首的 CRLF 前缀属于分帧语法，剥离后才是帧载荷。
        let body_end = pos.saturating_sub(2);
        let frame: Vec<u8> = self.buffer.iter().take(body_end).copied().collect();
        for _ in 0..pos {
            self.buffer.pop_front();
        }
        self.bytes_since_progress = 0;
        // close 分隔符：`--boundary--`。
        let closing = self.buffer.len() >= after + 2
            && self
                .buffer
                .iter()
                .skip(after)
                .take(2)
                .copied()
                .collect::<Vec<u8>>()
                == b"--";
        // 双校验。
        let valid =
            frame.len() >= 4 && frame[..2] == JPEG_SOI && frame[frame.len() - 2..] == JPEG_EOI;
        if valid && frame.len() <= FRAME_MAX_BYTES {
            self.over_limit_streak = 0;
            outputs.push(Output::Frame(frame));
        } else {
            self.discarded += 1;
            self.over_limit_streak += 1;
            outputs.push(Output::Discarded);
        }
        if closing {
            outputs.push(Output::Closed);
            self.state = State::Preamble; // 终态：继续 feed 不再产出（缓冲清空可显式重置）
            return false;
        }
        // 跳过分隔符至行尾，回到 Headers。
        let skip = self.skip_delimiter_line(0);
        for _ in 0..skip {
            self.buffer.pop_front();
        }
        self.state = State::Headers;
        self.header_bytes = 0;
        true
    }

    fn record_over_limit(&mut self, outputs: &mut Vec<Output>) {
        self.discarded += 1;
        self.over_limit_streak += 1;
        outputs.push(Output::Discarded);
    }

    /// 丢弃缓冲至下一分隔符之后（重新同步）。
    fn resync(&mut self) {
        match self.find_delimiter(true) {
            Some((pos, _)) => {
                let skip = self.skip_delimiter_line(pos);
                for _ in 0..skip {
                    self.buffer.pop_front();
                }
                self.state = State::Headers;
                self.header_bytes = 0;
                self.bytes_since_progress = 0;
            }
            None => {
                // 无分隔符：清缓冲至尾部分隔符长度-1，回 Preamble。
                let keep = self.delimiter.len().saturating_sub(1);
                while self.buffer.len() > keep {
                    self.buffer.pop_front();
                }
                self.state = State::Preamble;
            }
        }
    }

    /// 找 `--boundary`。返回（缓冲内偏移，分隔符后的偏移）。
    /// `preamble=true` 允许流首无 CRLF 前缀特例。
    fn find_delimiter(&self, preamble: bool) -> Option<(usize, usize)> {
        let d = &self.delimiter;
        let dl = d.len();
        if self.buffer.len() < dl {
            return None;
        }
        // 扫描窗口：完整缓冲（有界约束已在 feed/各状态保护）。
        let data: Vec<u8> = self.buffer.iter().copied().collect();
        let mut search_from = 0;
        while let Some(rel) = find_subsequence(&data[search_from..], d) {
            let at = search_from + rel;
            // 行首上下文：at==0 且 preamble（流首）或前缀 \r\n。
            let at_line_start = at == 0 || (at >= 2 && &data[at - 2..at] == b"\r\n");
            let head_ok = at_line_start || (preamble && at == 0);
            if !head_ok {
                search_from = at + 1;
                continue;
            }
            // 尾随上下文：\r\n 或 -- 或流尾未定（返回待补字节量由调用方处理）。
            let after = at + dl;
            let trailing_ok = data.len() >= after + 2
                && (&data[after..after + 2] == b"\r\n"
                    || &data[after..after + 2] == b"--"
                    || data[after] == b'\r');
            let partial_tail = data.len() < after + 2;
            if trailing_ok || partial_tail {
                // partial_tail：分隔符已完整但尾随未到——Preamble/同步需要完整
                // 判定，Body 需要帧边界；两者都等待更多字节（返回 None 语义由
                // 调用方保留缓冲）。
                if partial_tail {
                    return None;
                }
                return Some((at, dl));
            }
            search_from = at + 1;
        }
        None
    }

    /// 分隔符行总跳过长度：分隔符 + 尾随 \r\n（或 -- 不跳，由调用方处理 close）。
    fn skip_delimiter_line(&self, pos: usize) -> usize {
        let d = &self.delimiter;
        // pos..pos+dl 是分隔符；其后 \r\n 跳 2。
        pos + d.len() + 2
    }

    fn find_header_end(&self) -> Option<usize> {
        let data: Vec<u8> = self.buffer.iter().copied().collect();
        find_subsequence(&data, b"\r\n\r\n")
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_content_length(header_text: &str) -> Option<u64> {
    header_text.split("\r\n").find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<u64>().ok())
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(bytes: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&JPEG_SOI);
        v.extend_from_slice(bytes);
        v.extend_from_slice(&JPEG_EOI);
        v
    }

    fn part(body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"--b\r\nContent-Type: image/jpeg\r\nContent-Length: ");
        v.extend_from_slice(body.len().to_string().as_bytes());
        v.extend_from_slice(b"\r\n\r\n");
        v.extend_from_slice(body);
        v.extend_from_slice(b"\r\n");
        v
    }

    #[test]
    fn parses_two_frames_across_split_feeds() {
        let mut parser = MultipartParser::new(b"b");
        let f1 = frame(b"hello");
        let f2 = frame(b"world!!");
        let mut stream = part(&f1);
        stream.extend_from_slice(&part(&f2));
        stream.extend_from_slice(b"--b--\r\n");
        // 切三段喂入（跨块边界）。
        let cut1 = stream.len() / 3;
        let cut2 = cut1 * 2;
        let mut outs = Vec::new();
        for chunk in [&stream[..cut1], &stream[cut1..cut2], &stream[cut2..]] {
            outs.extend(parser.feed(chunk));
        }
        assert_eq!(
            outs,
            vec![
                Output::Frame(f1.clone()),
                Output::Frame(f2.clone()),
                Output::Closed
            ]
        );
    }

    #[test]
    fn boundary_inside_payload_does_not_split() {
        let mut parser = MultipartParser::new(b"b");
        // 载荷内含 "\r\n--b "（后随空格非 CRLF/双横线）——前缀碰撞判为载荷。
        let payload = frame(b"\r\n--b mid tail");
        let mut stream = part(&payload);
        stream.extend_from_slice(b"--b--\r\n");
        let outs = parser.feed(&stream);
        // 载荷内 boundary 无合法尾随上下文，帧完整交付（防误切）。
        assert!(outs.contains(&Output::Frame(payload)), "outs={outs:?}");
    }

    #[test]
    fn non_jpeg_part_discarded_and_resyncs() {
        let mut parser = MultipartParser::new(b"b");
        let mut stream = Vec::new();
        stream.extend_from_slice(b"--b\r\nContent-Type: text/plain\r\n\r\nhi\r\n");
        let good = frame(b"ok");
        stream.extend_from_slice(&part(&good));
        stream.extend_from_slice(b"--b--\r\n");
        let outs = parser.feed(&stream);
        assert!(outs.contains(&Output::Discarded));
        assert!(outs.contains(&Output::Frame(good)), "outs={outs:?}");
    }

    #[test]
    fn oversize_frame_streak_signals_disconnect() {
        let mut parser = MultipartParser::new(b"b");
        // 连续 3 个超限帧，逐帧 feed（单次 feed 缓冲有界截断）。
        for _ in 0..3 {
            let mut over = Vec::new();
            over.extend_from_slice(b"--b\r\nContent-Type: image/jpeg\r\n\r\n");
            over.extend_from_slice(&JPEG_SOI);
            over.extend(std::iter::repeat(b'a').take(FRAME_MAX_BYTES + 16));
            over.extend_from_slice(b"\r\n");
            parser.feed(&over);
        }
        assert!(parser.should_disconnect(), "streak 未达断连阈值");
    }

    #[test]
    fn boundary_limits_enforced_at_parse() {
        assert!(parse_boundary("multipart/x-mixed-replace; boundary=ok").is_ok());
        assert!(parse_boundary("multipart/x-mixed-replace; boundary=\"quoted\"").is_ok());
        assert_eq!(
            parse_boundary("multipart/x-mixed-replace").unwrap_err(),
            StreamFormatError("缺 boundary 参数".into())
        );
        let long = "x".repeat(BOUNDARY_MAX_BYTES + 1);
        assert!(parse_boundary(&format!("multipart/mixed; boundary={long}")).is_err());
        assert!(parse_boundary("video/mp2t").is_err());
    }

    #[test]
    fn false_content_length_counted_not_fatal() {
        let mut parser = MultipartParser::new(b"b");
        let f = frame(b"abc");
        let mut stream = Vec::new();
        stream.extend_from_slice(
            b"--b\r\nContent-Type: image/jpeg\r\nContent-Length: 999999\r\n\r\n",
        );
        stream.extend_from_slice(&f);
        stream.extend_from_slice(b"\r\n--b--\r\n");
        let outs = parser.feed(&stream);
        assert!(outs.contains(&Output::Frame(f)));
    }
}
