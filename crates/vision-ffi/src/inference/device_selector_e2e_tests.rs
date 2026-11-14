//! 设备定向 e2e 真机门（V8.5，§3.5b）：DML 进程专跑——AM_SDK_ORT_DIR 指向 DML
//! 构建运行库（.local/runtimes/ort-dml-124），不与 CUDA 线 ORT 同进程混载（互斥
//! 实证即本门探针臂）。覆盖：定向 Arc 命中（device_label 真值+推理身份）、对照臂
//! RTX 命中、零命中同步拒绝、DML/CUDA 同进程互斥探针、多路浸泡（FB_V85_SOAK_MS，
//! 默认 60s 冒烟/验收 600s）与跨后端基准臂（FB_V85_BENCH_*，供核显/独显/CPU 口径
//! 分列；各后端独立进程跑，运行库互斥由部署形态保证）。
use super::exports::*;
use super::selected_exports::amv_model_load_selected;
use super::selection::{amv_get_model_selection_report, amv_get_selection_attempt_report};
use crate::domain;
use am_sdk_support::{abi::*, context};
use am_vision_core::sdk::device::{dxgi_adapters, select_by_substring};
use std::{
    ffi::{c_char, CString},
    path::PathBuf,
    ptr,
    time::{Duration, Instant},
};

const COLORS: [(&str, [f32; 3]); 4] = [
    ("red", [1.0, 0.0, 0.0]),
    ("green", [0.0, 1.0, 0.0]),
    ("blue", [0.0, 0.0, 1.0]),
    ("yellow", [1.0, 1.0, 0.0]),
];

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("缺少环境变量 {name}")))
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var_os(name)
        .map(|v| v.to_string_lossy().parse().expect("数值环境变量"))
        .unwrap_or(default)
}

fn init_ctx(dirs: &[String], threads: u32) -> usize {
    let names: Vec<CString> = dirs
        .iter()
        .map(|d| CString::new(d.as_str()).unwrap())
        .collect();
    let pointers: Vec<*const c_char> = names.iter().map(|d| d.as_ptr()).collect();
    let params = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: pointers.as_ptr(),
        runtime_dir_count: pointers.len() as u32,
        infer_threads: threads,
        ..Default::default()
    };
    unsafe { context::initialize_vision(domain(), &params) }.expect("vision ctx")
}

struct CtxGuard {
    ctx: usize,
    models: Vec<usize>,
}

impl CtxGuard {
    fn track(&mut self, key: usize) {
        self.models.push(key);
    }
}

impl Drop for CtxGuard {
    fn drop(&mut self) {
        for key in self.models.iter().rev() {
            let _ = domain().close(*key, 5000);
            let _ = domain().destroy(*key);
        }
        let _ = domain().close(self.ctx, 5000);
        let _ = domain().destroy(self.ctx);
    }
}

struct Selected {
    rc: i32,
    model: usize,
    id: u64,
}

/// 经 amv_model_load_selected 加载（strict：allow_fallback=0，定向不静默回退）。
fn load_selected(ctx: usize, path: &CString, backend: u32, needle: Option<&CString>) -> Selected {
    let candidates = [am_backend_candidate_t {
        backend,
        device_id: 0,
    }];
    let selector = needle.map(|n| am_device_selector_t {
        struct_size: std::mem::size_of::<am_device_selector_t>() as u32,
        match_utf8: n.as_ptr(),
        // match_bytes 含 NUL 终结（§3.5b）。
        match_bytes: n.as_bytes_with_nul().len() as u32,
        instance: 0,
        reserved: 0,
    });
    let selection = am_backend_selection_t {
        struct_size: std::mem::size_of::<am_backend_selection_t>() as u32,
        candidate_count: 1,
        candidates: candidates.as_ptr(),
        allow_fallback: 0,
        budget_ms: 1500,
        device: selector.as_ref().map_or(ptr::null(), |s| s as *const _),
    };
    let desc = am_model_desc_t {
        struct_size: std::mem::size_of::<am_model_desc_t>() as u32,
        path_utf8: path.as_ptr(),
        load_mode: AM_LOAD_RAW,
        profile_id: ptr::null(),
        backend: ptr::null(),
        intra_threads: 2,
    };
    let mut out = ptr::null_mut();
    let mut id = 0u64;
    let rc = unsafe { amv_model_load_selected(ctx as _, &desc, &selection, &mut out, &mut id) };
    Selected {
        rc,
        model: out as usize,
        id,
    }
}

/// 失败加载的尝试报告（候选阶段/错误串），供边界登记取证。
fn attempt_report(ctx: usize, id: u64) {
    let mut buffer = [0u8; 8192];
    let mut required = 0u64;
    if unsafe {
        amv_get_selection_attempt_report(
            ctx as _,
            id,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u64,
            &mut required,
        )
    } == 0
    {
        let json: serde_json::Value =
            serde_json::from_slice(&buffer[..required as usize - 1]).unwrap();
        eprintln!("V85_ATTEMPT_FAIL {json}");
    }
}

fn model_device(model: usize) -> (u32, String) {
    let mut info = am_model_info_t::default();
    assert_eq!(unsafe { amv_model_get_info(model as _, &mut info) }, 0);
    assert!(
        info.execution.fallback_reason_utf8.is_null(),
        "strict 不回退"
    );
    let label = unsafe { std::ffi::CStr::from_ptr(info.execution.device_utf8) }
        .to_string_lossy()
        .into_owned();
    (info.execution.actual, label)
}

/// selection 报告回读候选 device_id（selector 解析真值；DXGI 0 号无标签后缀）。
fn selection_targets(model: usize) -> Vec<(u32, u32)> {
    let mut buffer = [0u8; 8192];
    let mut required = 0u64;
    assert_eq!(
        unsafe {
            amv_get_model_selection_report(
                model as _,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u64,
                &mut required,
            )
        },
        0
    );
    // required 含 NUL 终结（copy_report 语义）：JSON 实体为 required-1 字节。
    let json: serde_json::Value = serde_json::from_slice(&buffer[..required as usize - 1]).unwrap();
    json["selection"]["candidates"]
        .as_array()
        .expect("selection 候选")
        .iter()
        .map(|c| {
            (
                c["backend"].as_u64().expect("backend") as u32,
                c["device_id"].as_u64().unwrap_or(0) as u32,
            )
        })
        .collect()
}

fn expect_of(rgb: [f32; 3]) -> [f32; 9] {
    let mut expected = [0f32; 9];
    for c in 0..3 {
        expected[c] = rgb[c];
        expected[3 + c] = rgb[c] * 2.0;
        expected[6 + c] = rgb[c] * 4.0;
    }
    expected
}

/// 单次同步推理并取回 out0 九元组；返回（值，端到端毫秒）。
fn infer_identity(ctx: usize, model: usize, png: &[u8]) -> ([f32; 9], f64) {
    let input = am_image_input_t {
        kind: AM_IMG_PNG,
        encoded: png.as_ptr(),
        encoded_size: png.len() as u64,
        ..Default::default()
    };
    let raw = am_raw_preprocess_t {
        struct_size: std::mem::size_of::<am_raw_preprocess_t>() as u32,
        resize: AM_RESIZE_LETTERBOX,
        target_w: 224,
        target_h: 224,
        fill_value: 114.0,
        color_order: AM_COLOR_RGB,
        normalize: AM_NORM_DIV255,
        ..Default::default()
    };
    let params = am_infer_params_t {
        raw_preprocess: &raw,
        ..Default::default()
    };
    let begin = Instant::now();
    let mut out = ptr::null_mut();
    let mut id = 0u64;
    assert_eq!(
        unsafe { amv_infer_image_sync(model as _, &input, &params, &mut out, &mut id) },
        0,
        "推理应成功"
    );
    let ms = begin.elapsed().as_secs_f64() * 1000.0;
    let result = unsafe { &*out };
    assert_eq!(result.raw_output_count, 1, "单输出模型");
    let tensor = unsafe { &*result.raw_outputs };
    assert_eq!(tensor.shape_len, 2);
    let shape = unsafe { std::slice::from_raw_parts(tensor.shape, 2) };
    assert_eq!(shape, &[1, 9]);
    let bytes =
        unsafe { std::slice::from_raw_parts(tensor.data.cast::<u8>(), tensor.data_size as usize) };
    let values: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    unsafe { amv_result_release(out) };
    assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0);
    (values.try_into().expect("out0 应为 9 个 f32"), ms)
}

fn assert_identity(label: &str, values: [f32; 9], rgb: [f32; 3]) -> f32 {
    let expected = expect_of(rgb);
    let mut max_dev = 0.0f32;
    for (i, (a, b)) in values.iter().zip(&expected).enumerate() {
        max_dev = max_dev.max((a - b).abs());
        assert!(
            (a - b).abs() < 2e-3,
            "{label}[{i}] 身份漂移: got={a} want={b}"
        );
    }
    max_dev
}

fn percentile(mut sorted: Vec<f64>, p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted.sort_by(|a, b| a.total_cmp(b));
    let index = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

struct LaneStat {
    frames: u64,
    latencies: Vec<f64>,
}

fn lane_loop(
    ctx: usize,
    model: usize,
    png: &[u8],
    rgb: [f32; 3],
    deadline: Instant,
    label: &str,
) -> LaneStat {
    let mut latencies = Vec::new();
    let mut frames = 0u64;
    while Instant::now() < deadline {
        let (values, ms) = infer_identity(ctx, model, png);
        assert_identity(&format!("{label} f{frames}"), values, rgb);
        latencies.push(ms);
        frames += 1;
    }
    LaneStat { frames, latencies }
}

fn report_lanes(prefix: &str, stats: &[LaneStat], window_ms: u64) {
    let total: u64 = stats.iter().map(|s| s.frames).sum();
    let secs = window_ms as f64 / 1000.0;
    eprintln!(
        "{prefix} window_ms={window_ms} lanes={} total_frames={total} agg_fps={:.1}",
        stats.len(),
        total as f64 / secs
    );
    for (lane, stat) in stats.iter().enumerate() {
        let sorted = stat.latencies.clone();
        eprintln!(
            "{prefix} lane={lane} frames={} fps={:.1} p50_ms={:.2} p95_ms={:.2} p99_ms={:.2}",
            stat.frames,
            stat.frames as f64 / secs,
            percentile(sorted.clone(), 0.50),
            percentile(sorted.clone(), 0.95),
            percentile(sorted, 0.99),
        );
    }
}

fn run_window(
    ctx: usize,
    model: usize,
    lanes: usize,
    window_ms: u64,
    pngs: &[Vec<u8>],
) -> Vec<LaneStat> {
    let deadline = Instant::now() + Duration::from_millis(window_ms);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..lanes)
            .map(|lane| {
                let png = &pngs[lane % pngs.len()];
                let rgb = COLORS[lane % COLORS.len()].1;
                scope.spawn(move || lane_loop(ctx, model, png, rgb, deadline, "soak"))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("lane 线程"))
            .collect()
    })
}

/// 基准臂：同模型同输入按后端测吞吐/延迟（独立进程跑：DML/CUDA/CPU/OV 原生运行库
/// 互斥，口径分列依据）。FB_V85_BENCH_BACKEND=0|2|3|4，FB_V85_BENCH_MATCH 可选定向
/// 子串，FB_V85_BENCH_MS 默认 60000（单/多路两窗），FB_V85_OV_DIR 为 backend=4 的
/// OpenVINO 原生运行库目录。
#[test]
#[ignore = "需真实 ORT 运行库（AM_SDK_ORT_DIR）与合成批模型夹具（FB_ROI_FIXTURES）；DML 定向臂需 ort-dml-124+本机 Arc 核显（V8.5）"]
fn device_selector_e2e_targets_intel_igpu() {
    let ort_dir = env_path("AM_SDK_ORT_DIR");
    let fixtures = env_path("FB_ROI_FIXTURES");
    let model_path = CString::new(
        fixtures
            .join("batch-image")
            .join("image-dynamic.onnx")
            .to_str()
            .unwrap(),
    )
    .unwrap();
    let pngs: Vec<Vec<u8>> = COLORS
        .iter()
        .map(|(name, _)| std::fs::read(fixtures.join("batch-image").join(format!("{name}.png"))))
        .collect::<Result<_, _>>()
        .expect("四色 PNG 夹具");

    if let Some(backend) = std::env::var_os("FB_V85_BENCH_BACKEND") {
        let backend: u32 = backend.to_string_lossy().parse().expect("backend 编号");
        let needle = std::env::var_os("FB_V85_BENCH_MATCH")
            .map(|v| CString::new(v.to_string_lossy().into_owned()).unwrap());
        let bench_ms = env_u64("FB_V85_BENCH_MS", 60_000);
        let mut dirs = vec![ort_dir.to_string_lossy().into_owned()];
        if backend == AM_BACKEND_OPENVINO_NATIVE as u32 {
            dirs.push(env_path("FB_V85_OV_DIR").to_string_lossy().into_owned());
        }
        let mut guard = CtxGuard {
            ctx: init_ctx(&dirs, 2),
            models: Vec::new(),
        };
        let selected = load_selected(guard.ctx, &model_path, backend, needle.as_ref());
        if selected.rc != 0 {
            if let Ok((_, Some(detail))) =
                am_sdk_support::diagnostics::snapshot(domain(), guard.ctx)
            {
                eprintln!(
                    "V85_BENCH_LOAD_FAIL rc={} detail={}",
                    selected.rc,
                    detail.message.to_string_lossy()
                );
            }
            attempt_report(guard.ctx, selected.id);
        }
        assert_eq!(selected.rc, 0, "基准臂加载应成功");
        guard.track(selected.model);
        let (actual, label) = model_device(selected.model);
        let targets = selection_targets(selected.model);
        eprintln!(
            "V85_BENCH_LOAD backend={backend} actual={actual} label={label} targets={targets:?}"
        );
        for _ in 0..20 {
            let (values, _) = infer_identity(guard.ctx, selected.model, &pngs[0]);
            assert_identity("bench-warmup", values, COLORS[0].1);
        }
        let single = run_window(guard.ctx, selected.model, 1, bench_ms, &pngs);
        report_lanes("V85_BENCH", &single, bench_ms);
        let multi = run_window(guard.ctx, selected.model, 4, bench_ms, &pngs);
        report_lanes("V85_BENCH", &multi, bench_ms);
        return;
    }

    // —— 门主体：DML 定向（进程只挂 DML 运行库）——
    let mut guard = CtxGuard {
        ctx: init_ctx(&[ort_dir.to_string_lossy().into_owned()], 2),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let mut capability = amv_capability_t::default();
    assert_eq!(
        unsafe { crate::amv_query_capability(ctx as _, &mut capability) },
        0
    );
    assert_ne!(
        u64::from(capability.feature_bits) & AMV_CAP_DEVICE_SELECTOR,
        0,
        "能力位 bit12（设备定向）"
    );

    // 期望 DXGI 序独立枚举（与本门断言同源的公开纯函数）。
    let adapters = dxgi_adapters().expect("DXGI 枚举");
    let hardware: Vec<(u32, &str)> = adapters
        .iter()
        .filter(|a| !a.software)
        .map(|a| (a.index, a.description.as_str()))
        .collect();
    let arc = select_by_substring(hardware.iter().copied(), "Arc", 0)
        .expect("本机需有 Arc 核显（非软件适配器）");
    let rtx = select_by_substring(hardware.iter().copied(), "RTX", 0).expect("对照臂需有 RTX 独显");
    eprintln!("V85_SEL arc_dxgi={arc} rtx_dxgi={rtx} inventory={hardware:?}");

    // 1) 定向 Arc：加载成功+真值标签+推理身份。
    let arc_needle = CString::new("Arc").unwrap();
    let selected = load_selected(
        ctx,
        &model_path,
        AM_BACKEND_DIRECTML as u32,
        Some(&arc_needle),
    );
    assert_eq!(selected.rc, 0, "定向 Arc 的 DML 加载应成功");
    guard.track(selected.model);
    let (actual, label) = model_device(selected.model);
    assert_eq!(actual, AM_BACKEND_DIRECTML as u32);
    // DXGI 0 号无 ":0" 后缀（device_label 语义）；selection 报告回读精确序号。
    let expected_label = if arc != 0 {
        format!("DML:{arc}")
    } else {
        "DML".to_owned()
    };
    assert_eq!(label, expected_label, "device_label 应为定向命中真值");
    assert_eq!(
        selection_targets(selected.model),
        vec![(AM_BACKEND_DIRECTML as u32, arc)],
        "selector 解析应写入 DXGI 序号"
    );
    let (values, infer_ms) = infer_identity(ctx, selected.model, &pngs[0]);
    let dev = assert_identity("arc/red", values, COLORS[0].1);
    eprintln!("V85_HIT arm=arc label={label} identity_max_dev={dev:.6} infer_ms={infer_ms:.2}");

    // 2) 对照臂：同款定向 RTX（DML 命中独显）。
    let rtx_needle = CString::new("RTX").unwrap();
    let control = load_selected(
        ctx,
        &model_path,
        AM_BACKEND_DIRECTML as u32,
        Some(&rtx_needle),
    );
    assert_eq!(control.rc, 0, "对照臂 RTX 的 DML 加载应成功");
    guard.track(control.model);
    let (c_actual, c_label) = model_device(control.model);
    assert_eq!(c_actual, AM_BACKEND_DIRECTML as u32);
    assert_eq!(
        c_label,
        if rtx != 0 {
            format!("DML:{rtx}")
        } else {
            "DML".to_owned()
        },
        "对照臂标签应命中独显序号"
    );
    let (values, control_ms) = infer_identity(ctx, control.model, &pngs[1]);
    assert_identity("rtx/green", values, COLORS[1].1);
    eprintln!("V85_HIT arm=rtx label={c_label} infer_ms={control_ms:.2}");

    // 3) 零命中子串：invalidArgument 同步拒绝（不静默回退）。
    let ghost = CString::new("nonexistent-gpu").unwrap();
    let rejected = load_selected(ctx, &model_path, AM_BACKEND_DIRECTML as u32, Some(&ghost));
    assert_eq!(
        rejected.rc, AM_ERR_SDK_INVALID_ARGUMENT,
        "零命中应同步 invalidArgument"
    );
    assert_eq!(rejected.model, 0, "拒绝不得产出模型句柄");
    eprintln!("V85_REJECT needle=nonexistent-gpu rc={}", rejected.rc);

    // 4) DML/CUDA 同进程互斥探针：DML 已初始化后，CUDA 线 ORT 目录的任何加载
    //    尝试都须 backendConflict（1026）且 DML 会话不受影响（反向顺序对称，
    //    canonical 不等即拒——runtime.rs initialize 锁路径）。
    let cuda_dir = env_path("FB_V85_CUDA_ORT_DIR");
    assert!(
        cuda_dir.join("onnxruntime.dll").is_file(),
        "互斥探针需 CUDA 线 ORT 目录（含 onnxruntime.dll）"
    );
    // 第二 ctx 线程配置须与进程内执行器单例一致（0=继承首 ctx 的预算）。
    let ctx2 = init_ctx(&[cuda_dir.to_string_lossy().into_owned()], 0);
    let policy = am_backend_policy_t {
        requested: AM_BACKEND_CUDA as u32,
        strict: 1,
        ..Default::default()
    };
    let desc = am_model_desc_t {
        struct_size: std::mem::size_of::<am_model_desc_t>() as u32,
        path_utf8: model_path.as_ptr(),
        load_mode: AM_LOAD_RAW,
        profile_id: ptr::null(),
        backend: &policy,
        intra_threads: 1,
    };
    let mut out = ptr::null_mut();
    let legacy_rc = unsafe { amv_model_load(ctx2 as _, &desc, &mut out) };
    let selected_rc = load_selected(ctx2, &model_path, AM_BACKEND_CUDA as u32, None).rc;
    assert_eq!(
        legacy_rc, AM_ERR_VISION_BACKEND_CONFLICT,
        "legacy 加载应互斥拒绝"
    );
    assert_eq!(
        selected_rc, AM_ERR_VISION_BACKEND_CONFLICT,
        "selection 加载应互斥拒绝"
    );
    let (values, _) = infer_identity(ctx, selected.model, &pngs[2]);
    assert_identity("post-mutex/blue", values, COLORS[2].1);
    eprintln!("V85_MUTEX dml_first=true legacy_rc={legacy_rc} selected_rc={selected_rc} dml_still_usable=true");
    let _ = domain().close(ctx2, 5000);
    let _ = domain().destroy(ctx2);

    // 5) 多路浸泡：默认 60s 冒烟；验收窗口 FB_V85_SOAK_MS=600000（≥10 分钟）。
    let soak_ms = env_u64("FB_V85_SOAK_MS", 60_000);
    let stats = run_window(ctx, selected.model, 4, soak_ms, &pngs);
    report_lanes("V85_SOAK", &stats, soak_ms);
    let floor = 10u64 * (soak_ms / 60_000).max(1);
    for (lane, stat) in stats.iter().enumerate() {
        assert!(
            stat.frames >= floor,
            "lane {lane} 帧数 {} 低于稳定下限 {floor}",
            stat.frames
        );
    }
}
