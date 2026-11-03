//! 五类固定导出档案；纯后处理不加载ORT、不连接设备、不接触业务数据。
mod geometry;
mod mask;
mod registry;
mod results;

pub use registry::{all, find, release_for_sdk_unload, Profile, Task, TensorSpec};
pub use results::{
    ClassScore, Detection, Output, Prediction, RotatedDetection, Segment, TensorView, VocabHit,
    VocabLayout, VocabResult,
};

use crate::{preprocess::Mapping, Error, Result};

/// 模型输入像素到原图的映射。scale是等比缩放系数，padding为实际整数填充。
#[derive(Clone, Copy, Debug)]
pub struct ImageMapping {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub left: u32,
    pub top: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub confidence: f32,
    pub nms_iou: f32,
    pub max_detections: usize,
    pub top_k: usize,
    /// 掩膜结果单独限额；调用方总账还需在外层准入。
    pub mask_budget_bytes: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            confidence: 0.25,
            nms_iou: 0.7,
            max_detections: 300,
            top_k: 5,
            mask_budget_bytes: 64 * 1024 * 1024,
        }
    }
}

impl ImageMapping {
    fn validate(&self, size: u32, task: Task) -> Result<()> {
        if self.width == 0
            || self.height == 0
            || self.width > 8192
            || self.height > 8192
            || (task != Task::Classify
                && (!self.scale.is_finite()
                    || self.scale < 1.0 / 8192.0
                    || self.left >= size
                    || self.top >= size
                    || self.width as f32 * self.scale > (size - self.left) as f32 + 0.5
                    || self.height as f32 * self.scale > (size - self.top) as f32 + 0.5))
        {
            return Err(Error("vision.imageViewInvalid".into()));
        }
        Ok(())
    }
    fn point(&self, x: f32, y: f32) -> [f32; 2] {
        [
            (x - self.left as f32) / self.scale / self.width as f32,
            (y - self.top as f32) / self.scale / self.height as f32,
        ]
    }
    fn bounds(&self, bounds: [f32; 4]) -> [f32; 4] {
        let first = self.point(bounds[0], bounds[1]);
        let last = self.point(bounds[2], bounds[3]);
        [
            first[0].clamp(0.0, 1.0),
            first[1].clamp(0.0, 1.0),
            last[0].clamp(0.0, 1.0),
            last[1].clamp(0.0, 1.0),
        ]
    }
}

/// 只接受档案规定的命名f32输出，不根据形状猜任务，也不把缺失原型当检测框返回。
/// `vocab`（A2）：双输入 detect 档案（classes 未声明、亲和头形态）绑定词表后的
/// 布局——提供时走亲和解码（组内去重+命中明细）；其余路径为 None 零影响。
pub fn decode(
    profile_id: &str,
    tensors: &[TensorView<'_>],
    mapping: ImageMapping,
    options: Options,
    vocab: Option<&results::VocabLayout>,
) -> Result<Prediction> {
    let profile = find(profile_id)?;
    let affinity =
        profile.task == Task::Detect && profile.classes.is_none() && profile.inputs.len() > 1;
    if profile.task == Task::Detect && profile.classes.is_none() && !affinity {
        return Err(Error("vision.profileUnsupported".into()));
    }
    if affinity && vocab.is_none() {
        // 未绑定的亲和头档案无类别语义（画像 classes 未声明），解码通道前置拒绝；
        // 推理注入层已在更早处以 vocabNotBound 拦截，此处为纯后处理独立入口的兜底。
        return Err(Error("vision.profileUnsupported".into()));
    }
    if !options.confidence.is_finite()
        || !(0.0..=1.0).contains(&options.confidence)
        || !options.nms_iou.is_finite()
        || !(0.0..=1.0).contains(&options.nms_iou)
        || options.nms_iou == 0.0
        || options.max_detections == 0
        || options.max_detections > 300
        || options.top_k == 0
        || options.top_k > 1000
        || options.mask_budget_bytes > 64 * 1024 * 1024
    {
        return Err(Error("vision.confidenceRange".into()));
    }
    let size = profile.input.shape[2] as u32;
    mapping.validate(size, profile.task)?;
    if tensors.len() != profile.outputs.len() {
        return Err(Error("vision.outputShapeMismatch".into()));
    }
    let mut ordered = Vec::with_capacity(tensors.len());
    for expected in &profile.outputs {
        let matches: Vec<_> = tensors.iter().filter(|t| t.name == expected.name).collect();
        if matches.len() != 1 {
            return Err(Error("vision.outputShapeMismatch".into()));
        }
        let actual = matches[0];
        let count = expected
            .shape
            .iter()
            .try_fold(1usize, |count, &dim| count.checked_mul(dim));
        if actual.shape != expected.shape || count != Some(actual.values.len()) {
            return Err(Error("vision.outputShapeMismatch".into()));
        }
        if actual.values.iter().any(|v| !v.is_finite()) {
            return Err(Error("vision.inferenceFailed|nonFiniteOutput".into()));
        }
        ordered.push(actual.values);
    }
    let mut vocab_hits = None;
    let output = match profile.task {
        Task::Pose => {
            let map = Mapping {
                width: mapping.width,
                height: mapping.height,
                scale: mapping.scale,
                left: mapping.left,
                top: mapping.top,
            };
            let mut detections = crate::decode::decode(ordered[0], &map, options.confidence)?;
            detections.truncate(options.max_detections);
            Output::Pose(detections)
        }
        Task::Classify => Output::Classify(results::classify(ordered[0], options.top_k)?),
        Task::Detect if affinity && let Some(layout) = vocab => {
            let (detections, hits) =
                results::decode_affinity(profile, &ordered, mapping, options, layout)?;
            vocab_hits = Some(results::VocabResult {
                hits,
                labels: layout.entries.iter().map(|(l, _)| l.clone()).collect(),
            });
            Output::Detect(detections)
        }
        task => results::decode_head(task, profile, &ordered, mapping, options)?,
    };
    Ok(Prediction {
        profile_id: profile.id.clone(),
        width: mapping.width,
        height: mapping.height,
        output,
        vocab_hits,
    })
}

#[cfg(test)]
mod reference_tests;
#[cfg(test)]
mod tests;
