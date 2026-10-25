use crate::{Error, Result};
use image::{DynamicImage, ImageDecoder, ImageReader, Limits};
use std::io::Cursor;

fn reader(bytes: &[u8]) -> Result<impl ImageDecoder + '_> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| Error("vision.imageFormatUnknown".into()))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader
        .into_decoder()
        .map_err(|_| Error("vision.imageDecodeFailed".into()))
}
pub fn dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    let d = reader(bytes)?;
    let (w, h) = d.dimensions();
    if w == 0 || h == 0 || d.total_bytes() > 64 * 1024 * 1024 {
        return Err(Error("vision.imageTooLarge".into()));
    }
    Ok((w, h))
}
pub fn decode(bytes: &[u8]) -> Result<image::RgbImage> {
    dimensions(bytes)?;
    let mut d = reader(bytes)?;
    let orientation = d
        .orientation()
        .map_err(|_| Error("vision.orientationInvalid".into()))?;
    let mut image =
        DynamicImage::from_decoder(d).map_err(|_| Error("vision.imageDecodeFailed".into()))?;
    image.apply_orientation(orientation);
    Ok(image.to_rgb8())
}
