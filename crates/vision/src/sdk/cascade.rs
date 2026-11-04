//! ROI 级联推理（C1，合同 §4.1）纯逻辑：ROI 生成（类过滤/外扩/clamp/退化丢弃/
//! 置信度截断）与 ROI→原图坐标回映；全归一化平面运算，不触引擎与像素。

/// 静态+动态生成 ROI 总数上限（超限按一级置信度降序截断）。
pub const MAX_ROIS: usize = 16;
/// 动态规则数上限。
pub const MAX_RULES: usize = 16;
/// 静态 ROI 数上限。
pub const MAX_STATIC: usize = 16;
/// ROI 归一化最小边长默认值（规则 min_roi_size=0 时生效）。
pub const MIN_ROI_DEFAULT: f32 = 0.02;

/// 动态 ROI 规则（FFI 侧解析后的安全形态；min_roi_size=0 在生成时取默认）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rule {
    /// 一级命中此类别才生成 ROI；-1=全部一级框。
    pub source_class_id: i32,
    /// 框外扩比例（框宽高各向外 expand/2）。
    pub expand: f32,
    /// ROI 归一化最小边长，任一边小于则丢弃。
    pub min_roi_size: f32,
}

/// 一级检出框参与 ROI 生成的最小视图（归一化 xyxy）。
#[derive(Clone, Copy, Debug)]
pub struct PrimaryBox {
    pub bounds: [f32; 4],
    pub confidence: f32,
    pub class_id: u32,
}

fn normalize(rect: [f32; 4]) -> Option<[f32; 4]> {
    let rect = [
        rect[0].clamp(0.0, 1.0),
        rect[1].clamp(0.0, 1.0),
        rect[2].clamp(0.0, 1.0),
        rect[3].clamp(0.0, 1.0),
    ];
    (rect[2] > rect[0] && rect[3] > rect[1]).then_some(rect)
}

/// 生成级联 ROI：静态（clamp 图内，退化丢弃）与动态（类过滤→外扩→clamp→
/// min_roi_size 丢弃）合并；动态部分按一级置信度降序参与，总数超 MAX_ROIS
/// 时截断——静态 ROI 无一级来源、为调用方显式指定，优先保留不受截断。
/// 返回 (ROI 归一化 xyxy, primary_index)；静态 ROI 的 index=-1。
pub fn generate_rois(
    primary: &[PrimaryBox],
    rules: &[Rule],
    static_rois: &[[f32; 4]],
) -> Vec<([f32; 4], i32)> {
    let mut dynamic: Vec<(f32, [f32; 4], i32)> = Vec::new();
    for rule in rules {
        let min_size = if rule.min_roi_size == 0.0 {
            MIN_ROI_DEFAULT
        } else {
            rule.min_roi_size
        };
        for (index, b) in primary.iter().enumerate() {
            if rule.source_class_id != -1 && b.class_id as i32 != rule.source_class_id {
                continue;
            }
            let (w, h) = (b.bounds[2] - b.bounds[0], b.bounds[3] - b.bounds[1]);
            let expanded = [
                b.bounds[0] - w * rule.expand / 2.0,
                b.bounds[1] - h * rule.expand / 2.0,
                b.bounds[2] + w * rule.expand / 2.0,
                b.bounds[3] + h * rule.expand / 2.0,
            ];
            let Some(roi) = normalize(expanded) else {
                continue;
            };
            if roi[2] - roi[0] < min_size || roi[3] - roi[1] < min_size {
                continue;
            }
            dynamic.push((b.confidence, roi, index as i32));
        }
    }
    // 稳定排序：置信度相同时保持（规则序,框序），截断结果确定。
    dynamic.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut result: Vec<([f32; 4], i32)> = static_rois
        .iter()
        .filter_map(|roi| normalize(*roi).map(|roi| (roi, -1)))
        .collect();
    for (_, roi, index) in dynamic {
        if result.len() >= MAX_ROIS {
            break;
        }
        result.push((roi, index));
    }
    result
}

/// ROI 平面归一化框回映原图归一化平面（ROI 偏移+缩放，合同"缩放逆变换+ROI 偏移"）。
pub fn map_box_to_original(box_in_roi: [f32; 4], roi: [f32; 4]) -> [f32; 4] {
    let (w, h) = (roi[2] - roi[0], roi[3] - roi[1]);
    [
        roi[0] + box_in_roi[0] * w,
        roi[1] + box_in_roi[1] * h,
        roi[0] + box_in_roi[2] * w,
        roi[1] + box_in_roi[3] * h,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(class: i32, expand: f32, min: f32) -> Rule {
        Rule {
            source_class_id: class,
            expand,
            min_roi_size: min,
        }
    }

    fn box_at(x0: f32, y0: f32, x1: f32, y1: f32, class: u32, conf: f32) -> PrimaryBox {
        PrimaryBox {
            bounds: [x0, y0, x1, y1],
            confidence: conf,
            class_id: class,
        }
    }

    fn near(a: [f32; 4], b: [f32; 4]) {
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 1e-5, "{a:?} !~ {b:?}");
        }
    }

    #[test]
    fn dynamic_rule_generates_expanded_clamped_rois() {
        let primary = [box_at(0.4, 0.4, 0.6, 0.6, 3, 0.9)];
        let rois = generate_rois(&primary, &[rule(-1, 0.5, 0.0)], &[]);
        // 外扩 0.5：每侧 w*expand/2=0.05 → [0.35,0.35,0.65,0.65]
        assert_eq!(rois.len(), 1);
        assert_eq!(rois[0].1, 0);
        near(rois[0].0, [0.35, 0.35, 0.65, 0.65]);
        // clamp 图内：框贴边外扩不越 [0,1]
        let primary = [box_at(0.0, 0.0, 0.2, 0.2, 0, 0.5)];
        let rois = generate_rois(&primary, &[rule(-1, 1.0, 0.0)], &[]);
        assert_eq!(rois.len(), 1);
        near(rois[0].0, [0.0, 0.0, 0.3, 0.3]);
    }

    #[test]
    fn class_filter_selects_only_matching_boxes() {
        let primary = [
            box_at(0.1, 0.1, 0.3, 0.3, 0, 0.9),
            box_at(0.5, 0.5, 0.7, 0.7, 5, 0.8),
        ];
        let rois = generate_rois(&primary, &[rule(5, 0.0, 0.0)], &[]);
        assert_eq!(rois.len(), 1);
        assert_eq!(rois[0].1, 1, "only class-5 box generates ROI");
        // -1=全部
        let rois = generate_rois(&primary, &[rule(-1, 0.0, 0.0)], &[]);
        assert_eq!(rois.len(), 2);
    }

    #[test]
    fn min_roi_size_drops_degenerate_rois_and_defaults() {
        let primary = [box_at(0.45, 0.45, 0.55, 0.55, 0, 0.9)];
        // 显式 min=0.2：外扩后 ROI 0.12 边长 < 0.2 → 丢弃
        assert!(generate_rois(&primary, &[rule(-1, 0.2, 0.2)], &[]).is_empty());
        // min=0：默认 0.02，0.12 边长保留
        let rois = generate_rois(&primary, &[rule(-1, 0.2, 0.0)], &[]);
        assert_eq!(rois.len(), 1);
        // 极小框：外扩后仍小于默认 0.02 → 丢弃
        let tiny = [box_at(0.499, 0.499, 0.501, 0.501, 0, 0.9)];
        assert!(generate_rois(&tiny, &[rule(-1, 0.0, 0.0)], &[]).is_empty());
        // 负宽高框（异常一级数据）经 clamp 退化 → 丢弃不 panic
        let bad = [box_at(0.6, 0.6, 0.4, 0.4, 0, 0.9)];
        assert!(generate_rois(&bad, &[rule(-1, 0.0, 0.0)], &[]).is_empty());
    }

    #[test]
    fn truncation_keeps_static_first_and_dynamic_by_confidence() {
        let primary: Vec<PrimaryBox> = (0..20)
            .map(|i| {
                box_at(
                    0.05 * i as f32,
                    0.05,
                    0.05 * i as f32 + 0.04,
                    0.09,
                    0,
                    i as f32 / 40.0,
                )
            })
            .collect();
        let rois = generate_rois(&primary, &[rule(-1, 0.0, 0.0)], &[[0.0, 0.0, 0.1, 0.1]]);
        assert_eq!(rois.len(), MAX_ROIS);
        assert_eq!(rois[0], ([0.0, 0.0, 0.1, 0.1], -1), "static first");
        // 动态按置信度降序：最高分框 index=19
        assert_eq!(rois[1].1, 19);
        assert_eq!(rois[15].1, 5, "lowest-confidence dynamic truncated");
    }

    #[test]
    fn static_rois_clamp_and_drop_degenerate() {
        // 越界静态 ROI clamp 图内；退化（x1<=x0）丢弃
        let rois = generate_rois(&[], &[], &[[0.9, 0.9, 1.2, 1.2], [0.3, 0.3, 0.2, 0.4]]);
        assert_eq!(rois, vec![([0.9, 0.9, 1.0, 1.0], -1)]);
    }

    #[test]
    fn map_box_math_normal_and_edges() {
        let roi = [0.2, 0.3, 0.6, 0.7];
        let mapped = map_box_to_original([0.0, 0.0, 1.0, 1.0], roi);
        assert_eq!(mapped, roi, "full-ROI box maps to ROI itself");
        let mapped = map_box_to_original([0.5, 0.5, 1.0, 1.0], roi);
        near(mapped, [0.4, 0.5, 0.6, 0.7]);
        // 原点 ROI：无偏移直通
        let mapped = map_box_to_original([0.25, 0.75, 0.5, 1.0], [0.0, 0.0, 0.5, 0.25]);
        near(mapped, [0.125, 0.1875, 0.25, 0.25]);
        // 全图 ROI：恒等
        let mapped = map_box_to_original([0.1, 0.2, 0.3, 0.4], [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(mapped, [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn multiple_rules_can_emit_multiple_rois_per_box() {
        let primary = [box_at(0.4, 0.4, 0.6, 0.6, 0, 0.9)];
        let rois = generate_rois(&primary, &[rule(-1, 0.0, 0.0), rule(0, 1.0, 0.0)], &[]);
        assert_eq!(rois.len(), 2);
        // 同框两条规则：expand=0 在前（插入序，置信度相同稳定排序）
        near(rois[0].0, [0.4, 0.4, 0.6, 0.6]);
        near(rois[1].0, [0.3, 0.3, 0.7, 0.7]);
        assert_eq!(rois[0].1, 0);
        assert_eq!(rois[1].1, 0);
    }
}
