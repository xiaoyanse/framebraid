use crate::{Error, Result};

/// CPU只读平面；调用方在本次读取期间保持拥有者，跨线程寿命由导出适配层负责。
pub struct Pixels<'a> {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub planes: Vec<&'a [u8]>,
    pub strides: Vec<usize>,
    pub color_matrix: u32,
    pub color_range: u32,
}
impl Pixels<'_> {
    pub fn validate(&self) -> Result<()> {
        let bad = || Error("vision.imageViewInvalid".into());
        if self.width == 0 || self.height == 0 || self.width > 8192 || self.height > 8192 {
            return Err(bad());
        }
        let rows = match self.format {
            0 => vec![(self.width as usize * 4, self.height as usize)],
            1 | 2 => vec![(self.width as usize * 3, self.height as usize)],
            3 if self.width.is_multiple_of(2)
                && self.height.is_multiple_of(2)
                && self.color_matrix <= 2
                && self.color_range <= 1 =>
            {
                vec![
                    (self.width as usize, self.height as usize),
                    (self.width as usize, self.height as usize / 2),
                ]
            }
            _ => return Err(bad()),
        };
        if self.planes.len() != rows.len() || self.strides.len() != rows.len() {
            return Err(bad());
        }
        for (i, (width, height)) in rows.iter().enumerate() {
            if self.strides[i] < *width
                || self.strides[i]
                    .checked_mul(height - 1)
                    .and_then(|n| n.checked_add(*width))
                    .is_none_or(|n| n > self.planes[i].len())
            {
                return Err(bad());
            }
        }
        Ok(())
    }
    pub fn rgb(&self, x: usize, y: usize) -> [f32; 3] {
        let row = y * self.strides[0];
        match self.format {
            0 | 1 => {
                let i = row + x * if self.format == 0 { 4 } else { 3 };
                std::array::from_fn(|c| self.planes[0][i + c] as f32)
            }
            2 => {
                let i = row + x * 3;
                std::array::from_fn(|c| self.planes[0][i + 2 - c] as f32)
            }
            _ => {
                let yy = self.planes[0][row + x] as f32;
                let uv = (y / 2) * self.strides[1] + (x / 2) * 2;
                let mut u = self.planes[1][uv] as f32 - 128.0;
                let mut v = self.planes[1][uv + 1] as f32 - 128.0;
                let luma = if self.color_range == 0 {
                    u *= 255.0 / 224.0;
                    v *= 255.0 / 224.0;
                    (yy - 16.0) * 255.0 / 219.0
                } else {
                    yy
                };
                // 公开枚举是601=0/709=1/2020=2，不能沿用其他库的枚举偏移。
                let (r, gu, gv, b) = match self.color_matrix {
                    1 => (1.5748, 0.187324, 0.468124, 1.8556),
                    2 => (1.4746, 0.164553, 0.571353, 1.8814),
                    _ => (1.402, 0.344136, 0.714136, 1.772),
                };
                [luma + r * v, luma - gu * u - gv * v, luma + b * u].map(|v| v.clamp(0.0, 255.0))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nv12_uses_public_matrix_numbers_and_range() {
        let y = [81; 4];
        let uv = [90, 240];
        let mut pixels = Pixels {
            width: 2,
            height: 2,
            format: 3,
            planes: vec![&y, &uv],
            strides: vec![2, 2],
            color_matrix: 0,
            color_range: 0,
        };
        pixels.validate().unwrap();
        let red = pixels.rgb(0, 0);
        assert!(red[0] > 250.0 && red[1] < 1.0 && red[2] < 1.0);
        pixels.color_matrix = 1;
        assert!(pixels.rgb(0, 0)[1] > 20.0);
        pixels.color_matrix = 2;
        assert!((8.0..14.0).contains(&pixels.rgb(0, 0)[1]));
        pixels.color_matrix = 3;
        assert!(pixels.validate().is_err());
        pixels.color_matrix = 0;
        let neutral = [128, 128];
        let black = [16; 4];
        pixels.planes = vec![&black, &neutral];
        assert_eq!(pixels.rgb(0, 0), [0.0; 3]);
        pixels.color_range = 1;
        assert_eq!(pixels.rgb(0, 0), [16.0; 3]);
    }
}
