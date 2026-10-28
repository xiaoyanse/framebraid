use super::{digest, validate_url, Image, IoError};
use crate::credentials::Secret;
use image::ImageDecoder as _;
use std::{
    collections::HashMap,
    io::{Cursor, Read},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

/// 进程级 Digest challenge 缓存（origin → 最近 challenge）。容量上限防漂移；
/// nonce 失效由 401 拒收回退刷新，不存凭据。
static CHALLENGE_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn challenge_cache() -> &'static Mutex<HashMap<String, String>> {
    CHALLENGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn port_map(port: Option<u16>) -> String {
    match port {
        Some(p) => format!(":{p}"),
        None => String::new(),
    }
}

pub fn capture(
    url: &str,
    secret: &Secret,
    timeout: Duration,
    max_bytes: usize,
    max_pixels: u64,
    decode: bool,
) -> Result<Image, IoError> {
    validate_url(url, true)?;
    if timeout.is_zero()
        || max_bytes == 0
        || max_bytes > 64 * 1024 * 1024
        || max_pixels == 0
        || max_pixels > 64 * 1024 * 1024
    {
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
    // Digest challenge 缓存（2026-10-29，DEP-004 R2）：按源 origin 缓存最近一次
    // challenge，命中时单事务直达（原实现每次先匿名探测 401，节拍恒 2 倍）。
    // nonce 失效由服务器 401 拒收表达——快路径失败自动回退匿名探测并刷新缓存，
    // 语义与逐次探测等价。跨源隔离按 origin（scheme+host+port）。
    let parsed = reqwest::Url::parse(url).map_err(|_| IoError::Invalid)?;
    let cache_key = match (parsed.scheme(), parsed.host_str(), parsed.port()) {
        (scheme, Some(host), port) => {
            let port_suffix = port_map(port);
            format!("{scheme}://{host}{port_suffix}")
        }
        _ => String::new(),
    };
    let cached = if cache_key.is_empty() {
        None
    } else {
        challenge_cache()
            .lock()
            .ok()
            .and_then(|cache| cache.get(&cache_key).cloned())
    };
    let mut uri = parsed.path().to_owned();
    if let Some(query) = parsed.query() {
        uri.push('?');
        uri.push_str(query);
    }
    let mut response = if let Some(header) = cached {
        // 快路径：缓存 challenge 直接授权请求。nonce 一次性/轮换的服务器以 401
        // 拒收并附新 challenge——该响应直接进入下方统一分支消费新 challenge
        // 重授权（无匿名回退）：任何 nonce 语义下每拍 ≤2 事务，可复用 nonce
        // 的服务器 1 事务。
        let authorization = digest::authorization(&header, secret, &uri).ok();
        match authorization.and_then(|auth| {
            client
                .get(url)
                .header(reqwest::header::AUTHORIZATION, auth)
                .timeout(remaining(deadline).ok()?)
                .send()
                .ok()
        }) {
            Some(response) => response,
            None => client
                .get(url)
                .timeout(remaining(deadline)?)
                .send()
                .map_err(network)?,
        }
    } else {
        client
            .get(url)
            .timeout(remaining(deadline)?)
            .send()
            .map_err(network)?
    };
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let header = response
            .headers()
            .get_all(reqwest::header::WWW_AUTHENTICATE)
            .iter()
            .filter_map(|h| h.to_str().ok())
            .find(|h| {
                h.get(..7)
                    .is_some_and(|p| p.eq_ignore_ascii_case("digest "))
            })
            .ok_or(IoError::Unsupported)?;
        if !cache_key.is_empty() {
            if let Ok(mut cache) = challenge_cache().lock() {
                cache.insert(cache_key, header.to_owned());
            }
        }
        let authorization = digest::authorization(header, secret, &uri)?;
        drop(response);
        response = client
            .get(url)
            .header(reqwest::header::AUTHORIZATION, authorization)
            .timeout(remaining(deadline)?)
            .send()
            .map_err(network)?;
    }
    if matches!(response.status().as_u16(), 401 | 403) {
        return Err(IoError::Auth);
    }
    if !response.status().is_success() {
        return Err(IoError::Network);
    }
    if response
        .content_length()
        .is_some_and(|n| n > max_bytes as u64)
    {
        return Err(IoError::Resource);
    }
    let mut encoded = Vec::new();
    encoded
        .try_reserve(max_bytes.min(64 * 1024))
        .map_err(|_| IoError::Resource)?;
    let mut bounded = response.take(max_bytes as u64 + 1);
    let mut chunk = [0; 16 * 1024];
    loop {
        remaining(deadline)?;
        let count = bounded.read(&mut chunk).map_err(|e| {
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
            break;
        }
        if encoded.len() + count > max_bytes {
            return Err(IoError::Resource);
        }
        encoded.try_reserve(count).map_err(|_| IoError::Resource)?;
        encoded.extend_from_slice(&chunk[..count]);
    }
    let result = decode_image(encoded, max_pixels, decode)?;
    remaining(deadline)?;
    Ok(result)
}

/// 与抓图共用方向、尺寸和分配限制，可直接解码 MJPEG 包。
pub fn decode_image(encoded: Vec<u8>, max_pixels: u64, decode: bool) -> Result<Image, IoError> {
    if encoded.len() > 64 * 1024 * 1024 || max_pixels == 0 || max_pixels > 64 * 1024 * 1024 {
        return Err(IoError::Resource);
    }
    let format = image::guess_format(&encoded).map_err(|_| IoError::Unsupported)?;
    if !matches!(format, image::ImageFormat::Jpeg | image::ImageFormat::Png) {
        return Err(IoError::Unsupported);
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(max_pixels.min(u32::MAX as u64) as u32);
    limits.max_image_height = limits.max_image_width;
    limits.max_alloc = Some(max_pixels.checked_mul(8).ok_or(IoError::Resource)?);
    let mut reader = image::ImageReader::with_format(Cursor::new(&encoded), format);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(image_error)?;
    let (mut width, mut height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > max_pixels {
        return Err(IoError::Resource);
    }
    let orientation = decoder.orientation().map_err(|_| IoError::Invalid)?;
    if decoder.total_bytes() > max_pixels.saturating_mul(8) {
        return Err(IoError::Resource);
    }
    let rgba = if decode {
        if format == image::ImageFormat::Jpeg {
            // JPEG 快路径（R6）：zune-jpeg 直解 RGBA，消除 DynamicImage 中转的
            // 全图 RGB→RGBA 转换（实测约 25% 耗时）；EXIF 方向用解码头结果。
            drop(decoder);
            let options = zune_jpeg::zune_core::options::DecoderOptions::default()
                .jpeg_set_out_colorspace(zune_jpeg::zune_core::colorspace::ColorSpace::RGBA);
            let mut jpeg = zune_jpeg::JpegDecoder::new_with_options(
                zune_jpeg::zune_core::bytestream::ZCursor::new(&encoded[..]),
                options,
            );
            let pixels = jpeg.decode().map_err(|_| IoError::Invalid)?;
            let rgba_image =
                image::RgbaImage::from_raw(width, height, pixels).ok_or(IoError::Resource)?;
            let mut dynamic = image::DynamicImage::ImageRgba8(rgba_image);
            dynamic.apply_orientation(orientation);
            Some(dynamic.into_rgba8().into_raw())
        } else {
            let mut image = image::DynamicImage::from_decoder(decoder).map_err(image_error)?;
            image.apply_orientation(orientation);
            Some(image.into_rgba8().into_raw())
        }
    } else {
        drop(decoder);
        None
    };
    if orientation.to_exif() >= 5 {
        std::mem::swap(&mut width, &mut height);
    }
    Ok(Image {
        encoded,
        rgba,
        width,
        height,
    })
}

fn remaining(deadline: Instant) -> Result<Duration, IoError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(IoError::Timeout)
    } else {
        Ok(remaining)
    }
}
fn network(error: reqwest::Error) -> IoError {
    if error.is_timeout() {
        IoError::Timeout
    } else {
        IoError::Network
    }
}
fn image_error(error: image::ImageError) -> IoError {
    if matches!(error, image::ImageError::Limits(_)) {
        IoError::Resource
    } else {
        IoError::Invalid
    }
}
