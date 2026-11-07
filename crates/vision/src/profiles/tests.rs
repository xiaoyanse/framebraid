use super::*;

fn mapping() -> ImageMapping {
    ImageMapping {
        width: 1920,
        height: 1080,
        scale: 1.0 / 3.0,
        left: 0,
        top: 140,
    }
}

fn head_candidate(head: &mut [f32], anchor: usize, class: usize, score: f32, xywh: [f32; 4]) {
    for (channel, value) in xywh.into_iter().enumerate() {
        head[channel * 8400 + anchor] = value;
    }
    head[(4 + class) * 8400 + anchor] = score;
}

fn tensor<'a>(spec: &'a TensorSpec, values: &'a [f32]) -> TensorView<'a> {
    TensorView {
        name: &spec.name,
        shape: &spec.shape,
        values,
    }
}

#[test]
fn detect_head_four_layout_coordinate_combinations() {
    use super::registry::{CoordForm, Layout};
    for layout in [Layout::Transposed, Layout::RowMajor] {
        for coord in [CoordForm::Cxcywh, CoordForm::Xyxy] {
            let mut p = find("detect-yolo11n-nms-v1").unwrap().clone();
            p.classes = Some(4);
            p.postprocess.layout = layout;
            p.postprocess.coord_form = coord;
            p.outputs[0].shape = match layout {
                Layout::Transposed => vec![1, 8, 8],
                Layout::RowMajor => vec![1, 8, 8],
            };
            let mut head = vec![0.0; 64];
            let coords = match coord {
                CoordForm::Cxcywh => [320., 320., 200., 100.],
                CoordForm::Xyxy => [220., 270., 420., 370.],
            };
            for (row, value) in coords.into_iter().chain([0., 0., 0.9, 0.]).enumerate() {
                let index = match layout {
                    Layout::Transposed => row * 8,
                    Layout::RowMajor => row,
                };
                head[index] = value;
            }
            let m = ImageMapping {
                width: 640,
                height: 640,
                scale: 1.,
                left: 0,
                top: 0,
            };
            let Output::Detect(found) =
                results::decode_head(Task::Detect, &p, &[&head], m, Options::default()).unwrap()
            else {
                panic!("wrong output")
            };
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].class_id, 2);
            assert_eq!(found[0].confidence, 0.9);
            assert_eq!(
                found[0].bounds,
                [220. / 640., 270. / 640., 420. / 640., 370. / 640.]
            );
        }
    }
}

#[test]
fn registry_is_five_explicit_profiles_not_shape_guessing() {
    // 批 D 后 9 条：五类冻结样本 + world-head 2-D 亲和 + 固定 77 文本塔 +
    // world 批动态 3-D 亲和 + dynseq 文本塔（兼容新增，既有条目语义不动）。
    assert_eq!(all().unwrap().len(), 9);
    for profile in all()
        .unwrap()
        .iter()
        .filter(|p| p.sample_sha256 != "ignore")
    {
        assert_eq!(profile.sample_sha256.len(), 64);
        assert_eq!(profile.input.name, "images");
        assert_eq!(profile.input.shape.len(), 4);
        assert_eq!(profile.input.shape[..2], [1, 3]);
        assert_eq!(profile.input.shape[2], profile.input.shape[3]);
    }
    assert!(find("yolo-arbitrary-version").is_err());
}

#[test]
fn detect_nms_is_class_aware_and_maps_out_letterbox() {
    let p = find("detect-yolo11n-nms-v1").unwrap();
    let mut head = vec![0.0; 84 * 8400];
    head_candidate(&mut head, 0, 1, 0.9, [320.0, 320.0, 300.0, 180.0]);
    head_candidate(&mut head, 1, 1, 0.8, [320.0, 320.0, 300.0, 180.0]);
    head_candidate(&mut head, 2, 2, 0.7, [320.0, 320.0, 300.0, 180.0]);
    let result = decode(
        &p.id,
        &[tensor(&p.outputs[0], &head)],
        mapping(),
        Options::default(),
        None,
    )
    .unwrap();
    let Output::Detect(found) = result.output else {
        panic!("wrong task")
    };
    assert_eq!(found.len(), 2);
    assert_eq!((found[0].class_id, found[1].class_id), (1, 2));
    assert!((found[0].bounds[0] - 0.265625).abs() < 1e-6);
    assert!((found[0].bounds[1] - 0.25).abs() < 1e-6);
}

#[test]
fn classification_keeps_probabilities_and_accepts_cropped_non_square_image() {
    let p = find("classify-yolo11n-prob-v1").unwrap();
    let mut values = vec![0.0; 1000];
    values[2] = 0.7;
    values[3] = 0.3;
    let result = decode(
        &p.id,
        &[tensor(&p.outputs[0], &values)],
        mapping(),
        Options::default(),
        None,
    )
    .unwrap();
    let Output::Classify(found) = result.output else {
        panic!("wrong task")
    };
    assert_eq!(found.len(), 5);
    assert_eq!(found[0].probability, 0.7);
    assert_eq!(found[0].class_id, 2);
    values[3] = -1.0;
    assert!(decode(
        &p.id,
        &[tensor(&p.outputs[0], &values)],
        mapping(),
        Options::default(),
        None
    )
    .is_err());
}

#[test]
fn pose_keeps_existing_coco17_decoder() {
    let p = find("pose-yolo26-e2e-coco17-v1").unwrap();
    let mut values = vec![0.0; 300 * 57];
    values[..6].copy_from_slice(&[160.0, 230.0, 480.0, 410.0, 0.9, 0.0]);
    let result = decode(
        &p.id,
        &[tensor(&p.outputs[0], &values)],
        mapping(),
        Options::default(),
        None,
    )
    .unwrap();
    let Output::Pose(found) = result.output else {
        panic!("wrong task")
    };
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].keypoints.len(), 17);
    assert!((found[0].bounds[0] - 0.25).abs() < 1e-6);
}

#[test]
fn segment_uses_logit_zero_not_half_and_packs_lsb_padding() {
    let p = find("segment-yolo11n-nms-v1").unwrap();
    let mut head = vec![0.0; 116 * 8400];
    head_candidate(&mut head, 0, 0, 0.9, [6.5, 4.5, 13.0, 9.0]);
    head[84 * 8400] = 1.0;
    let mut prototypes = vec![0.0; 32 * 160 * 160];
    prototypes[..160 * 160].fill(0.2);
    let map = ImageMapping {
        width: 13,
        height: 9,
        scale: 1.0,
        left: 0,
        top: 0,
    };
    let views = [
        tensor(&p.outputs[0], &head),
        tensor(&p.outputs[1], &prototypes),
    ];
    let result = decode(&p.id, &views, map, Options::default(), None).unwrap();
    let Output::Segment(found) = result.output else {
        panic!("wrong task")
    };
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].mask_stride, 2);
    for row in found[0].mask_bits.chunks_exact(2) {
        assert_eq!(row, [0xff, 0x1f]);
    }
    let options = Options {
        mask_budget_bytes: 17,
        ..Options::default()
    };
    assert!(decode(&p.id, &views, map, options, None)
        .unwrap_err()
        .0
        .contains("resourceExhausted"));
    assert!(decode(&p.id, &views[..1], map, Options::default(), None).is_err());
}

#[test]
fn obb_vertices_follow_frozen_non_square_example_and_probiou_nms() {
    let p = find("obb-yolo11n-nms-v1").unwrap();
    let mut head = vec![0.0; 20 * 8400];
    for (anchor, score) in [(0, 0.9), (1, 0.8)] {
        head_candidate(&mut head, anchor, 0, score, [320.0, 240.0, 200.0, 100.0]);
        head[19 * 8400 + anchor] = std::f32::consts::PI / 6.0;
    }
    let map = ImageMapping {
        width: 640,
        height: 480,
        scale: 1.0,
        left: 0,
        top: 0,
    };
    let result = decode(
        &p.id,
        &[tensor(&p.outputs[0], &head)],
        map,
        Options::default(),
        None,
    )
    .unwrap();
    let Output::Obb(found) = result.output else {
        panic!("wrong task")
    };
    assert_eq!(found.len(), 1);
    assert!((found[0].vertices[0][0] * 640.0 - 381.60254).abs() < 0.001);
    assert!((found[0].vertices[0][1] * 480.0 - 333.30127).abs() < 0.001);
    let edge = [
        (found[0].vertices[1][0] - found[0].vertices[0][0]) * 640.0,
        (found[0].vertices[1][1] - found[0].vertices[0][1]) * 480.0,
    ];
    assert!((edge[0].hypot(edge[1]) - 200.0).abs() < 0.001);
}

#[test]
fn invalid_tensor_identity_shape_and_nonfinite_values_fail_before_decoding() {
    let p = find("detect-yolo11n-nms-v1").unwrap();
    let mut head = vec![0.0; 84 * 8400];
    let wrong = TensorView {
        name: "output-other",
        shape: &p.outputs[0].shape,
        values: &head,
    };
    assert!(decode(&p.id, &[wrong], mapping(), Options::default(), None).is_err());
    assert!(decode(
        &p.id,
        &[tensor(&p.outputs[0], &head[..3])],
        mapping(),
        Options::default(),
        None
    )
    .is_err());
    head[0] = f32::NAN;
    assert!(decode(
        &p.id,
        &[tensor(&p.outputs[0], &head)],
        mapping(),
        Options::default(),
        None
    )
    .is_err());
    assert!(decode(
        &p.id,
        &[],
        ImageMapping {
            width: 0,
            ..mapping()
        },
        Options::default(),
        None
    )
    .is_err());
}

#[test]
fn affinity_decode_merges_within_entry_and_keeps_hits() {
    use super::results::{decode_affinity, VocabLayout};
    let spec = TensorSpec {
        name: "output0".into(),
        shape: vec![7, 4],
    };
    let profile = super::Profile {
        id: "affinity-test".into(),
        task: Task::Detect,
        sample_sha256: String::new(),
        input: TensorSpec {
            name: "images".into(),
            shape: vec![1, 3, 640, 640],
        },
        inputs: vec![
            TensorSpec {
                name: "images".into(),
                shape: vec![1, 3, 640, 640],
            },
            TensorSpec {
                name: "text_embeddings".into(),
                shape: vec![0, 512],
            },
        ],
        outputs: vec![spec],
        classes: None,
        postprocess: Default::default(),
    };
    let layout = VocabLayout {
        entries: vec![("hand".into(), 2), ("glove".into(), 1)],
    };
    // transposed 布局：行 0..3 坐标，行 4..6 = 提示词分（hand p0/p1、glove p0）
    let mut head = vec![0.0f32; 7 * 4];
    let mut put = |anchor: usize, row: usize, v: f32| head[row * 4 + anchor] = v;
    // anchor 0：同一框，hand 两词命中（0.9/0.8）→ 组内合并留 0.9（prompt 0）
    for row in 0..4 {
        put(0, row, [320.0, 320.0, 300.0, 180.0][row]);
    }
    put(0, 4, 0.9);
    put(0, 5, 0.8);
    // anchor 1：与 anchor 0 近重合（IoU>0.7），hand 词 1（0.85）→ 跨 anchor 组内合并（留 0.9）
    for row in 0..4 {
        put(1, row, [321.0, 320.0, 300.0, 180.0][row]);
    }
    put(1, 5, 0.85);
    // anchor 2：与 anchor 0 同框但 glove 组（0.7）→ 组间独立保留
    for row in 0..4 {
        put(2, row, [320.0, 320.0, 300.0, 180.0][row]);
    }
    put(2, 6, 0.7);
    // anchor 3：低于阈值
    for row in 0..4 {
        put(3, row, [100.0, 100.0, 50.0, 50.0][row]);
    }
    put(3, 4, 0.1);
    let (detections, hits) = decode_affinity(
        &profile,
        &[&head],
        &[7, 4],
        mapping(),
        Options::default(),
        &layout,
    )
    .unwrap();
    assert_eq!(detections.len(), 2, "{detections:?}");
    assert_eq!(detections[0].class_id, 0, "hand 组最高分");
    assert_eq!(detections[0].confidence, 0.9);
    assert_eq!(detections[1].class_id, 1, "glove 组独立保留");
    assert_eq!(detections[1].confidence, 0.7);
    assert_eq!(hits.len(), 2);
    assert_eq!((hits[0].entry_index, hits[0].prompt_index), (0, 0));
    assert_eq!((hits[1].entry_index, hits[1].prompt_index), (1, 0));
}

#[test]
fn affinity_decode_rejects_short_head_and_bad_score() {
    use super::results::{decode_affinity, VocabLayout};
    let profile = super::Profile {
        id: "affinity-short".into(),
        task: Task::Detect,
        sample_sha256: String::new(),
        input: TensorSpec {
            name: "images".into(),
            shape: vec![1, 3, 640, 640],
        },
        inputs: vec![],
        outputs: vec![TensorSpec {
            name: "output0".into(),
            shape: vec![6, 4], // 4 坐标+2 词也放不下 3 行词表
        }],
        classes: None,
        postprocess: Default::default(),
    };
    let layout = VocabLayout {
        entries: vec![("a".into(), 1), ("b".into(), 1), ("c".into(), 1)],
    };
    let head = vec![0.0f32; 24];
    let error = decode_affinity(
        &profile,
        &[&head],
        &[6, 4],
        mapping(),
        Options::default(),
        &layout,
    )
    .err()
    .unwrap()
    .0;
    assert!(error.contains("affinityHead"), "{error}");
    // 分数越界（>1）拒绝
    let profile_ok = super::Profile {
        outputs: vec![TensorSpec {
            name: "output0".into(),
            shape: vec![7, 4],
        }],
        ..profile
    };
    let mut bad = vec![0.0f32; 28];
    for row in 0..4 {
        bad[row * 4] = [320.0, 320.0, 300.0, 180.0][row]; // anchor0 合法坐标
    }
    bad[4 * 4] = 1.5; // anchor0 prompt0 分 1.5
    let error = decode_affinity(
        &profile_ok,
        &[&bad],
        &[7, 4],
        mapping(),
        Options::default(),
        &layout,
    )
    .err()
    .unwrap()
    .0;
    assert!(error.contains("classProbabilityExpected"), "{error}");
}

/// 批 D 布局取证固化：真实 world.onnx 输出 [B, 4+提示词行, anchors] 与批拆分后
/// 的逐项 [1, rows, anchors] 扁平化后即亲和解码器索引的 [rows, anchors] 行主序
/// （批轴因子 1 不改变偏移）——同值张量按 2-D 与 3-D（带批轴）形状声明解码，
/// 结果必须逐项一致；anchors 取实际末维，档案末维动态（=0）不再参与除法。
#[test]
fn affinity_decode_batch_axis_shape_is_compatible_with_raw_affinity() {
    use super::results::{decode_affinity, VocabLayout};
    let profile = super::Profile {
        id: "affinity-batch".into(),
        task: Task::Detect,
        sample_sha256: String::new(),
        input: TensorSpec {
            name: "images".into(),
            shape: vec![0, 3, 640, 640],
        },
        inputs: vec![
            TensorSpec {
                name: "images".into(),
                shape: vec![0, 3, 640, 640],
            },
            TensorSpec {
                name: "txt_feats".into(),
                shape: vec![0, 0, 512],
            },
        ],
        outputs: vec![TensorSpec {
            name: "output0".into(),
            shape: vec![0, 0, 7], // batch/rows 动态，anchors=7（档案钉死格点数）
        }],
        classes: None,
        postprocess: Default::default(),
    };
    let layout = VocabLayout {
        entries: vec![("hand".into(), 1)],
    };
    // [rows=5, anchors=7] 已知值：anchor2 一框（合法坐标），prompt0 分 0.8。
    let mut head = vec![0.0f32; 5 * 7];
    for row in 0..4 {
        head[row * 7 + 2] = [320.0, 320.0, 300.0, 180.0][row];
    }
    head[4 * 7 + 2] = 0.8;
    let flat = decode_affinity(
        &profile,
        &[&head],
        &[5, 7],
        mapping(),
        Options::default(),
        &layout,
    )
    .unwrap();
    let batched = decode_affinity(
        &profile,
        &[&head],
        &[1, 5, 7],
        mapping(),
        Options::default(),
        &layout,
    )
    .unwrap();
    assert_eq!(flat.0.len(), 1, "{:?}", flat.0);
    assert_eq!((flat.0[0].confidence, flat.0[0].class_id), (0.8, 0));
    assert_eq!(
        batched.0.len(),
        flat.0.len(),
        "批轴形状与扁平形状解码结果一致"
    );
    for (a, b) in batched.0.iter().zip(&flat.0) {
        assert_eq!(a.class_id, b.class_id);
        assert_eq!(a.confidence, b.confidence);
        assert_eq!(a.bounds, b.bounds);
    }
    assert_eq!(batched.1.len(), flat.1.len());
    assert_eq!(
        (batched.1[0].entry_index, batched.1[0].prompt_index),
        (flat.1[0].entry_index, flat.1[0].prompt_index)
    );
    // 实际末维 anchors=0（异常形状）：显式拒绝，不除零。
    let error = decode_affinity(
        &profile,
        &[&head],
        &[5, 0],
        mapping(),
        Options::default(),
        &layout,
    )
    .err()
    .unwrap()
    .0;
    assert!(error.contains("affinityHead"), "{error}");
    // 实际元素数与形状积不一致：拒绝（不静默截断）。
    let error = decode_affinity(
        &profile,
        &[&head],
        &[5, 6],
        mapping(),
        Options::default(),
        &layout,
    )
    .err()
    .unwrap()
    .0;
    assert!(error.contains("affinityHead"), "{error}");
}
