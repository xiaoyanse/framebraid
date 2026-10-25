use crate::{preprocess::Mapping, Error, Keypoint, PoseDetection, Result};

pub const KEYPOINT_NAMES: [&str; 17] = [
    "nose",
    "left_eye",
    "right_eye",
    "left_ear",
    "right_ear",
    "left_shoulder",
    "right_shoulder",
    "left_elbow",
    "right_elbow",
    "left_wrist",
    "right_wrist",
    "left_hip",
    "right_hip",
    "left_knee",
    "right_knee",
    "left_ankle",
    "right_ankle",
];

pub fn decode(values: &[f32], map: &Mapping, threshold: f32) -> Result<Vec<PoseDetection>> {
    if values.len() != 300 * 57 || !values.iter().all(|v| v.is_finite()) {
        return Err(Error("vision.outputInvalid".into()));
    }
    let x = |value: f32| (value - map.left as f32) / map.scale / map.width as f32;
    let y = |value: f32| (value - map.top as f32) / map.scale / map.height as f32;
    let mut detections = Vec::new();
    for row in values.chunks_exact(57) {
        if row[4] < threshold || row[4] <= 0.0 {
            continue;
        }
        if !(0.0..=1.0).contains(&row[4]) || row[5] != 0.0 {
            return Err(Error("vision.outputMismatch".into()));
        }
        let bounds = [
            x(row[0]).clamp(0.0, 1.0),
            y(row[1]).clamp(0.0, 1.0),
            x(row[2]).clamp(0.0, 1.0),
            y(row[3]).clamp(0.0, 1.0),
        ];
        if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
            continue;
        }
        let mut keypoints = Vec::with_capacity(17);
        for point in row[6..].chunks_exact(3) {
            let (px, py) = (x(point[0]), y(point[1]));
            if !(0.0..=1.0).contains(&point[2]) {
                return Err(Error("vision.keypointConfidenceInvalid".into()));
            }
            keypoints.push(Keypoint {
                x: px,
                y: py,
                confidence: if (0.0..=1.0).contains(&px) && (0.0..=1.0).contains(&py) {
                    point[2]
                } else {
                    0.0
                },
            });
        }
        detections.push(PoseDetection {
            bounds,
            confidence: row[4],
            keypoints,
        });
    }
    Ok(detections)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_letterbox_back_and_does_not_turn_padding_into_valid_points() {
        let map = Mapping {
            width: 1920,
            height: 1080,
            scale: 1.0 / 3.0,
            left: 0,
            top: 140,
        };
        let mut output = vec![0.0; 300 * 57];
        output[..6].copy_from_slice(&[0.0, 140.0, 640.0, 500.0, 0.9, 0.0]);
        output[6..9].copy_from_slice(&[320.0, 320.0, 0.8]);
        output[9..12].copy_from_slice(&[320.0, 100.0, 0.9]);
        let detections = decode(&output, &map, 0.25).unwrap();
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].bounds, [0.0, 0.0, 1.0, 1.0]);
        assert!((detections[0].keypoints[0].y - 0.5).abs() < 0.0001);
        assert_eq!(detections[0].keypoints[1].confidence, 0.0);
        output[0] = f32::NAN;
        assert!(decode(&output, &map, 0.25).is_err());
    }
}
