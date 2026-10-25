use super::*;
use std::path::{Path, PathBuf};

fn tensor(name: &str, dtype: u32, shape: &[i64], bytes: &[u8]) -> TensorData {
    TensorData::new(
        TensorInfo {
            name: name.into(),
            dtype,
            shape: shape.to_vec(),
        },
        bytes,
    )
    .unwrap()
}
fn items(count: usize) -> Vec<Vec<TensorData>> {
    (0..count)
        .map(|i| {
            let floats = [i as f32, i as f32 + 1.0, i as f32 + 2.0]
                .into_iter()
                .flat_map(f32::to_ne_bytes)
                .collect::<Vec<_>>();
            let ints = [i as i32, -7, 0, 300]
                .into_iter()
                .flat_map(i32::to_ne_bytes)
                .collect::<Vec<_>>();
            let mut values = vec![
                tensor("float", 0, &[1, 3], &floats),
                tensor("half", 1, &[1, 2], &[0, 0x3c, 0, 0xc0]),
                tensor("long", 2, &[1], &(i as i64 * 1000 + 7).to_ne_bytes()),
                tensor("int", 3, &[1, 2, 2], &ints),
                tensor("byte", 4, &[1, 3], &[i as u8, 0, 255]),
            ];
            if i.is_multiple_of(2) {
                values.reverse();
            }
            values
        })
        .collect()
}
fn run(model: &mut Model, data: &[Vec<TensorData>]) -> crate::Result<Vec<Vec<TensorData>>> {
    model.run_batch(
        &data.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        1024 * 1024,
        1024 * 1024,
    )
}
fn load(directory: &Path, name: &str) -> Model {
    Model::load(ModelSource::Path(&directory.join(name)), None, 0, true, 1)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"))
}
fn check(data: &[Vec<TensorData>], output: &[Vec<TensorData>]) {
    assert_eq!(data.len(), output.len());
    for (i, (input, row)) in data.iter().zip(output).enumerate() {
        assert_eq!(row.len(), 5);
        for value in input {
            let out = row
                .iter()
                .find(|t| t.info.name == format!("out_{}", value.info.name))
                .unwrap();
            assert_eq!(out.info.dtype, value.info.dtype);
            assert_eq!(out.info.shape, value.info.shape);
            if value.info.name == "float" {
                let expected = i as f32 - (data.len() - 1) as f32 / 2.0;
                assert!(out
                    .floats()
                    .unwrap()
                    .iter()
                    .all(|v| (*v - expected).abs() < 0.00001));
            } else {
                assert_eq!(out.bytes(), value.bytes());
            }
        }
    }
}

#[test]
#[ignore = "需显式ORT与本阶段生成样本，必须另行真实执行，不以跳过代验"]
fn actual_cpu_native_batch_and_budget_contract() {
    initialize(&PathBuf::from(std::env::var("AM_SDK_ORT_LIBRARY").unwrap())).unwrap();
    let directory = PathBuf::from(std::env::var("AM_SDK_BATCH_SAMPLE_DIR").unwrap());
    let mut dynamic = load(&directory, "dynamic.onnx");
    assert_eq!(dynamic.native_batch(), 1);
    for count in [1, 3, 64] {
        let data = items(count);
        let output = run(&mut dynamic, &data).unwrap();
        check(&data, &output);
    }
    let too_many = items(65);
    assert!(matches!(run(&mut dynamic, &too_many), Err(e) if e.0 == "vision.dynamicRangeExceeded"));
    let mut fixed = load(&directory, "fixed4.onnx");
    assert_eq!(fixed.native_batch(), 4);
    let data = items(4);
    let output = run(&mut fixed, &data).unwrap();
    check(&data, &output);
    let model_bytes = std::fs::read(directory.join("fixed4.onnx")).unwrap();
    let mut memory = Model::load(ModelSource::Memory(&model_bytes), None, 0, true, 1).unwrap();
    assert_eq!(memory.native_batch(), 4);
    check(&data, &run(&mut memory, &data).unwrap());
    assert!(
        matches!(run(&mut fixed, &data[..3]), Err(e) if e.0 == "vision.insufficientBatchRejected")
    );
    assert!(run(&mut fixed, &items(5)).is_err());
    assert!(
        matches!(dynamic.run_batch(&[&data[0]], 0, 1024), Err(e) if e.0 == "sdk.resourceExhausted")
    );
    assert!(
        matches!(dynamic.run_batch(&[&data[0]], 1024, 0), Err(e) if e.0 == "sdk.resourceExhausted")
    );
    // 单项五输出实际对齐占56字节，完整结果与拆分副本并存须112字节。
    assert!(
        matches!(dynamic.run_batch(&[&data[0]], 111, 112), Err(e) if e.0 == "sdk.resourceExhausted")
    );
    assert!(
        matches!(dynamic.run_batch(&[&data[0]], 112, 111), Err(e) if e.0 == "sdk.resourceExhausted")
    );
    assert!(dynamic.run_batch(&[&data[0]], 112, 112).is_ok());
    let mut unproved = load(&directory, "unproved.onnx");
    assert_eq!(unproved.native_batch(), 0);
    assert!(matches!(run(&mut unproved, &data), Err(e) if e.0 == "vision.batchUnsupported"));
    let grouped = super::batch::combine(
        &unproved.inputs,
        &too_many.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        1024 * 1024,
    );
    assert!(grouped.is_ok());
    let grouped = grouped.unwrap();
    assert!(unproved.run(&grouped, 1024 * 1024).is_ok());
    assert!(
        matches!(dynamic.run(&grouped, 1024 * 1024), Err(e) if e.0 == "vision.dynamicRangeExceeded")
    );
    assert!(Model::load(
        ModelSource::Path(&directory.join("wrong-axis.onnx")),
        None,
        0,
        true,
        1
    )
    .is_err());
    match Model::load(
        ModelSource::Path(&directory.join("bad-output.onnx")),
        None,
        0,
        true,
        1,
    ) {
        Ok(mut model) => {
            assert!(matches!(run(&mut model, &data), Err(e) if e.0 == "vision.batchUnsupported"))
        }
        Err(e) => assert_eq!(e.0, "vision.batchUnsupported"),
    }
    let mut runtime_bad = load(&directory, "runtime-bad-output.onnx");
    assert_eq!(runtime_bad.native_batch(), 1);
    assert!(matches!(run(&mut runtime_bad, &data), Err(e) if e.0 == "vision.batchUnsupported"));
    eprintln!("实际CPU原生batch通过：动态1/3/64、固定4、5dtype、跨batch计算、尾项/65/111-112字节预算/错误声明/实际输出首轴漂移拒绝；无证明首轴65仍可普通执行");
}
