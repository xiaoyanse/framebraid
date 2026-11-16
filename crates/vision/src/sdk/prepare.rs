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
    // 以下重排仅是实现形态优化，与冻结参考（cfg(test) reference.rs）位级等价：
    // 每输出值的公式与运算顺序逐字保留，只有求值被提升到不改变结果的位置。
    let wid = p.width as usize;
    let w_last = w as usize - 1;
    let h_last = h as usize - 1;
    let (left, top) = (m.left, m.top);
    let (right, bottom) = (m.left + scaled_w, m.top + scaled_h);
    // letterbox 填充值三通道同源，归一化结果只依赖配置，整图算一次（同式）。
    let fill_value: [f32; 3] = std::array::from_fn(|channel| {
        let mut value = p.fill;
        if p.normalize != 0 {
            value /= 255.0;
        }
        if p.normalize == 2 {
            value = (value - p.mean[channel]) / p.std[channel];
        }
        value
    });
    // xx 只依赖 x：逐像素的除法/clamp/浮转整串行链按列一次算完（运算与参考逐序相同）。
    #[derive(Clone, Copy)]
    struct ColumnTap {
        fx: f32,
        ix: u32,
        x1: u32,
        fill: bool,
    }
    let columns: Vec<ColumnTap> = (0..p.width)
        .map(|x| {
            if p.resize == 0 && (x < left || x >= right) {
                return ColumnTap {
                    fx: 0.0,
                    ix: 0,
                    x1: 0,
                    fill: true,
                };
            }
            let xx = ((x as f32 - left as f32 + 0.5) / m.scale_x - 0.5 + m.crop_x as f32)
                .clamp(0.0, w as f32 - 1.0);
            let ix = xx as usize;
            ColumnTap {
                fx: xx - ix as f32,
                ix: ix as u32,
                x1: (ix + 1).min(w_last) as u32,
                fill: false,
            }
        })
        .collect();
    // 行缓冲：采样行先写满小缓冲，再按输出布局整行连续写出（值与逐像素直写相同）。
    // raw 存原始 rgb，norm 存归一化结果；bgr 交换是跨平面读，原地写会串道，故双缓冲。
    let mut row_raw = vec![0f32; 3 * wid];
    let mut row_norm = vec![0f32; 3 * wid];
    for y in 0..p.height {
        let row_base = y as usize * wid;
        if p.resize == 0 && (y < top || y >= bottom) {
            // 整行填充带：同式常值直接按通道连续填充，无需采样。
            if p.nhwc {
                let base3 = row_base * 3;
                for pixel in values[base3..base3 + wid * 3].chunks_exact_mut(3) {
                    pixel.copy_from_slice(&fill_value);
                }
            } else {
                for channel in 0..3 {
                    values[channel * area + row_base..channel * area + row_base + wid]
                        .fill(fill_value[channel]);
                }
            }
            continue;
        }
        // yy 只依赖 y：行内一次（同参考公式）。
        let yy = ((y as f32 - top as f32 + 0.5) / m.scale_y - 0.5 + m.crop_y as f32)
            .clamp(0.0, h as f32 - 1.0);
        let iy = yy as usize;
        let y1 = (iy + 1).min(h_last);
        let fy = yy - iy as f32;
        let (iy, y1) = (iy as u32, y1 as u32);
        // 第一段：只算原始 rgb（含混合），按通道平面写入行缓冲；归一化延后到
        // 第二段的通道外连续流（值与逐像素算完即归一化完全相同）。
        for (x, col) in columns.iter().enumerate() {
            if col.fill {
                for channel in 0..3 {
                    row_raw[channel * wid + x] = p.fill;
                }
                continue;
            }
            // 零权重快速路：fx=fy=0（整像素对齐，如 scale=1）时双线性按位退化为单像素
            // 直采。证明：rgb 分量均为有限非负值（u8 直转或 clamp 到 [0,255]，且和式
            // 在舍入到最近下不会产生 -0.0），故 b*0=(+0)、a*1=a、a+(+0)=a、再 *1、
            // 加 c 路 *0=(+0)，全程逐位等于 a。此时只需 1 次 rgb（4 tap 共享同一
            // 变换基的极限情形），跳过 4 tap 与 3 次混合。
            let rgb = if col.fx == 0.0 && fy == 0.0 {
                pixels.rgb(col.ix as usize, iy as usize)
            } else {
                // 同一 (x,y) 的 rgb 是纯函数，边界 clamp 命中时 tap 与相邻 tap 同源，
                // 直接复用值（逐位相同），省去重复转换。
                let a = pixels.rgb(col.ix as usize, iy as usize);
                let b = if col.x1 == col.ix {
                    a
                } else {
                    pixels.rgb(col.x1 as usize, iy as usize)
                };
                let c = if y1 == iy {
                    a
                } else {
                    pixels.rgb(col.ix as usize, y1 as usize)
                };
                let d = if col.x1 == col.ix {
                    c
                } else if y1 == iy {
                    b
                } else {
                    pixels.rgb(col.x1 as usize, y1 as usize)
                };
                let (fx, wx) = (col.fx, 1.0 - col.fx);
                let wy = 1.0 - fy;
                [
                    (a[0] * wx + b[0] * fx) * wy + (c[0] * wx + d[0] * fx) * fy,
                    (a[1] * wx + b[1] * fx) * wy + (c[1] * wx + d[1] * fx) * fy,
                    (a[2] * wx + b[2] * fx) * wy + (c[2] * wx + d[2] * fx) * fy,
                ]
            };
            for channel in 0..3 {
                row_raw[channel * wid + x] = rgb[channel];
            }
        }
        // 第二段：通道外归一化（同式同序，bgr 交换在这里取基址），连续流可向量化。
        for channel in 0..3 {
            let source = if p.bgr { 2 - channel } else { channel } * wid;
            let target = channel * wid;
            for x in 0..wid {
                let mut value = row_raw[source + x];
                if p.normalize != 0 {
                    value /= 255.0;
                }
                if p.normalize == 2 {
                    value = (value - p.mean[channel]) / p.std[channel];
                }
                row_norm[target + x] = value;
            }
        }
        if p.nhwc {
            let base3 = row_base * 3;
            for x in 0..wid {
                for channel in 0..3 {
                    values[base3 + x * 3 + channel] = row_norm[channel * wid + x];
                }
            }
        } else {
            for channel in 0..3 {
                values[channel * area + row_base..channel * area + row_base + wid]
                    .copy_from_slice(&row_norm[channel * wid..(channel + 1) * wid]);
            }
        }
    }
    Ok((tensor, m))
}

#[cfg(test)]
mod reference;
#[cfg(test)]
mod tests;
