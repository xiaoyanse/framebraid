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
fn registry_is_five_explicit_profiles_not_shape_guessing() {
    assert_eq!(all().unwrap().len(), 5);
    for profile in all().unwrap() {
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
        Options::default()
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
    let result = decode(&p.id, &views, map, Options::default()).unwrap();
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
    assert!(decode(&p.id, &views, map, options)
        .unwrap_err()
        .0
        .contains("resourceExhausted"));
    assert!(decode(&p.id, &views[..1], map, Options::default()).is_err());
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
    assert!(decode(&p.id, &[wrong], mapping(), Options::default()).is_err());
    assert!(decode(
        &p.id,
        &[tensor(&p.outputs[0], &head[..3])],
        mapping(),
        Options::default()
    )
    .is_err());
    head[0] = f32::NAN;
    assert!(decode(
        &p.id,
        &[tensor(&p.outputs[0], &head)],
        mapping(),
        Options::default()
    )
    .is_err());
    assert!(decode(
        &p.id,
        &[],
        ImageMapping {
            width: 0,
            ..mapping()
        },
        Options::default()
    )
    .is_err());
}
