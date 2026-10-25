use super::{geometry, results::Candidate, *};
use serde::Deserialize;

#[derive(Deserialize)]
struct Pair {
    first: [f32; 5],
    second: [f32; 5],
    similarity: f32,
}
#[derive(Deserialize)]
struct FastNms {
    boxes: Vec<[f32; 5]>,
    scores: Vec<f32>,
    threshold: f32,
    keep: Vec<usize>,
}
#[derive(Deserialize)]
struct Mask {
    coefficients: [f32; 3],
    bounds: [f32; 4],
    width: u32,
    height: u32,
    scale: f32,
    left: u32,
    top: u32,
    bits: Vec<u8>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reference {
    pairs: Vec<Pair>,
    fast_nms: FastNms,
    mask: Mask,
}

fn reference() -> Reference {
    serde_json::from_str(include_str!(
        "../../resources/postprocess-reference-v1.json"
    ))
    .unwrap()
}
fn candidate(values: [f32; 5]) -> Candidate {
    Candidate {
        anchor: 0,
        xywh: [values[0], values[1], values[2], values[3]],
        bounds: [0.0; 4],
        confidence: 0.9,
        class_id: 0,
        angle: values[4],
    }
}

#[test]
fn independent_rotated_similarity_reference_matches_49_pairs() {
    let reference = reference();
    assert_eq!(reference.pairs.len(), 49);
    for pair in reference.pairs {
        let actual = geometry::probiou(&candidate(pair.first), &candidate(pair.second));
        assert!(
            (actual - pair.similarity).abs() < 0.0001,
            "{actual} != {} for {:?}, {:?}",
            pair.similarity,
            pair.first,
            pair.second
        );
    }
}

#[test]
fn independent_fast_nms_chain_rejects_greedy_substitution() {
    let reference = reference().fast_nms;
    let profile = find("obb-yolo11n-nms-v1").unwrap();
    let mut head = vec![0.0; 20 * 8400];
    for (i, values) in reference.boxes.iter().enumerate() {
        for channel in 0..4 {
            head[channel * 8400 + i] = values[channel];
        }
        head[4 * 8400 + i] = reference.scores[i];
        head[19 * 8400 + i] = values[4];
    }
    let views = [TensorView {
        name: "output0",
        shape: &profile.outputs[0].shape,
        values: &head,
    }];
    let map = ImageMapping {
        width: 640,
        height: 640,
        scale: 1.0,
        left: 0,
        top: 0,
    };
    let options = Options {
        nms_iou: reference.threshold,
        ..Options::default()
    };
    let result = decode(&profile.id, &views, map, options).unwrap();
    let Output::Obb(found) = result.output else {
        panic!("wrong task")
    };
    let scores: Vec<_> = reference
        .keep
        .iter()
        .map(|&i| reference.scores[i])
        .collect();
    assert_eq!(
        found.iter().map(|f| f.confidence).collect::<Vec<_>>(),
        scores
    );
}

#[test]
fn independent_bilinear_crop_unpad_nearest_mask_matches_every_bit() {
    let mask = reference().mask;
    let profile = find("segment-yolo11n-nms-v1").unwrap();
    let mut head = vec![0.0; 116 * 8400];
    let [x0, y0, x1, y1] = mask.bounds;
    for (channel, value) in [(x0 + x1) / 2.0, (y0 + y1) / 2.0, x1 - x0, y1 - y0]
        .into_iter()
        .enumerate()
    {
        head[channel * 8400] = value;
    }
    head[4 * 8400] = 0.9;
    for (i, coefficient) in mask.coefficients.into_iter().enumerate() {
        head[(84 + i) * 8400] = coefficient;
    }
    let mut prototypes = vec![0.0; 32 * 160 * 160];
    for y in 0..160 {
        for x in 0..160 {
            let i = y * 160 + x;
            prototypes[i] = (x as f32 / 11.0).sin();
            prototypes[160 * 160 + i] = (y as f32 / 13.0).cos();
            prototypes[2 * 160 * 160 + i] = (x + y) as f32 / 320.0;
        }
    }
    let views = [
        TensorView {
            name: "output0",
            shape: &profile.outputs[0].shape,
            values: &head,
        },
        TensorView {
            name: "output1",
            shape: &profile.outputs[1].shape,
            values: &prototypes,
        },
    ];
    let mapping = ImageMapping {
        width: mask.width,
        height: mask.height,
        scale: mask.scale,
        left: mask.left,
        top: mask.top,
    };
    let result = decode(&profile.id, &views, mapping, Options::default()).unwrap();
    let Output::Segment(found) = result.output else {
        panic!("wrong task")
    };
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].mask_bits, mask.bits);
}
