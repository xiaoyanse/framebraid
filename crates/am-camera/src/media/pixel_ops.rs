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
        let ty = ((y as f64 + 0.5) * oh as f64 / nh as f64 - 0.5).clamp(0., (oh - 1) as f64);
        if method == 0 {
            let sy = ty.round() as usize;
            for x in 0..nw {
                let tx =
                    ((x as f64 + 0.5) * ow as f64 / nw as f64 - 0.5).clamp(0., (ow - 1) as f64);
                let source_offset = (sy * ow + tx.round() as usize) * channels;
                let target_offset = (y * nw + x) * channels;
                target[target_offset..target_offset + channels]
                    .copy_from_slice(&source[source_offset..source_offset + channels]);
            }
        } else {
            for x in 0..nw {
                let tx =
                    ((x as f64 + 0.5) * ow as f64 / nw as f64 - 0.5).clamp(0., (ow - 1) as f64);
                for c in 0..channels {
                    let sample =
                        |xx: usize, yy: usize| source[(yy * ow + xx) * channels + c] as f64;
                    let (x0, y0) = (tx.floor() as usize, ty.floor() as usize);
                    let (dx, dy) = (tx - x0 as f64, ty - y0 as f64);
                    let x1 = (x0 + 1).min(ow - 1);
                    let y1 = (y0 + 1).min(oh - 1);
                    let value = (sample(x0, y0) * (1. - dx) + sample(x1, y0) * dx) * (1. - dy)
                        + (sample(x0, y1) * (1. - dx) + sample(x1, y1) * dx) * dy;
                    target[(y * nw + x) * channels + c] = value.round().clamp(0., 255.) as u8;
                }
            }
        }
    }
    Ok(())
}
struct Nv12Tables {
    y_rg: Vec<f64>,
    u_rg: Vec<f64>,
    v_rg: Vec<f64>,
    y_b: [f64; 256],
    u_b: [f64; 256],
    v_b: [f64; 256],
}
// 表项求值顺序与冻结f64公式逐位相同：Y=((16+yr)+yg)+yb，U/V=(rg项)+b项+128。
fn nv12_tables() -> &'static Nv12Tables {
    static TABLES: std::sync::OnceLock<Nv12Tables> = std::sync::OnceLock::new();
    TABLES.get_or_init(|| {
        let mut tables = Nv12Tables {
            y_rg: vec![0f64; 256 * 256],
            u_rg: vec![0f64; 256 * 256],
            v_rg: vec![0f64; 256 * 256],
            y_b: [0f64; 256],
            u_b: [0f64; 256],
            v_b: [0f64; 256],
        };
        for r in 0..256usize {
            for g in 0..256usize {
                let (rf, gf) = (r as f64, g as f64);
                let at = r * 256 + g;
                tables.y_rg[at] = 16. + 0.256788 * rf + 0.504129 * gf;
                tables.u_rg[at] = -0.148223 * rf - 0.290993 * gf;
                tables.v_rg[at] = 0.439216 * rf - 0.367788 * gf;
            }
        }
        for b in 0..256usize {
            let bf = b as f64;
            tables.y_b[b] = 0.097906 * bf;
            tables.u_b[b] = 0.439216 * bf;
            tables.v_b[b] = -0.071427 * bf;
        }
        tables
    })
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
    // BT.601系数含6位十进制小数，256^3内存在精确半整数边界（如rgb(7,39,43)的
    // U=134.5），定点/SIMD重写会翻转f64 round的half-away结果；因此保留f64数值
    // 路径，仅把乘法预计算进进程级RG表，输出逐字节一致由exhaustive_solid_blocks
    // 与random_mixed_blocks穷举/抽样对拍保证。
    let tables = nv12_tables();
    let mut out = vec![0; w * h * 3 / 2];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let (r, g, b) = (
                source[i] as usize,
                source[i + 1] as usize,
                source[i + 2] as usize,
            );
            let rg = r * 256 + g;
            out[y * w + x] = (tables.y_rg[rg] + tables.y_b[b]).round().clamp(16., 235.) as u8;
            if x % 2 == 0 && y % 2 == 0 {
                let (mut u, mut v) = (0f64, 0f64);
                for dy in 0..2 {
                    for dx in 0..2 {
                        let i = ((y + dy) * w + x + dx) * 4;
                        let (r, g, b) = (
                            source[i] as usize,
                            source[i + 1] as usize,
                            source[i + 2] as usize,
                        );
                        let rg = r * 256 + g;
                        u += tables.u_rg[rg] + tables.u_b[b] + 128.;
                        v += tables.v_rg[rg] + tables.v_b[b] + 128.;
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
    // 冻结f64参考：定点LUT重写前的逐字节契约，禁止随实现漂移。
    fn frozen_f64_rgba_to_nv12(source: &[u8], w: u32, h: u32) -> Vec<u8> {
        let (w, h) = (w as usize, h as usize);
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
                            let p =
                                uv(source[i] as f64, source[i + 1] as f64, source[i + 2] as f64);
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
        out
    }

    fn solid_2x2(r: u8, g: u8, b: u8) -> [u8; 16] {
        let mut frame = [0u8; 16];
        for pixel in frame.chunks_exact_mut(4) {
            pixel[0] = r;
            pixel[1] = g;
            pixel[2] = b;
            pixel[3] = 255;
        }
        frame
    }

    #[test]
    fn exhaustive_solid_blocks_match_frozen_f64_reference() {
        // 256^3同色2x2块穷举：Y逐像素完备，U/V覆盖全部单像素值与自平均块。
        for r in 0..=255u32 {
            for g in 0..=255u32 {
                for b in 0..=255u32 {
                    let frame = solid_2x2(r as u8, g as u8, b as u8);
                    let expected = frozen_f64_rgba_to_nv12(&frame, 2, 2);
                    let actual = rgba_to_nv12(&frame, 2, 2).unwrap();
                    assert_eq!(actual, expected, "mismatch at rgb({r},{g},{b})");
                }
            }
        }
    }

    #[test]
    fn random_mixed_blocks_match_frozen_f64_reference() {
        // 定点误差上界~1.4e-6，异色块只在和值贴近半整数边界时可能翻转；
        // 随机混合块抽样冒烟，同色穷举与误差分析为主要证据。
        let mut state = 0x2545F4914F6CDD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut frame = [0u8; 16];
        for _ in 0..2_000_000u32 {
            for byte in frame.iter_mut() {
                *byte = next() as u8;
            }
            for pixel in frame.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
            let expected = frozen_f64_rgba_to_nv12(&frame, 2, 2);
            let actual = rgba_to_nv12(&frame, 2, 2).unwrap();
            assert_eq!(actual, expected);
        }
    }

    use super::*;
    #[test]
    fn nearest_matches_frozen_half_pixel_reference() {
        for ow in 1..=9u32 {
            for oh in 1..=7u32 {
                for nw in 1..=8u32 {
                    for nh in 1..=6u32 {
                        for channels in 1..=4usize {
                            let source: Vec<u8> = (0..ow as usize * oh as usize * channels)
                                .map(|i| ((i * 73 + 19) % 251) as u8)
                                .collect();
                            let mut result = vec![0; nw as usize * nh as usize * channels];
                            resize_plane(&source, (ow, oh), &mut result, (nw, nh), channels, 0)
                                .unwrap();
                            let mut expected = vec![0; result.len()];
                            for y in 0..nh as usize {
                                for x in 0..nw as usize {
                                    let tx = ((x as f64 + 0.5) * ow as f64 / nw as f64 - 0.5)
                                        .clamp(0., (ow - 1) as f64);
                                    let ty = ((y as f64 + 0.5) * oh as f64 / nh as f64 - 0.5)
                                        .clamp(0., (oh - 1) as f64);
                                    for c in 0..channels {
                                        expected[(y * nw as usize + x) * channels + c] =
                                            source[(ty.round() as usize * ow as usize
                                                + tx.round() as usize)
                                                * channels
                                                + c];
                                    }
                                }
                            }
                            assert_eq!(
                                result, expected,
                                "{ow}x{oh} -> {nw}x{nh}, channels={channels}"
                            );
                        }
                    }
                }
            }
        }
    }

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
