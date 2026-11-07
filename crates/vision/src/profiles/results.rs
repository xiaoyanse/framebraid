use super::{
    geometry, mask,
    registry::{CoordForm, Layout},
    ImageMapping, Options, Profile, Task,
};
use crate::{Error, PoseDetection, Result};
use serde::Serialize;

pub struct TensorView<'a> {
    pub name: &'a str,
    pub shape: &'a [usize],
    pub values: &'a [f32],
}

/// 开放词汇命中明细（A2）：与 Detect 框同序——条目索引（=class_id）+组内提示词行。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct VocabHit {
    pub entry_index: u32,
    pub prompt_index: u32,
}

/// vocab 检测结果伴生数据（A2/A4）：命中明细 + 条目 label 表（结果 class_name 源）。
#[derive(Debug, Clone, Serialize)]
pub struct VocabResult {
    pub hits: Vec<VocabHit>,
    pub labels: Vec<String>,
}

/// 词表布局（A2 解码用）：条目表 (label, prompt_rows)，行区间由前缀和确定。
pub struct VocabLayout {
    pub entries: Vec<(String, u32)>,
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
    /// 开放词汇命中明细（A2）：vocab 检测路径与 Detect 框同序；其余 None。
    pub vocab_hits: Option<VocabResult>,
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

/// 开放词汇亲和头解码（A2，§4.1）：输出列为全部提示词行（Σ rows，4 坐标 + 每行
/// 一个亲和分列），每个 (anchor, prompt) 独立成候选；**组内跨提示词 NMS**——同条目
/// 内 IoU 超阈值的多提示词命中合并为单框保留最高分（hit 记保留词行号），组间独立
/// 不去重；class_id=条目索引。坐标语义复用档案 layout/coordForm 声明。
/// `actual`：output0 实际运行期形状（批 D 起 anchors 取实际末维——档案末维可为
/// 动态命名维（=0），且批拆分后带批轴 [1, rows, anchors]，批轴因子 1 不影响
/// 行主序索引；固定声明维经 decode() 前置校验与实际一致，语义不变）。
pub(super) fn decode_affinity(
    profile: &Profile,
    outputs: &[&[f32]],
    actual: &[usize],
    mapping: ImageMapping,
    options: Options,
    layout: &VocabLayout,
) -> Result<(Vec<Detection>, Vec<VocabHit>)> {
    let anchors = match profile.postprocess.layout {
        Layout::Transposed => actual[actual.len() - 1],
        Layout::RowMajor => actual[actual.len() - 2],
    };
    let head = outputs[0];
    let rows = head.len() / anchors.max(1);
    let prompt_rows: u32 = layout.entries.iter().map(|(_, n)| *n).sum();
    if anchors == 0 || rows < 4 + prompt_rows as usize || rows * anchors != head.len() {
        return Err(Error("vision.outputShapeMismatch|affinityHead".into()));
    }
    // 提示词列号 → 条目索引（前缀和）
    let mut starts = Vec::with_capacity(layout.entries.len());
    let mut acc = 0u32;
    for (_, n) in &layout.entries {
        starts.push(acc);
        acc += n;
    }
    struct AffinityCandidate {
        anchor: usize,
        entry: u32,
        prompt: u32,
        bounds: [f32; 4],
        confidence: f32,
    }
    let mut candidates = Vec::new();
    for anchor in 0..anchors {
        let at = |row| {
            head[match profile.postprocess.layout {
                Layout::Transposed => row * anchors + anchor,
                Layout::RowMajor => anchor * rows + row,
            }]
        };
        let coords = [at(0), at(1), at(2), at(3)];
        let xywh = match profile.postprocess.coord_form {
            CoordForm::Cxcywh => coords,
            CoordForm::Xyxy => [
                (coords[0] + coords[2]) / 2.0,
                (coords[1] + coords[3]) / 2.0,
                coords[2] - coords[0],
                coords[3] - coords[1],
            ],
        };
        if xywh.iter().any(|v| v.abs() > 1_000_000.0) || xywh[2] <= 0.0 || xywh[3] <= 0.0 {
            continue;
        }
        let bounds = match profile.postprocess.coord_form {
            CoordForm::Xyxy => coords,
            CoordForm::Cxcywh => [
                xywh[0] - xywh[2] / 2.0,
                xywh[1] - xywh[3] / 2.0,
                xywh[0] + xywh[2] / 2.0,
                xywh[1] + xywh[3] / 2.0,
            ],
        };
        let visible = mapping.bounds(bounds);
        if visible[2] <= visible[0] || visible[3] <= visible[1] {
            continue;
        }
        for col in 0..prompt_rows {
            let score = at(4 + col as usize);
            if !(0.0..=1.0).contains(&score) {
                return Err(Error(
                    "vision.formatIncompatible|classProbabilityExpected".into(),
                ));
            }
            if score <= options.confidence {
                continue;
            }
            // 求组：提示词列号落到的条目（前缀和区间扫描）
            let mut group = 0usize;
            for (i, &s) in starts.iter().enumerate() {
                if col >= s {
                    group = i;
                }
            }
            candidates.push(AffinityCandidate {
                anchor,
                entry: group as u32,
                prompt: col - starts[group],
                bounds,
                confidence: score,
            });
        }
    }
    candidates.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then(a.anchor.cmp(&b.anchor))
            .then(a.prompt.cmp(&b.prompt))
    });
    let mut selected: Vec<&AffinityCandidate> = Vec::new();
    for candidate in &candidates {
        // 组内 NMS：同条目跨提示词合并（保留最高分——排序保证先到者分高），
        // 组间独立（不同类目各自输出，不去重）。
        let suppressed = selected.iter().any(|prior| {
            prior.entry == candidate.entry
                && geometry::iou(prior.bounds, candidate.bounds) > options.nms_iou
        });
        if !suppressed {
            selected.push(candidate);
        }
        if selected.len() == options.max_detections {
            break;
        }
    }
    let detections = selected
        .iter()
        .map(|c| Detection {
            bounds: mapping.bounds(c.bounds),
            confidence: c.confidence,
            class_id: c.entry,
        })
        .collect();
    let hits = selected
        .iter()
        .map(|c| VocabHit {
            entry_index: c.entry,
            prompt_index: c.prompt,
        })
        .collect();
    Ok((detections, hits))
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
    let shape = &profile.outputs[0].shape;
    let anchors = match profile.postprocess.layout {
        Layout::Transposed => shape[shape.len() - 1],
        Layout::RowMajor => shape[shape.len() - 2],
    };
    let head = outputs[0];
    let rows = head.len() / anchors;
    let mut candidates = Vec::new();
    for anchor in 0..anchors {
        let at = |row| {
            head[match profile.postprocess.layout {
                Layout::Transposed => row * anchors + anchor,
                Layout::RowMajor => anchor * rows + row,
            }]
        };
        let mut best = (0usize, 0.0f32);
        for class in 0..classes {
            let score = at(4 + class);
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
        let coords = [at(0), at(1), at(2), at(3)];
        let xywh = match profile.postprocess.coord_form {
            CoordForm::Cxcywh => coords,
            CoordForm::Xyxy => [
                (coords[0] + coords[2]) / 2.0,
                (coords[1] + coords[3]) / 2.0,
                coords[2] - coords[0],
                coords[3] - coords[1],
            ],
        };
        if xywh.iter().any(|value| value.abs() > 1_000_000.0) {
            return Err(Error("vision.inferenceFailed|boxRangeInvalid".into()));
        }
        if xywh[2] <= 0.0 || xywh[3] <= 0.0 {
            continue;
        }
        let bounds = match profile.postprocess.coord_form {
            CoordForm::Xyxy => coords,
            CoordForm::Cxcywh => [
                xywh[0] - xywh[2] / 2.0,
                xywh[1] - xywh[3] / 2.0,
                xywh[0] + xywh[2] / 2.0,
                xywh[1] + xywh[3] / 2.0,
            ],
        };
        let angle = if task == Task::Obb {
            at(4 + classes)
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
