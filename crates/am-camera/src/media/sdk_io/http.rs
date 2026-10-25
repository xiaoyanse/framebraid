use super::{digest, validate_url, Image, IoError};
use crate::credentials::Secret;
use image::ImageDecoder as _;
use std::{
    io::{Cursor, Read},
    time::{Duration, Instant},
};

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
    let mut response = client
        .get(url)
        .timeout(remaining(deadline)?)
        .send()
        .map_err(network)?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let parsed = reqwest::Url::parse(url).map_err(|_| IoError::Invalid)?;
        let mut uri = parsed.path().to_owned();
        if let Some(query) = parsed.query() {
            uri.push('?');
            uri.push_str(query);
        }
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
        let mut image = image::DynamicImage::from_decoder(decoder).map_err(image_error)?;
        image.apply_orientation(orientation);
        Some(image.into_rgba8().into_raw())
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
