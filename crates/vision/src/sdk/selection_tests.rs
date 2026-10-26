use super::{selection::*, *};
use std::path::PathBuf;

#[test]
#[ignore = "独立测试进程和显式CPU ORT/Identity；验证原生执行不依赖结果交付"]
fn native_success_is_observed_when_output_budget_rejects_delivery() {
    initialize(&PathBuf::from(std::env::var("AM_SDK_ORT_LIBRARY").unwrap())).unwrap();
    assert_eq!(
        runtime::observations().unwrap().executed_mask,
        0,
        "须以精确测试名在独立进程运行，不能复用其他测试的执行观察"
    );
    let path = PathBuf::from(std::env::var("AM_SDK_RAW_MODEL").unwrap());
    let cpu = BackendTarget {
        backend: 0,
        device_id: 0,
    };
    let policy = BackendSelectionPolicy {
        priority: vec![cpu],
        allow_fallback: false,
        budget_ms: 5000,
    };
    let mut model = Model::load_selected(ModelSource::Path(&path), None, &policy, 1).unwrap();
    assert_eq!(model.inputs.len(), 1);
    assert_eq!(model.inputs[0].name, "images");
    assert_eq!(model.inputs[0].dtype, 0);
    assert_eq!(model.inputs[0].shape, [1, 3, 2, 2]);
    let bytes: Vec<u8> = (0..12)
        .flat_map(|i| (i as f32 / 4.0).to_ne_bytes())
        .collect();
    let input = TensorData::new(model.inputs[0].clone(), &bytes).unwrap();
    assert_eq!(runtime::observations().unwrap().executed_mask, 0);
    // 公开兼容字段不是实际选择的可信来源。
    model.actual_backend = 2;
    assert_eq!(
        model.run(std::slice::from_ref(&input), 0).err().unwrap().0,
        "sdk.resourceExhausted"
    );
    for _ in 0..3 {
        let report = model.selection_report();
        assert_eq!(report.selected, Some(cpu));
        assert_eq!(report.native_runs, 1);
        assert_eq!(report.delivered_runs, 0);
        assert_eq!(report.native_failures, 0);
        assert_eq!(report.last_native_run_succeeded, Some(true));
        assert_eq!(report.attempts.len(), 1);
        assert_eq!(runtime::observations().unwrap().executed_mask, 1);
    }
    println!(
        "native-success-delivery-rejected {}",
        serde_json::to_string(model.selection_report()).unwrap()
    );
    // 下一次仅来自调用方显式调用，不自动重放失败交付的请求。
    let output = model.run(std::slice::from_ref(&input), 1024).unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].bytes(), bytes);
    assert_eq!(model.selection_report().native_runs, 2);
    assert_eq!(model.selection_report().delivered_runs, 1);
    assert_eq!(runtime::observations().unwrap().executed_mask, 1);
}

#[test]
#[ignore = "显式CPU ORT和越界Gather合成模型；真实Run错误不自动重放"]
fn native_run_failure_is_reported_once_without_automatic_replay_or_model_switch() {
    initialize(&PathBuf::from(std::env::var("AM_SDK_ORT_LIBRARY").unwrap())).unwrap();
    let path = PathBuf::from(std::env::var("AM_SDK_BAD_RUN_MODEL").unwrap());
    let cpu = BackendTarget {
        backend: 0,
        device_id: 0,
    };
    let policy = BackendSelectionPolicy {
        priority: vec![cpu],
        allow_fallback: false,
        budget_ms: 5000,
    };
    let mut model = Model::load_selected(ModelSource::Path(&path), None, &policy, 1).unwrap();
    let input = TensorData::new(
        TensorInfo {
            name: "x".into(),
            dtype: 0,
            shape: vec![1],
        },
        &[0; 4],
    )
    .unwrap();
    assert_eq!(
        model.run(&[input], 1024).err().unwrap().0,
        "vision.inferenceFailed"
    );
    let report = model.selection_report();
    assert_eq!(report.native_failures, 1);
    assert_eq!(report.native_runs, 0);
    assert_eq!(report.delivered_runs, 0);
    assert_eq!(report.selected, Some(cpu));
    assert_eq!(report.attempts.len(), 1);
    assert_eq!(report.last_native_run_succeeded, Some(false));
    assert!(report.last_native_error_code.is_some());
    println!(
        "selected-native-failure {}",
        serde_json::to_string(report).unwrap()
    );
}

#[test]
#[ignore = "显式CPU ORT和隔离ONNX模型；不冒充加速EP验收"]
fn selected_models_prove_cpu_fallback_strict_and_execution_not_public_fields() {
    initialize(&PathBuf::from(std::env::var("AM_SDK_ORT_LIBRARY").unwrap())).unwrap();
    let path = PathBuf::from(std::env::var("AM_SDK_RAW_MODEL").unwrap());
    let cuda = BackendTarget {
        backend: 2,
        device_id: 0,
    };
    let cpu = BackendTarget {
        backend: 0,
        device_id: 0,
    };
    let mut policy = BackendSelectionPolicy {
        priority: vec![cuda, cpu],
        allow_fallback: true,
        budget_ms: 5000,
    };
    let mut model = Model::load_selected(ModelSource::Path(&path), None, &policy, 1).unwrap();
    assert_eq!(model.selection_report().selected, Some(cpu));
    assert_eq!(model.selection_report().attempts.len(), 2);
    assert_eq!(
        model.selection_report().attempts[0].stage,
        "compiled_provider",
        "本用例必须加载CPU-only运行库，不能拿已配CUDA环境冒充缺EP测试"
    );
    assert!(model.selection_report().model_accepted);
    assert_eq!(model.selection_report().native_runs, 0);
    assert!(model.fallback);
    // 旧公开兼容字段可修改，但不能借此将CPU真实执行登记成CUDA。
    model.actual_backend = 2;
    let input = TensorData::new(
        TensorInfo {
            name: "images".into(),
            dtype: 0,
            shape: vec![1, 3, 2, 2],
        },
        &[0; 48],
    )
    .unwrap();
    assert_eq!(
        model.run(std::slice::from_ref(&input), 1024).unwrap().len(),
        1
    );
    let report = model.selection_report();
    assert_eq!(report.selected, Some(cpu));
    assert_eq!(report.native_runs, 1);
    assert_eq!(report.delivered_runs, 1);
    assert_eq!(report.last_native_run_succeeded, Some(true));
    assert!(!report.placement_verified);
    let matrix = runtime::observations().unwrap();
    assert!(
        matrix.executed_mask & (1 << 2) == 0,
        "公开字段不能污染全局CUDA执行证据"
    );
    assert!(model.run(std::slice::from_ref(&input), 0).is_err());
    assert_eq!(model.selection_report().native_runs, 2);
    assert_eq!(model.selection_report().delivered_runs, 1);
    policy.allow_fallback = false;
    let failure = Model::load_selected(ModelSource::Path(&path), None, &policy, 1)
        .err()
        .unwrap();
    assert_eq!(failure.error.0, "vision.backendStrictUnavailable");
    assert_eq!(failure.report.attempts.len(), 1);
    policy.allow_fallback = true;
    policy.priority = vec![cuda];
    assert!(
        Model::load_selected(ModelSource::Path(&path), None, &policy, 1).is_err(),
        "未允许CPU不得添加CPU"
    );
    policy.priority = vec![cpu];
    let invalid = Model::load_selected(ModelSource::Memory(b"invalid ONNX"), None, &policy, 1)
        .err()
        .unwrap();
    assert_eq!(invalid.error.0, "vision.modelLoadFailed");
    assert!(!invalid.report.model_accepted);
    println!(
        "selected-backend-real {}",
        serde_json::to_string(model.selection_report()).unwrap()
    );
}

#[test]
#[ignore = "显式CPU ORT及隔离MatMul；当前候选两加载方式三轮，不是GPU性能测试"]
fn selection_single_cpu_preserves_bytes_and_quantifies_load_and_run_overhead() {
    use std::time::Instant;
    initialize(&PathBuf::from(std::env::var("AM_SDK_ORT_LIBRARY").unwrap())).unwrap();
    let path = PathBuf::from(std::env::var("AM_SDK_MATMUL_MODEL").unwrap());
    let policy = BackendSelectionPolicy {
        priority: vec![BackendTarget {
            backend: 0,
            device_id: 0,
        }],
        allow_fallback: false,
        budget_ms: 5000,
    };
    for round in 0..3 {
        let order = if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        };
        let mut expected = None;
        for selected in order {
            let start = Instant::now();
            let mut model = if selected {
                Model::load_selected(ModelSource::Path(&path), None, &policy, 1).unwrap()
            } else {
                Model::load(ModelSource::Path(&path), None, 0, true, 1).unwrap()
            };
            let load_us = start.elapsed().as_micros();
            let spec = &model.inputs[0];
            assert_eq!(spec.dtype, 0);
            let bytes: Vec<u8> = (0..byte_count(spec.dtype, &spec.shape).unwrap() / 4)
                .flat_map(|i| (((i % 31) as f32 - 15.0) / 16.0).to_ne_bytes())
                .collect();
            let input = TensorData::new(spec.clone(), &bytes).unwrap();
            let mut times = Vec::new();
            for n in -20..120 {
                let start = Instant::now();
                let output = model
                    .run(std::slice::from_ref(&input), 16 * 1024 * 1024)
                    .unwrap();
                let elapsed = start.elapsed().as_micros();
                if let Some(expected) = &expected {
                    assert_eq!(output[0].bytes(), expected);
                } else {
                    expected = Some(output[0].bytes().to_vec());
                }
                if n >= 0 {
                    times.push(elapsed);
                }
            }
            times.sort_unstable();
            println!("selection-quant {{\"round\":{round},\"selected_entry\":{selected},\"load_us\":{load_us},\"median_us\":{},\"p95_us\":{},\"total_us\":{},\"frames\":120,\"warmup\":20}}",
                times[times.len()/2],times[(times.len()*95).div_ceil(100)-1],times.iter().sum::<u128>());
        }
    }
}
