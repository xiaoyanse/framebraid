use super::{results::Candidate, ImageMapping, RotatedDetection};

pub(super) fn iou(a: [f32; 4], b: [f32; 4]) -> f32 {
    let intersection =
        (a[2].min(b[2]) - a[0].max(b[0])).max(0.0) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    intersection
        / (((a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - intersection)
            .max(f32::EPSILON))
}

fn covariance(c: &Candidate) -> [f32; 3] {
    let a = c.xywh[2].powi(2) / 12.0;
    let b = c.xywh[3].powi(2) / 12.0;
    let (sin, cos) = c.angle.sin_cos();
    [
        a * cos * cos + b * sin * sin,
        a * sin * sin + b * cos * cos,
        (a - b) * cos * sin,
    ]
}

/// 按高斯矩近似旋转相似度，避免把多边形IoU当作同一个NMS指标。
pub(super) fn probiou(a: &Candidate, b: &Candidate) -> f32 {
    let [a1, b1, c1] = covariance(a);
    let [a2, b2, c2] = covariance(b);
    let dx = a.xywh[0] - b.xywh[0];
    let dy = a.xywh[1] - b.xywh[1];
    let determinant = (a1 + a2) * (b1 + b2) - (c1 + c2).powi(2);
    let eps = 1e-7;
    let first = ((a1 + a2) * dy * dy + (b1 + b2) * dx * dx) / (determinant + eps) * 0.25;
    let second = -(c1 + c2) * dx * dy / (determinant + eps) * 0.5;
    let third = (determinant
        / (4.0 * ((a1 * b1 - c1 * c1).max(0.0) * (a2 * b2 - c2 * c2).max(0.0)).sqrt() + eps)
        + eps)
        .ln()
        * 0.5;
    let distance = (first + second + third).clamp(eps, 100.0);
    1.0 - (1.0 - (-distance).exp() + eps).sqrt()
}

pub(super) fn rotated(c: &Candidate, mapping: ImageMapping) -> RotatedDetection {
    let angle = c.angle.sin().atan2(c.angle.cos());
    let (sin, cos) = angle.sin_cos();
    let u = [cos * c.xywh[2] / 2.0, sin * c.xywh[2] / 2.0];
    let v = [-sin * c.xywh[3] / 2.0, cos * c.xywh[3] / 2.0];
    let vertices = [[1.0, 1.0], [-1.0, 1.0], [-1.0, -1.0], [1.0, -1.0]].map(|[su, sv]| {
        mapping.point(
            c.xywh[0] + su * u[0] + sv * v[0],
            c.xywh[1] + su * u[1] + sv * v[1],
        )
    });
    RotatedDetection {
        vertices,
        center: mapping.point(c.xywh[0], c.xywh[1]),
        width: c.xywh[2] / mapping.scale / mapping.width as f32,
        height: c.xywh[3] / mapping.scale / mapping.height as f32,
        angle,
        confidence: c.confidence,
        class_id: c.class_id,
    }
}
