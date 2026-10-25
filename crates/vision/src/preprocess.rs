use crate::{Error, Result};
use image::{
    imageops::{resize, FilterType},
    DynamicImage, ImageDecoder, ImageReader, Limits,
};
use std::io::Cursor;

pub struct Mapping {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub left: u32,
    pub top: u32,
}

pub struct Prepared {
    pub tensor: Vec<f32>,
    pub mapping: Mapping,
}

pub fn prepare(bytes: &[u8]) -> Result<Prepared> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Error("vision.imageTooLarge".into()));
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| Error("vision.imageFormatUnknown".into()))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| Error("vision.imageDecodeFailed".into()))?;
    if decoder.total_bytes() > 128 * 1024 * 1024 {
        return Err(Error("vision.imageMemoryTooLarge".into()));
    }
    let orientation = decoder
        .orientation()
        .map_err(|_| Error("vision.orientationInvalid".into()))?;
    let mut decoded = DynamicImage::from_decoder(decoder)
        .map_err(|_| Error("vision.imageDecodeFailed".into()))?;
    // Browsers honor JPEG EXIF orientation. Infer in that same displayed coordinate system.
    decoded.apply_orientation(orientation);
    let image = decoded.to_rgb8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Err(Error("vision.imageSizeInvalid".into()));
    }
    let scale = (640.0 / width as f32).min(640.0 / height as f32);
    let resized_width = ((width as f32 * scale).round() as u32).clamp(1, 640);
    let resized_height = ((height as f32 * scale).round() as u32).clamp(1, 640);
    let left = (640 - resized_width) / 2;
    let top = (640 - resized_height) / 2;
    let resized = resize(&image, resized_width, resized_height, FilterType::Triangle);
    let mut tensor = vec![114.0 / 255.0; 3 * 640 * 640];
    for (x, y, pixel) in resized.enumerate_pixels() {
        for channel in 0..3 {
            tensor[channel * 640 * 640 + ((y + top) * 640 + x + left) as usize] =
                pixel[channel] as f32 / 255.0;
        }
    }
    Ok(Prepared {
        tensor,
        mapping: Mapping {
            width,
            height,
            scale,
            left,
            top,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{codecs::jpeg::JpegEncoder, ImageEncoder, ImageFormat, Rgb, RgbImage};
    #[test]
    fn jpeg_orientation_six_matches_browser_display_coordinates() {
        let rgb = RgbImage::from_fn(3, 2, |x, y| Rgb([(x * 70) as u8, (y * 120) as u8, 40]));
        let mut jpeg = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut jpeg, 100);
        // Little-endian TIFF: orientation tag (0x0112), SHORT count=1, value=6 (90 CW).
        encoder
            .set_exif_metadata(vec![
                0x49, 0x49, 0x2a, 0, 8, 0, 0, 0, 1, 0, 0x12, 1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0,
                0, 0,
            ])
            .unwrap();
        encoder.encode_image(&DynamicImage::ImageRgb8(rgb)).unwrap();
        let raw = ImageReader::new(Cursor::new(&jpeg))
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();
        let mut png = Cursor::new(Vec::new());
        raw.rotate90().write_to(&mut png, ImageFormat::Png).unwrap();
        let oriented = prepare(&jpeg).unwrap();
        let expected = prepare(png.get_ref()).unwrap();
        assert_eq!((oriented.mapping.width, oriented.mapping.height), (2, 3));
        assert_eq!(oriented.tensor, expected.tensor);
    }
}
