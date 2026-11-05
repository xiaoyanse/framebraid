//! Rust 直依赖调用示例（C1 级联 + §4.1b 跟踪）：下游 am-server workspace 以
//! crate 依赖消费 SDK 时的参考形态（非 FFI 绑定路径）。
//!
//! 运行：`cargo run --example vision_cascade_tracker -- <primary.onnx> <secondary.onnx>`
//! 无参数时只演示跟踪器（纯逻辑，无模型依赖）。
use am_vision::sdk::{
    cascade::{self, PrimaryBox},
    tracker::{Tracker, TrackerParams},
};

/// 跟踪器：逐帧喂检出框（归一化 xyxy/置信度/类别），拿稳定 track_id。
fn tracker_demo() {
    let params = TrackerParams {
        confirm_hits: 1, // 规则引擎自带时序确认时置 1：建档帧当帧即输出
        ..TrackerParams::default()
    };
    let mut tracker = Tracker::new(params);
    // 帧 1：两框建档（confirm_hits=1 即输出）
    let frame1 = [
        ([0.40, 0.40, 0.60, 0.60], 0.9f32, 3u32),
        ([0.10, 0.10, 0.30, 0.30], 0.8, 5),
    ];
    let out = tracker.update(&frame1);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].track_id, 1);
    // 帧 2：框 1 平移 5%（恒速预测+IoU 匹配保住 ID）
    let frame2 = [([0.42, 0.40, 0.62, 0.60], 0.9, 3)];
    let out = tracker.update(&frame2);
    assert_eq!(out[0].track_id, 1, "same target keeps id");
    // 帧 3：目标消失——当帧无输出（lost 未回收不输出）；TTL 内重现保 ID
    assert!(tracker.update(&[]).is_empty());
    let out = tracker.update(&[([0.44, 0.40, 0.64, 0.60], 0.9, 3)]);
    assert_eq!(out[0].track_id, 1, "re-acquired within ttl");
}

/// C1 级联：全图一级检测 → ROI 生成 → 逐 ROI 裁剪放大 → 二级检测 → 坐标回映。
/// 坐标回映示例（纯逻辑部分，模型两阶段见下方编排骨架）。
fn cascade_rois_demo() {
    let primary = [PrimaryBox {
        bounds: [0.40, 0.40, 0.60, 0.60],
        confidence: 0.9,
        class_id: 3,
    }];
    // 规则：class 3 的框外扩 50%（实证 0.15 起步，示例放大便于观察）
    let rules = [cascade::Rule {
        source_class_id: 3,
        expand: 0.5,
        min_roi_size: 0.0, // 0=默认 0.02
    }];
    let rois = cascade::generate_rois(&primary, &rules, &[]);
    assert_eq!(rois.len(), 1);
    let (roi, primary_index) = rois[0];
    assert_eq!(primary_index, 0, "dynamic roi traces its primary box");
    // 二级框在 ROI 平面 [0.5,0.5,0.8,0.8] → 回映原图
    let mapped = cascade::map_box_to_original([0.5, 0.5, 0.8, 0.8], roi);
    println!("roi={roi:?} mapped={mapped:?}");
}

/// 两阶段编排骨架（模型路径形态；检测解码走 profiles 模块，与 FFI 层同源）。
///
/// ```ignore
/// use am_vision::sdk::{Model, ModelSource, Preprocess, pixels::Pixels};
/// let primary = Model::load(ModelSource::Path(primary_path), Some(profile_id), backend, false, 2)?;
/// let secondary = Model::load(ModelSource::Path(secondary_path), Some(profile2), backend, false, 2)?;
/// // 一级：图像 prepare 到一级输入尺寸 → run_ref → profiles::decode 得框
/// let (tensor, mapping) = sdk::prepare(&pixels, name, primary_preprocess, budget)?;
/// let raw = primary.run_ref(&[&tensor], budget)?;
/// let prediction = profiles::decode(profile_id, &views, mapping.into(), options, None)?;
/// // ROI 生成（cascade::generate_rois）→ 逐 ROI 裁剪子图 Pixels → 二级同款
/// // prepare/run_ref/decode → map_box_to_original 回映原图归一化平面。
/// ```
fn main() {
    tracker_demo();
    cascade_rois_demo();
    println!("vision cascade+tracker demo ok");
}
