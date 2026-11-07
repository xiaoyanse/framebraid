//! 单帧图像导出（§2.11）：宿主持有的像素平面 → JPEG/PNG 字节。无状态纯函数，
//! 不占 amc_encoder 额度；JPEG 核心为 jpeg-encoder 纯 Rust 编码器（4:4:4 +
//! Annex K 量化表，质量刻度与 libjpeg/image crate 同源；AVX2 运行时检测，无
//! AVX2 自动回退标量），PNG 沿用 image crate；NV12 换算系数与 am-vision 侧
//! Pixels::rgb 同源（两库同像同色）。
use image::{codecs::png::PngEncoder, ImageEncoder, RgbImage};
use jpeg_encoder::{Encoder as JpegEncoderCore, SamplingFactor};

/// 像素格式值（与 am_pixel_format_e 对齐；本 crate 不依赖 sdk-support）。
pub const PF_RGBA8: u32 = 0;
pub const PF_RGB8: u32 = 1;
pub const PF_BGR8: u32 = 2;
pub const PF_NV12: u32 = 3;
/// 输出格式：JPEG。
pub const FORMAT_JPEG: u32 = 1;
/// 输出格式：PNG。
pub const FORMAT_PNG: u32 = 2;
/// JPEG 质量零值默认（opts.quality=0 时生效）。
pub const QUALITY_JPEG_DEFAULT: u32 = 85;
/// 输出字节上限（合同 §2.11）。
pub const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
/// 单边尺寸上限。
pub const MAX_DIMENSION: u32 = 8192;

#[derive(Debug, PartialEq)]
pub enum ExportError {
    PixelInvalid,
    FormatInvalid,
    EncodeFailed,
    OutputTooLarge,
}

/// 帧平面视图（FFI 侧完成指针/边界校验后组装）。
pub struct FrameInput<'a> {
    pub planes: &'a [&'a [u8]],
    pub strides: &'a [usize],
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub color_matrix: u32,
    pub color_range: u32,
}

/// NV12→RGB（BT.601/709/2020，limited/full；系数同 am-vision Pixels::rgb）。
fn nv12_to_rgb(input: &FrameInput<'_>) -> RgbImage {
    let (w, h) = (input.width as usize, input.height as usize);
    let mut image = RgbImage::new(input.width, input.height);
    for y in 0..h {
        let y_row = y * input.strides[0];
        let uv_row = (y / 2) * input.strides[1];
        for x in 0..w {
            let yy = input.planes[0][y_row + x] as f32;
            let uv = uv_row + (x / 2) * 2;
            let mut u = input.planes[1][uv] as f32 - 128.0;
            let mut v = input.planes[1][uv + 1] as f32 - 128.0;
            let luma = if input.color_range == 0 {
                u *= 255.0 / 224.0;
                v *= 255.0 / 224.0;
                (yy - 16.0) * 255.0 / 219.0
            } else {
                yy
            };
            let (r, gu, gv, b) = match input.color_matrix {
                1 => (1.5748, 0.187324, 0.468124, 1.8556),
                2 => (1.4746, 0.164553, 0.571353, 1.8814),
                _ => (1.402, 0.344136, 0.714136, 1.772),
            };
            let pixel = [luma + r * v, luma - gu * u - gv * v, luma + b * u];
            let px = image.get_pixel_mut(x as u32, y as u32);
            for c in 0..3 {
                px[c] = pixel[c].clamp(0.0, 255.0) as u8;
            }
        }
    }
    image
}

/// 组装 RgbImage：打包格式直接换算，NV12 走矩阵转换。
fn to_rgb_image(input: &FrameInput<'_>) -> Result<RgbImage, ExportError> {
    let mut image = RgbImage::new(input.width, input.height);
    let (w, h) = (input.width as usize, input.height as usize);
    match input.format {
        PF_RGB8 => {
            for y in 0..h {
                let row = y * input.strides[0];
                for x in 0..w {
                    let i = row + x * 3;
                    image
                        .get_pixel_mut(x as u32, y as u32)
                        .0
                        .copy_from_slice(&input.planes[0][i..i + 3]);
                }
            }
            Ok(image)
        }
        PF_BGR8 => {
            for y in 0..h {
                let row = y * input.strides[0];
                for x in 0..w {
                    let i = row + x * 3;
                    image
                        .get_pixel_mut(x as u32, y as u32)
                        .0
                        .copy_from_slice(&input.planes[0][i..i + 3]);
                    image.get_pixel_mut(x as u32, y as u32).0.reverse();
                }
            }
            Ok(image)
        }
        PF_RGBA8 => {
            for y in 0..h {
                let row = y * input.strides[0];
                for x in 0..w {
                    let i = row + x * 4;
                    image
                        .get_pixel_mut(x as u32, y as u32)
                        .0
                        .copy_from_slice(&input.planes[0][i..i + 3]);
                }
            }
            Ok(image)
        }
        PF_NV12 => Ok(nv12_to_rgb(input)),
        _ => Err(ExportError::PixelInvalid),
    }
}

/// 编码单帧：format=FORMAT_JPEG/FORMAT_PNG；quality 仅 JPEG（FFI 层已解默认与范围）。
pub fn encode(input: &FrameInput<'_>, format: u32, quality: u32) -> Result<Vec<u8>, ExportError> {
    if input.width == 0
        || input.height == 0
        || input.width > MAX_DIMENSION
        || input.height > MAX_DIMENSION
    {
        return Err(ExportError::PixelInvalid);
    }
    let image = to_rgb_image(input)?;
    let mut bytes = Vec::new();
    match format {
        FORMAT_JPEG => {
            let mut encoder = JpegEncoderCore::new(&mut bytes, quality.clamp(1, 100) as u8);
            // 4:4:4 对齐 image crate 旧底座实际输出（SOF0 取证为 h1v1；其文档标
            // 4:2:2 与实际不符），量化表/缩放公式同源，质量参数语义不变。
            encoder.set_sampling_factor(SamplingFactor::F_1_1);
            encoder
                .encode(
                    image.as_raw(),
                    input.width as u16,
                    input.height as u16,
                    jpeg_encoder::ColorType::Rgb,
                )
                .map_err(|_| ExportError::EncodeFailed)?;
        }
        FORMAT_PNG => {
            let encoder = PngEncoder::new(&mut bytes);
            encoder
                .write_image(
                    image.as_raw(),
                    input.width,
                    input.height,
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|_| ExportError::EncodeFailed)?;
        }
        _ => return Err(ExportError::FormatInvalid),
    }
    if bytes.len() > MAX_OUTPUT_BYTES {
        return Err(ExportError::OutputTooLarge);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 棋盘格 RGB 平面（偶格 [200,40,90]，奇格黑）。
    fn checkerboard(w: u32, h: u32) -> Vec<u8> {
        let mut data = vec![0u8; w as usize * h as usize * 3];
        for y in 0..h as usize {
            for x in 0..w as usize {
                if (x + y) % 2 == 0 {
                    let i = (y * w as usize + x) * 3;
                    data[i..i + 3].copy_from_slice(&[200, 40, 90]);
                }
            }
        }
        data
    }

    fn packed_input<'a>(
        data: &'a [u8],
        w: u32,
        h: u32,
        format: u32,
    ) -> (Vec<&'a [u8]>, Vec<usize>, FrameInput<'a>) {
        let bpp = match format {
            PF_RGBA8 => 4,
            PF_NV12 => 1,
            _ => 3,
        };
        let planes = vec![data as &[u8]];
        let strides = vec![w as usize * bpp];
        let input = FrameInput {
            planes: &[],
            strides: &[],
            width: w,
            height: h,
            format,
            color_matrix: 0,
            color_range: 0,
        };
        (planes, strides, input)
    }

    #[test]
    fn jpeg_roundtrip_rgb_preserves_dimensions_and_center_pixel() {
        let data = checkerboard(8, 8);
        let (planes, strides, mut input) = packed_input(&data, 8, 8, PF_RGB8);
        input.planes = &planes;
        input.strides = &strides;
        let bytes = encode(&input, FORMAT_JPEG, 85).expect("jpeg");
        assert!(bytes.starts_with(&[0xFF, 0xD8]), "SOI magic");
        let decoded = image::load_from_memory(&bytes).expect("decode");
        assert_eq!((decoded.width(), decoded.height()), (8, 8));
        let corner = decoded.to_rgb8().get_pixel(0, 0).0;
        assert!(
            (corner[0] as i32 - 200).abs() < 24,
            "red channel near, got {corner:?}"
        );
    }

    #[test]
    fn png_roundtrip_is_lossless() {
        let data = checkerboard(6, 4);
        let (planes, strides, mut input) = packed_input(&data, 6, 4, PF_RGB8);
        input.planes = &planes;
        input.strides = &strides;
        let bytes = encode(&input, FORMAT_PNG, 0).expect("png");
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "PNG magic");
        let decoded = image::load_from_memory(&bytes).expect("decode");
        assert_eq!(decoded.to_rgb8().get_pixel(0, 0).0, [200, 40, 90]);
        assert_eq!(decoded.to_rgb8().get_pixel(1, 0).0, [0, 0, 0]);
    }

    #[test]
    fn lower_quality_yields_smaller_jpeg() {
        // 噪声图使质量差异显著
        let mut noisy = checkerboard(64, 64);
        for (i, b) in noisy.iter_mut().enumerate() {
            *b = b.wrapping_add((i * 31 % 251) as u8);
        }
        let (planes, strides, mut input) = packed_input(&noisy, 64, 64, PF_RGB8);
        input.planes = &planes;
        input.strides = &strides;
        let high = encode(&input, FORMAT_JPEG, 95).expect("high");
        let low = encode(&input, FORMAT_JPEG, 10).expect("low");
        assert!(
            low.len() < high.len(),
            "quality orders size: {} vs {}",
            low.len(),
            high.len()
        );
    }

    #[test]
    fn jpeg_quality_scale_parity_with_reference_encoder() {
        // 新核心与 image crate 参考实现量化口径同源（Annex K 表 + libjpeg 缩放
        // 公式 + 4:4:4 采样）：同图同质量输出尺寸应在 ±3% 内。上游换表、改采样
        // 因子或质量刻度漂移都会打破此界，作为接入方无感的守门断言。
        let mut noisy = checkerboard(64, 64);
        for (i, b) in noisy.iter_mut().enumerate() {
            *b = b.wrapping_add((i * 31 % 251) as u8);
        }
        let (planes, strides, mut input) = packed_input(&noisy, 64, 64, PF_RGB8);
        input.planes = &planes;
        input.strides = &strides;
        let new_bytes = encode(&input, FORMAT_JPEG, 85).expect("new core");
        let mut reference = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut reference, 85);
        encoder
            .write_image(&noisy, 64, 64, image::ExtendedColorType::Rgb8)
            .expect("reference");
        let ratio = new_bytes.len() as f64 / reference.len() as f64;
        assert!(
            (0.97..=1.03).contains(&ratio),
            "quality scale parity ratio {ratio}"
        );
    }

    #[test]
    fn rgba_and_bgr_assemble_correctly() {
        // RGBA：alpha 丢弃，RGB 前三字节
        let rgba: Vec<u8> = [10u8, 20, 30, 255]
            .iter()
            .cycle()
            .take(16)
            .cloned()
            .collect();
        let (planes, strides, mut input) = packed_input(&rgba, 2, 2, PF_RGBA8);
        input.planes = &planes;
        input.strides = &strides;
        let bytes = encode(&input, FORMAT_PNG, 0).expect("png");
        let decoded = image::load_from_memory(&bytes).expect("decode");
        assert_eq!(decoded.to_rgb8().get_pixel(0, 0).0, [10, 20, 30]);
        // BGR：换序还原
        let bgr: Vec<u8> = [30u8, 20, 10].iter().cycle().take(12).cloned().collect();
        let (planes, strides, mut input) = packed_input(&bgr, 2, 2, PF_BGR8);
        input.planes = &planes;
        input.strides = &strides;
        let bytes = encode(&input, FORMAT_PNG, 0).expect("png");
        let decoded = image::load_from_memory(&bytes).expect("decode");
        assert_eq!(decoded.to_rgb8().get_pixel(0, 0).0, [10, 20, 30]);
    }

    #[test]
    fn nv12_gray_plane_encodes_near_neutral() {
        // NV12 全 Y=128（limited）+ UV=128（无色差）→ 近中性灰（≈130）
        let (w, h) = (4usize, 4usize);
        let luma = vec![128u8; w * h];
        let chroma = vec![128u8; w * h / 2];
        let planes = vec![luma.as_slice(), chroma.as_slice()];
        let strides = vec![w, w];
        let input = FrameInput {
            planes: &planes,
            strides: &strides,
            width: w as u32,
            height: h as u32,
            format: PF_NV12,
            color_matrix: 0,
            color_range: 0,
        };
        let bytes = encode(&input, FORMAT_PNG, 0).expect("png");
        let decoded = image::load_from_memory(&bytes).expect("decode");
        let p = decoded.to_rgb8().get_pixel(0, 0).0;
        for c in p {
            assert!((c as i32 - 130).abs() < 6, "neutral gray, got {p:?}");
        }
    }

    #[test]
    fn rejects_bad_format_and_dimensions() {
        let data = checkerboard(4, 4);
        let (planes, strides, mut input) = packed_input(&data, 4, 4, PF_RGB8);
        input.planes = &planes;
        input.strides = &strides;
        assert_eq!(encode(&input, 9, 0), Err(ExportError::FormatInvalid));
        let (empty_planes, empty_strides) = (Vec::new(), Vec::new());
        let big = FrameInput {
            planes: &empty_planes,
            strides: &empty_strides,
            width: 8193,
            height: 8,
            format: PF_RGB8,
            color_matrix: 0,
            color_range: 0,
        };
        assert_eq!(encode(&big, FORMAT_PNG, 0), Err(ExportError::PixelInvalid));
    }

    #[test]
    fn contract_constants_are_frozen() {
        // 32MiB 上限与格式值合同冻结（§2.11）
        assert_eq!(MAX_OUTPUT_BYTES, 32 * 1024 * 1024);
        assert_eq!(FORMAT_JPEG, 1);
        assert_eq!(FORMAT_PNG, 2);
        assert_eq!(MAX_DIMENSION, 8192);
    }
}
