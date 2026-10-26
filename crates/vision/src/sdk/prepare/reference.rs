// 仅测试冻结ddae515旧算法，避免优化后用相同实现自证正确。
// 原prepare.rs SHA-256: bf97f1899375ee41dd7bbea511d067d50aa67c239f92d3b139094b3c856ba4a0
use super::{ImageTransform, Pixels, Preprocess, TensorData, TensorInfo};
use crate::{Error, Result};

pub(super) fn prepare(
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
    let mut values = vec![0f32; area * 3];
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
    let shape = if p.nhwc {
        vec![1, p.height as i64, p.width as i64, 3]
    } else {
        vec![1, 3, p.height as i64, p.width as i64]
    };
    let data = unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), bytes) };
    Ok((
        TensorData::new(
            TensorInfo {
                name: name.into(),
                dtype: 0,
                shape,
            },
            data,
        )?,
        m,
    ))
}
