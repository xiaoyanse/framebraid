//! 无运行库的CPU平面操作；上层在分配前计入帧和临时空间预算。
pub fn resize_plane(
    source: &[u8],
    old: (u32, u32),
    target: &mut [u8],
    new: (u32, u32),
    channels: usize,
    method: u32,
) -> Result<(), &'static str> {
    let (ow, oh) = (old.0 as usize, old.1 as usize);
    let (nw, nh) = (new.0 as usize, new.1 as usize);
    if ow == 0
        || oh == 0
        || nw == 0
        || nh == 0
        || channels == 0
        || channels > 4
        || old.0 > 8192
        || old.1 > 8192
        || new.0 > 8192
        || new.1 > 8192
        || method > 1
        || source.len() != ow * oh * channels
        || target.len() != nw * nh * channels
    {
        return Err("camera.invalidPlane");
    }
    for y in 0..nh {
        for x in 0..nw {
            let tx = ((x as f64 + 0.5) * ow as f64 / nw as f64 - 0.5).clamp(0., (ow - 1) as f64);
            let ty = ((y as f64 + 0.5) * oh as f64 / nh as f64 - 0.5).clamp(0., (oh - 1) as f64);
            for c in 0..channels {
                let sample = |xx: usize, yy: usize| source[(yy * ow + xx) * channels + c] as f64;
                let value = if method == 0 {
                    sample(tx.round() as usize, ty.round() as usize)
                } else {
                    let (x0, y0) = (tx.floor() as usize, ty.floor() as usize);
                    let (dx, dy) = (tx - x0 as f64, ty - y0 as f64);
                    let x1 = (x0 + 1).min(ow - 1);
                    let y1 = (y0 + 1).min(oh - 1);
                    (sample(x0, y0) * (1. - dx) + sample(x1, y0) * dx) * (1. - dy)
                        + (sample(x0, y1) * (1. - dx) + sample(x1, y1) * dx) * dy
                };
                target[(y * nw + x) * channels + c] = value.round().clamp(0., 255.) as u8;
            }
        }
    }
    Ok(())
}
pub fn rgba_to_nv12(source: &[u8], w: u32, h: u32) -> Result<Vec<u8>, &'static str> {
    let (w, h) = (w as usize, h as usize);
    if w == 0
        || h == 0
        || w > 8192
        || h > 8192
        || !w.is_multiple_of(2)
        || !h.is_multiple_of(2)
        || source.len() != w * h * 4
    {
        return Err("camera.invalidPlane");
    }
    let mut out = vec![0; w * h * 3 / 2];
    let uv = |r: f64, g: f64, b: f64| {
        (
            -0.148223 * r - 0.290993 * g + 0.439216 * b + 128.,
            0.439216 * r - 0.367788 * g - 0.071427 * b + 128.,
        )
    };
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let (r, g, b) = (source[i] as f64, source[i + 1] as f64, source[i + 2] as f64);
            out[y * w + x] = (16. + 0.256788 * r + 0.504129 * g + 0.097906 * b)
                .round()
                .clamp(16., 235.) as u8;
            if x % 2 == 0 && y % 2 == 0 {
                let (mut u, mut v) = (0., 0.);
                for dy in 0..2 {
                    for dx in 0..2 {
                        let i = ((y + dy) * w + x + dx) * 4;
                        let p = uv(source[i] as f64, source[i + 1] as f64, source[i + 2] as f64);
                        u += p.0;
                        v += p.1;
                    }
                }
                let i = w * h + (y / 2) * w + x;
                out[i] = (u / 4.).round().clamp(16., 240.) as u8;
                out[i + 1] = (v / 4.).round().clamp(16., 240.) as u8;
            }
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn half_pixel_bilinear_and_chroma_are_independent() {
        let mut out = [0; 9];
        resize_plane(&[0, 100, 200, 255], (2, 2), &mut out, (3, 3), 1, 1).unwrap();
        assert_eq!(out, [0, 50, 100, 100, 139, 178, 200, 228, 255]);
        let rgb = [255, 0, 0, 255].repeat(4);
        assert_eq!(rgba_to_nv12(&rgb, 2, 2).unwrap(), [81, 81, 81, 81, 90, 240]);
        assert!(rgba_to_nv12(&rgb, 1, 4).is_err());
    }
}
