use super::{results::Candidate, ImageMapping};
use crate::{Error, Result};

pub(super) fn check_budget(mapping: ImageMapping, count: usize, budget: usize) -> Result<()> {
    let bytes = (mapping.width as usize)
        .div_ceil(8)
        .checked_mul(mapping.height as usize)
        .and_then(|v| v.checked_mul(count));
    if bytes.is_none_or(|bytes| bytes > budget) {
        return Err(Error("sdk.resourceExhausted".into()));
    }
    Ok(())
}

pub(super) fn compose(
    head: &[f32],
    prototypes: &[f32],
    anchors: usize,
    classes: usize,
    candidate: &Candidate,
    mapping: ImageMapping,
) -> Result<Vec<u8>> {
    const SIDE: usize = 160;
    let mut logits = vec![0.0f32; SIDE * SIDE];
    for channel in 0..32 {
        let coefficient = head[(4 + classes + channel) * anchors + candidate.anchor];
        for (value, prototype) in logits
            .iter_mut()
            .zip(&prototypes[channel * SIDE * SIDE..(channel + 1) * SIDE * SIDE])
        {
            *value += coefficient * prototype;
        }
    }
    if logits.iter().any(|v| !v.is_finite()) {
        return Err(Error("vision.inferenceFailed|maskOverflow".into()));
    }
    let stride = (mapping.width as usize).div_ceil(8);
    let mut bits = vec![0u8; stride * mapping.height as usize];
    let visible_width = (mapping.width as f32 * mapping.scale).round().max(1.0) as u32;
    let visible_height = (mapping.height as f32 * mapping.scale).round().max(1.0) as u32;
    for y in 0..mapping.height {
        for x in 0..mapping.width {
            // 最近邻从去填充后的输入二值掩膜取样，不对原型先阈值或先裁切。
            let ix = mapping.left + (x as u64 * visible_width as u64 / mapping.width as u64) as u32;
            let iy =
                mapping.top + (y as u64 * visible_height as u64 / mapping.height as u64) as u32;
            if (ix as f32) < candidate.bounds[0]
                || (iy as f32) < candidate.bounds[1]
                || ix as f32 >= candidate.bounds[2]
                || iy as f32 >= candidate.bounds[3]
            {
                continue;
            }
            if bilinear(&logits, ix, iy) > 0.0 {
                bits[y as usize * stride + x as usize / 8] |= 1 << (x % 8);
            }
        }
    }
    Ok(bits)
}

fn bilinear(logits: &[f32], x: u32, y: u32) -> f32 {
    let px = ((x as f32 + 0.5) / 4.0 - 0.5).clamp(0.0, 159.0);
    let py = ((y as f32 + 0.5) / 4.0 - 0.5).clamp(0.0, 159.0);
    let x0 = px.floor() as usize;
    let y0 = py.floor() as usize;
    let x1 = (x0 + 1).min(159);
    let y1 = (y0 + 1).min(159);
    let dx = px - x0 as f32;
    let dy = py - y0 as f32;
    (logits[y0 * 160 + x0] * (1.0 - dx) + logits[y0 * 160 + x1] * dx) * (1.0 - dy)
        + (logits[y1 * 160 + x0] * (1.0 - dx) + logits[y1 * 160 + x1] * dx) * dy
}
