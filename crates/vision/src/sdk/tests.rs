use super::*;
use crate::profiles::{self, ImageMapping, Options, TensorView};
use std::path::PathBuf;

#[test]
#[ignore = "需要显式ORT与五类冻结模型样本，不以缺少环境跳过后冒充实测"]
fn five_profiles_execute_real_cpu_and_memory_model() {
    initialize(&PathBuf::from(std::env::var("AM_SDK_ORT_LIBRARY").unwrap())).unwrap();
    let directory = PathBuf::from(std::env::var("AM_SDK_MODEL_DIR").unwrap());
    let files = [
        "yolo26n-pose.onnx",
        "yolo11n.onnx",
        "yolo11n-cls.onnx",
        "yolo11n-seg.onnx",
        "yolo11n-obb.onnx",
    ];
    let bytes = vec![50u8; 320 * 180 * 3];
    let image = pixels::Pixels {
        width: 320,
        height: 180,
        format: 1,
        planes: vec![&bytes],
        strides: vec![320 * 3],
        color_matrix: 0,
        color_range: 0,
    };
    for (profile, file) in profiles::all().unwrap().iter().zip(files) {
        let mut model = Model::load(
            ModelSource::Path(&directory.join(file)),
            Some(&profile.id),
            0,
            true,
            1,
        )
        .unwrap();
        assert_eq!(model.actual_backend, 0);
        let (input, m) = prepare(
            &image,
            "images",
            Preprocess::profile(
                profile.input.shape[2] as u32,
                profile.task == profiles::Task::Classify,
            ),
            64 * 1024 * 1024,
        )
        .unwrap();
        let result = model
            .run(std::slice::from_ref(&input), 64 * 1024 * 1024)
            .unwrap();
        let shapes = result
            .iter()
            .map(|r| r.info.shape.iter().map(|d| *d as usize).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let views = result
            .iter()
            .zip(&shapes)
            .map(|(r, s)| TensorView {
                name: &r.info.name,
                shape: s,
                values: r.floats().unwrap(),
            })
            .collect::<Vec<_>>();
        let prediction = profiles::decode(
            &profile.id,
            &views,
            ImageMapping {
                width: m.width,
                height: m.height,
                scale: m.scale_x,
                left: m.left,
                top: m.top,
            },
            Options::default(),
        )
        .unwrap();
        assert_eq!(prediction.width, 320);
        let mut memory = Model::load(
            ModelSource::Memory(&std::fs::read(directory.join(file)).unwrap()),
            Some(&profile.id),
            0,
            true,
            1,
        )
        .unwrap();
        let repeated = memory.run(&[input], 64 * 1024 * 1024).unwrap();
        assert_eq!(repeated.len(), result.len());
        for (a, b) in result.iter().zip(&repeated) {
            assert_eq!(a.bytes(), b.bytes());
        }
        eprintln!(
            "真实CPU档案 {} 完成，原始出口 {}，路径/内存逐字节一致",
            profile.id,
            result.len()
        );
    }
}
