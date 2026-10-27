use super::{
    model::{TensorData, TensorInfo},
    pixels::Pixels,
};
use crate::{Error, Result};

#[derive(Clone, Copy, PartialEq)]
pub struct Preprocess {
    pub width: u32,
    pub height: u32,
    pub resize: u32,
    pub fill: f32,
    pub bgr: bool,
    pub normalize: u32,
    pub mean: [f32; 3],
    pub std: [f32; 3],
    pub nhwc: bool,
}
impl Preprocess {
    pub fn profile(size: u32, classify: bool) -> Self {
        Self {
            width: size,
            height: size,
            resize: if classify { 2 } else { 0 },
            fill: 114.0,
            bgr: false,
            normalize: 1,
            mean: [0.0; 3],
            std: [1.0; 3],
            nhwc: false,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ImageTransform {
    pub width: u32,
    pub height: u32,
    pub dst_w: u32,
    pub dst_h: u32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub crop_x: i32,
    pub crop_y: i32,
    pub left: u32,
    pub top: u32,
}
pub fn prepare(
    pixels: &Pixels<'_>,
    name: &str,
    p: Preprocess,
    budget: usize,
) -> Result<(TensorData, ImageTransform)> {
    pixels.validate()?;
    let bytes = (p.width as usize)
        .checked_mul(p.height as usize)
        .and_then(|n| n.checked_mul(12))
        .ok_or_else(|| Error("vision.imageTooLarge".into()))?;
    if p.width == 0 || p.height == 0 || p.width > 8192 || p.height > 8192 || bytes > budget {
        return Err(Error("vision.imageTooLarge".into()));
    }
    if p.resize > 2
        || p.normalize > 2
        || !p.fill.is_finite()
        || p.mean.iter().any(|v| !v.is_finite())
        || p.std.iter().any(|v| !v.is_finite() || *v <= 0.0)
    {
        return Err(Error("sdk.invalidArgument".into()));
    }
    let (w, h) = (pixels.width, pixels.height);
    let (sx, sy) = (p.width as f32 / w as f32, p.height as f32 / h as f32);
    let scale = if p.resize == 2 {
        sx.max(sy)
    } else {
        sx.min(sy)
    };
    let (scaled_w, scaled_h) = if p.resize == 1 {
        (p.width, p.height)
    } else {
        (
            ((w as f32 * scale).round() as u32).max(1),
            ((h as f32 * scale).round() as u32).max(1),
        )
    };
    let mut m = ImageTransform {
        width: w,
        height: h,
        dst_w: p.width,
        dst_h: p.height,
        scale_x: if p.resize == 1 { sx } else { scale },
        scale_y: if p.resize == 1 { sy } else { scale },
        crop_x: 0,
        crop_y: 0,
        left: 0,
        top: 0,
    };
    if p.resize == 0 {
        m.left = (p.width - scaled_w) / 2;
        m.top = (p.height - scaled_h) / 2;
    }
    // 中心裁切在源图整数像素上定义，变换元数据可精确重建本次采样坐标。
    if p.resize == 2 {
        m.crop_x = ((w as f32 - p.width as f32 / scale) / 2.0).floor().max(0.0) as i32;
        m.crop_y = ((h as f32 - p.height as f32 / scale) / 2.0)
            .floor()
            .max(0.0) as i32;
    }
    let area = (p.width * p.height) as usize;
    let shape = if p.nhwc {
        vec![1, p.height as i64, p.width as i64, 3]
    } else {
        vec![1, 3, p.height as i64, p.width as i64]
    };
    let mut tensor = TensorData::zeroed(TensorInfo {
        name: name.into(),
        dtype: 0,
        shape,
    })?;
    // 直接写独占且已核长度的对齐拥有者，避免完整临时浮点数组及再次复制。
    let values = tensor.floats_mut()?;
    for y in 0..p.height {
        for x in 0..p.width {
            let rgb = if p.resize == 0
                && (x < m.left || y < m.top || x >= m.left + scaled_w || y >= m.top + scaled_h)
            {
                [p.fill; 3]
            } else {
                let xx = ((x as f32 - m.left as f32 + 0.5) / m.scale_x - 0.5 + m.crop_x as f32)
                    .clamp(0.0, w as f32 - 1.0);
                let yy = ((y as f32 - m.top as f32 + 0.5) / m.scale_y - 0.5 + m.crop_y as f32)
                    .clamp(0.0, h as f32 - 1.0);
                let (ix, iy) = (xx as usize, yy as usize);
                let (fx, fy) = (xx - ix as f32, yy - iy as f32);
                let a = pixels.rgb(ix, iy);
                let b = pixels.rgb((ix + 1).min(w as usize - 1), iy);
                let c = pixels.rgb(ix, (iy + 1).min(h as usize - 1));
                let d = pixels.rgb((ix + 1).min(w as usize - 1), (iy + 1).min(h as usize - 1));
                std::array::from_fn(|k| {
                    (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy)
                        + (c[k] * (1.0 - fx) + d[k] * fx) * fy
                })
            };
            for channel in 0..3 {
                let mut value = rgb[if p.bgr { 2 - channel } else { channel }];
                if p.normalize != 0 {
                    value /= 255.0;
                }
                if p.normalize == 2 {
                    value = (value - p.mean[channel]) / p.std[channel];
                }
                let i = (y * p.width + x) as usize;
                values[if p.nhwc {
                    i * 3 + channel
                } else {
                    channel * area + i
                }] = value;
            }
        }
    }
    Ok((tensor, m))
}

#[cfg(test)]
mod reference;
#[cfg(test)]
mod tests;
