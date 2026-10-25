use super::{geometry, mask, ImageMapping, Options, Profile, Task};
use crate::{Error, PoseDetection, Result};
use serde::Serialize;

pub struct TensorView<'a> {
    pub name: &'a str,
    pub shape: &'a [usize],
    pub values: &'a [f32],
}

#[derive(Debug, Clone, Serialize)]
pub struct Detection {
    pub bounds: [f32; 4],
    pub confidence: f32,
    pub class_id: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct ClassScore {
    pub class_id: u32,
    pub probability: f32,
}
#[derive(Debug, Clone, Serialize)]
pub struct RotatedDetection {
    pub vertices: [[f32; 2]; 4],
    pub center: [f32; 2],
    pub width: f32,
    pub height: f32,
    pub angle: f32,
    pub confidence: f32,
    pub class_id: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub detection: Detection,
    pub mask_width: u32,
    pub mask_height: u32,
    pub mask_stride: usize,
    pub mask_bits: Vec<u8>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "task", content = "instances", rename_all = "lowercase")]
pub enum Output {
    Pose(Vec<PoseDetection>),
    Detect(Vec<Detection>),
    Classify(Vec<ClassScore>),
    Segment(Vec<Segment>),
    Obb(Vec<RotatedDetection>),
}
#[derive(Debug, Clone, Serialize)]
pub struct Prediction {
    pub profile_id: String,
    pub width: u32,
    pub height: u32,
    pub output: Output,
}

pub(super) struct Candidate {
    pub anchor: usize,
    pub xywh: [f32; 4],
    pub bounds: [f32; 4],
    pub confidence: f32,
    pub class_id: u32,
    pub angle: f32,
}

pub(super) fn classify(values: &[f32], top_k: usize) -> Result<Vec<ClassScore>> {
    if values.iter().any(|v| !(0.0..=1.0).contains(v))
        || (values.iter().sum::<f32>() - 1.0).abs() > 0.01
    {
        return Err(Error(
            "vision.formatIncompatible|probabilityOutputExpected".into(),
        ));
    }
    let mut scores: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(class_id, &probability)| ClassScore {
            class_id: class_id as u32,
            probability,
        })
        .collect();
    // 概率档案不二次softmax，等分时用类别编号保证可复现。
    scores.sort_by(|a, b| {
        b.probability
            .total_cmp(&a.probability)
            .then(a.class_id.cmp(&b.class_id))
    });
    scores.truncate(top_k);
    Ok(scores)
}

pub(super) fn decode_head(
    task: Task,
    profile: &Profile,
    outputs: &[&[f32]],
    mapping: ImageMapping,
    options: Options,
) -> Result<Output> {
    let classes = profile
        .classes
        .ok_or_else(|| Error("vision.profileUnsupported".into()))?;
    let anchors = profile.outputs[0].shape[2];
    let head = outputs[0];
    let mut candidates = Vec::new();
    for anchor in 0..anchors {
        let mut best = (0usize, 0.0f32);
        for class in 0..classes {
            let score = head[(4 + class) * anchors + anchor];
            if !(0.0..=1.0).contains(&score) {
                return Err(Error(
                    "vision.formatIncompatible|classProbabilityExpected".into(),
                ));
            }
            if score > best.1 {
                best = (class, score);
            }
        }
        if best.1 <= options.confidence {
            continue;
        }
        let xywh = [
            head[anchor],
            head[anchors + anchor],
            head[2 * anchors + anchor],
            head[3 * anchors + anchor],
        ];
        if xywh.iter().any(|value| value.abs() > 1_000_000.0) {
            return Err(Error("vision.inferenceFailed|boxRangeInvalid".into()));
        }
        if xywh[2] <= 0.0 || xywh[3] <= 0.0 {
            continue;
        }
        let bounds = [
            xywh[0] - xywh[2] / 2.0,
            xywh[1] - xywh[3] / 2.0,
            xywh[0] + xywh[2] / 2.0,
            xywh[1] + xywh[3] / 2.0,
        ];
        let angle = if task == Task::Obb {
            head[(4 + classes) * anchors + anchor]
        } else {
            0.0
        };
        if task != Task::Obb {
            let visible = mapping.bounds(bounds);
            if visible[2] <= visible[0] || visible[3] <= visible[1] {
                continue;
            }
        }
        candidates.push(Candidate {
            anchor,
            xywh,
            bounds,
            confidence: best.1,
            class_id: best.0 as u32,
            angle,
        });
    }
    candidates.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then(a.anchor.cmp(&b.anchor))
    });
    let mut selected: Vec<&Candidate> = Vec::new();
    for (i, candidate) in candidates.iter().enumerate() {
        // OBB使用档案版本的Fast NMS；即使较高分框本身被抑制，也参与后续抑制。
        let suppressed = if task == Task::Obb {
            candidates[..i].iter().any(|prior| {
                prior.class_id == candidate.class_id
                    && geometry::probiou(prior, candidate) >= options.nms_iou
            })
        } else {
            selected.iter().any(|prior| {
                prior.class_id == candidate.class_id
                    && geometry::iou(prior.bounds, candidate.bounds) > options.nms_iou
            })
        };
        if !suppressed {
            selected.push(candidate);
        }
        if selected.len() == options.max_detections {
            break;
        }
    }
    let detection = |candidate: &Candidate| Detection {
        bounds: mapping.bounds(candidate.bounds),
        confidence: candidate.confidence,
        class_id: candidate.class_id,
    };
    match task {
        Task::Detect => Ok(Output::Detect(
            selected.iter().map(|c| detection(c)).collect(),
        )),
        Task::Obb => Ok(Output::Obb(
            selected
                .iter()
                .map(|c| geometry::rotated(c, mapping))
                .collect(),
        )),
        Task::Segment => {
            mask::check_budget(mapping, selected.len(), options.mask_budget_bytes)?;
            selected
                .iter()
                .map(|candidate| {
                    let bits =
                        mask::compose(head, outputs[1], anchors, classes, candidate, mapping)?;
                    Ok(Segment {
                        detection: detection(candidate),
                        mask_width: mapping.width,
                        mask_height: mapping.height,
                        mask_stride: (mapping.width as usize).div_ceil(8),
                        mask_bits: bits,
                    })
                })
                .collect::<Result<Vec<_>>>()
                .map(Output::Segment)
        }
        _ => Err(Error("vision.profileUnsupported".into())),
    }
}
