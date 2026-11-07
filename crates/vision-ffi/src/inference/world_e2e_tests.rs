//! 批 D 真实权重端到端门（ignore 显式跑）：真实 world.onnx（.local/tests/yolo-world，
//! 攒批夹具为其仅追加 am.sdk.batch_axis 元数据的派生副本）+ 真实 text.onnx（量化
//! CLIP 文本塔）走 FFI 全链——vocab bind（真实文本塔编码词表）→ CPU 注入推理 /
//! GPU 帧直传对拍 → 攒批窗口（真实 PROFILE 图像攒批）vs 逐帧单跑对拍。
//! 诚实边界：仓内无 CLIP tokenizer，token ids 为词表区间内的确定性序列——本门
//! 验证管线正确性（装载/注入/解码/hits/label/批语义），不主张检测语义质量；
//! 真实画面（R0.2 帧样本）仅用于几何与数值合理性断言。
use super::batching_exports::{amv_model_configure_batching, amv_model_get_batching};
use super::exports::*;
use crate::domain;
use am_sdk_support::{abi::*, context};
use std::ffi::{c_char, CString};
use std::time::{Duration, Instant};

const RUNTIME_DIR: &str = "E:\\projects\\framebraid\\.local\\runtimes\\ort-gpu-131-final";
const REAL: &str = "E:\\projects\\framebraid\\.local\\tests\\yolo-world";
const BATCHD: &str = "E:\\projects\\framebraid\\.local\\tests\\yolo-world-batchd";
/// CLIP 词表界内确定性 ids（BOS/EOS/0 pad；第二行掺入中位 id 产生互异嵌入）。
const CLIP_BOS: i64 = 49406;
const CLIP_EOS: i64 = 49407;
const TOKEN_COLS: usize = 77;

fn init_ctx() -> usize {
    let names: Vec<CString> = vec![CString::new(RUNTIME_DIR).unwrap()];
    let pointers: Vec<*const c_char> = names.iter().map(|d| d.as_ptr()).collect();
    let params = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: pointers.as_ptr(),
        runtime_dir_count: 1,
        infer_threads: 2,
        ..Default::default()
    };
    unsafe { context::initialize_vision(domain(), &params) }.expect("vision ctx")
}

fn load_model(ctx: usize, path: &str, profile: &str, backend: Option<u32>) -> usize {
    let cpath = CString::new(path).unwrap();
    let cprofile = CString::new(profile).unwrap();
    let policy = backend.map(|requested| am_backend_policy_t {
        requested,
        strict: 1,
        ..Default::default()
    });
    let desc = am_model_desc_t {
        struct_size: std::mem::size_of::<am_model_desc_t>() as u32,
        path_utf8: cpath.as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        profile_id: cprofile.as_ptr(),
        backend: policy.as_ref().map_or(std::ptr::null(), |p| p as *const _),
        ..Default::default()
    };
    let mut key = std::ptr::null_mut();
    assert_eq!(
        unsafe { amv_model_load(ctx as _, &desc, &mut key) },
        0,
        "{path}"
    );
    key as usize
}

fn prompt_row(seed: i64) -> Vec<i64> {
    let mut row = vec![0i64; TOKEN_COLS];
    row[0] = CLIP_BOS;
    row[1] = seed;
    row[2] = CLIP_EOS;
    row
}

/// 绑定双条目词表（worker 两提示词 / helmet 一提示词，嵌入互异）。
fn bind_vocab(detector: usize, tower: usize) -> Vec<CString> {
    let labels = [
        CString::new("worker").unwrap(),
        CString::new("helmet").unwrap(),
    ];
    let worker: Vec<i64> = [prompt_row(CLIP_EOS), prompt_row(320)].concat();
    let helmet: Vec<i64> = prompt_row(777);
    let entries = [
        am_vocab_entry_t {
            struct_size: std::mem::size_of::<am_vocab_entry_t>() as u32,
            label_utf8: labels[0].as_ptr(),
            token_ids: worker.as_ptr(),
            token_rows: 2,
            token_cols: TOKEN_COLS as u32,
        },
        am_vocab_entry_t {
            struct_size: std::mem::size_of::<am_vocab_entry_t>() as u32,
            label_utf8: labels[1].as_ptr(),
            token_ids: helmet.as_ptr(),
            token_rows: 1,
            token_cols: TOKEN_COLS as u32,
        },
    ];
    assert_eq!(
        unsafe { amv_vocab_bind(detector as _, tower as _, entries.as_ptr(), 2) },
        0
    );
    labels.into()
}

#[derive(Clone, Debug)]
struct BoxHit {
    bounds: [f32; 4],
    confidence: f32,
    class_id: i32,
    label: String,
}

fn collect(result: &amv_result_t) -> Vec<BoxHit> {
    assert_eq!(result.task, AM_TASK_DETECT);
    let count = result.detect.count as usize;
    let hits = unsafe { &*result.vocab_hits };
    assert_eq!(hits.count as usize, count, "vocab hits 与框同序同数");
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let box0 = unsafe { &*result.detect.boxes.add(index) };
        let hit = unsafe { &*hits.hits.add(index) };
        assert_eq!(hit.entry_index, box0.class_id, "hit 条目=框类目");
        for value in box0.bounds {
            assert!(
                (0.0..=1.0).contains(&value),
                "归一化框界: {:?}",
                box0.bounds
            );
        }
        assert!(
            box0.confidence > 0.0 && box0.confidence <= 1.0,
            "置信度界: {}",
            box0.confidence
        );
        let label = unsafe { std::ffi::CStr::from_ptr(box0.class_name_utf8) }
            .to_string_lossy()
            .into_owned();
        out.push(BoxHit {
            bounds: box0.bounds,
            confidence: box0.confidence,
            class_id: box0.class_id,
            label,
        });
    }
    out
}

fn infer_png_sync(
    ctx: usize,
    model: usize,
    png: &[u8],
    confidence: f32,
    prompt_rows: usize,
) -> (Vec<BoxHit>, Vec<f32>) {
    let input = am_image_input_t {
        kind: AM_IMG_PNG,
        encoded: png.as_ptr(),
        encoded_size: png.len() as u64,
        ..Default::default()
    };
    let params = am_infer_params_t {
        confidence_threshold: confidence,
        ..Default::default()
    };
    let mut out: *mut amv_result_t = std::ptr::null_mut();
    let mut id = 0u64;
    assert_eq!(
        unsafe { amv_infer_image_sync(model as _, &input, &params, &mut out, &mut id) },
        0
    );
    let result = unsafe { &*out };
    assert!(!result.vocab_hits.is_null(), "vocab 路径携带 hits");
    let boxes = collect(result);
    let (values, _) = raw_head(result, prompt_rows);
    unsafe { amv_result_release(out) };
    let _ = unsafe { amv_result_ack(ctx as _, id) };
    (boxes, values)
}

/// 单帧 output0 原始张量：(values, shape)。PROFILE 结果携带原始输出——
/// 直接断言真实 world 布局 [1, 4+提示词行, 8400]（txt_feats 行数=输出通道数的
/// 真模型语义），并从亲和分通道（行 4..）取最高分做对拍阈值自标定。
fn raw_head(result: &amv_result_t, prompt_rows: usize) -> (Vec<f32>, Vec<i64>) {
    assert_eq!(result.raw_output_count, 1, "单输出检测头");
    let tensor = unsafe { &*result.raw_outputs };
    let shape =
        unsafe { std::slice::from_raw_parts(tensor.shape, tensor.shape_len as usize).to_vec() };
    assert_eq!(
        shape,
        vec![1i64, (4 + prompt_rows) as i64, 8400],
        "真实 world 输出布局 [1, 4+提示词行, 8400]"
    );
    let bytes =
        unsafe { std::slice::from_raw_parts(tensor.data.cast::<u8>(), tensor.data_size as usize) };
    let values = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    (values, shape)
}

/// 亲和分最高分（通道行 4.. 各格点）：对拍阈值自标定基线。
fn top_affinity(values: &[f32], prompt_rows: usize) -> f32 {
    let anchors = 8400usize;
    values[4 * anchors..(4 + prompt_rows) * anchors]
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max)
}

/// 亲和分通道切片（行 4..，sigmoid 界内）：跨预处理域对拍用——坐标通道是
/// 模型内文本条件回归量，非语义嵌入下逐锚点不稳定（噪声段坐标漂移大），不
/// 作跨域判据；同域（同输入）全输出对拍不受此影响。
fn score_channels(values: &[f32], prompt_rows: usize) -> &[f32] {
    &values[4 * 8400..(4 + prompt_rows) * 8400]
}

fn wait_terminal(ctx: usize, id: u64) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let mut snap: *mut am_request_snapshot_t = std::ptr::null_mut();
        assert_eq!(unsafe { amv_request_query(ctx as _, id, &mut snap) }, 0);
        let state = unsafe { (*snap).state };
        let terminal = unsafe { (*snap).terminal };
        unsafe { amv_request_snapshot_release(snap) };
        if terminal != 0 {
            return state as i32;
        }
        assert!(Instant::now() < deadline, "等待终态超时 id={id}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn fetch_boxes(ctx: usize, id: u64, prompt_rows: usize) -> (Vec<BoxHit>, Vec<f32>) {
    let mut out: *mut amv_result_t = std::ptr::null_mut();
    assert_eq!(unsafe { amv_result_fetch(ctx as _, id, &mut out) }, 0);
    let result = unsafe { &*out };
    let boxes = collect(result);
    let (values, _) = raw_head(result, prompt_rows);
    unsafe { amv_result_release(out) };
    assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0);
    (boxes, values)
}

fn iou(a: [f32; 4], b: [f32; 4]) -> f32 {
    let (x0, y0) = (a[0].max(b[0]), a[1].max(b[1]));
    let (x1, y1) = (a[2].min(b[2]), a[3].min(b[3]));
    let inter = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
    let area = |v: [f32; 4]| (v[2] - v[0]) * (v[3] - v[1]);
    inter / (area(a) + area(b) - inter).max(1e-9)
}

/// 原始输出对拍（同域主判据，同批 C world 组装门口径）：逐元素 |Δ| 上界 =
/// max(rel×幅值, floor)。同域（同输入仅合批差）收紧；跨预处理域不适用（近零
/// sigmoid 的相对漂移由 logit 敏感度主导，BT.601 色度抽样扰动不可收紧）。
fn assert_raw_close(baseline: &[f32], candidate: &[f32], rel: f32, floor: f32, label: &str) {
    assert_eq!(baseline.len(), candidate.len(), "{label} 元素数一致");
    let scale = baseline
        .iter()
        .zip(candidate)
        .fold(0f32, |m, (a, b)| m.max(a.abs()).max(b.abs()));
    let max_diff = baseline
        .iter()
        .zip(candidate)
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    eprintln!("[world-raw] {label}: max|Δ|={max_diff:.6} scale={scale:.4}");
    assert!(
        max_diff <= (rel * scale).max(floor),
        "{label} raw 漂移过界: {max_diff} > max({rel}×{scale}, {floor})"
    );
}

/// 跨预处理域相关性判据（GPU NV12 vs CPU PNG）：Pearson 相关 ≥ floor——同图像
/// 两侧亲和分分布应同结构；幅值/偏移域差（BT.601 往返）不进判据。
fn assert_scores_correlate(a: &[f32], b: &[f32], floor: f64, label: &str) {
    assert_eq!(a.len(), b.len());
    let n = a.len() as f64;
    let (ma, mb) = (
        a.iter().sum::<f32>() as f64 / n,
        b.iter().sum::<f32>() as f64 / n,
    );
    let mut cov = 0.0;
    let (mut va, mut vb) = (0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (*x as f64 - ma, *y as f64 - mb);
        cov += x * y;
        va += x * x;
        vb += y * y;
    }
    let correlation = cov / (va.sqrt() * vb.sqrt());
    eprintln!("[world-raw] {label}: Pearson r={correlation:.4}");
    assert!(
        correlation >= floor,
        "{label} 相关性过界: {correlation} < {floor}"
    );
}

/// 贪心 IoU 几何对拍（辅证）：逐基线框找最佳匹配，IoU 与置信差过界即失败；
/// 框数允许 |Δ|≤tolerance（阈值边界锚点翻转余量）。`top_n` 限定只对拍置信度
/// 最高的前 n 框（跨预处理域用——近阈边界框集合跨域不保证稳定，top 框是
/// 分离良好的稳定带）；None=全量（同域用）。类目一致性由同域 raw 对拍蕴含。
fn assert_boxes_match(
    baseline: &[BoxHit],
    candidate: &[BoxHit],
    tolerance: usize,
    min_iou: f32,
    top_n: Option<usize>,
) {
    assert!(
        (baseline.len() as isize - candidate.len() as isize).abs() <= tolerance as isize,
        "框数漂移 baseline={} candidate={}",
        baseline.len(),
        candidate.len()
    );
    let matchable = baseline.len().min(candidate.len());
    let mut remaining: Vec<&BoxHit> = candidate.iter().collect();
    for base in baseline
        .iter()
        .take(matchable.min(top_n.unwrap_or(usize::MAX)))
    {
        let Some((index, _)) = remaining
            .iter()
            .enumerate()
            .map(|(i, c)| (i, iou(base.bounds, c.bounds)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, overlap)| *overlap >= min_iou)
        else {
            panic!("无 IoU>={min_iou} 匹配框: {base:?} in {candidate:?}");
        };
        let matched = remaining.remove(index);
        assert!(
            (matched.confidence - base.confidence).abs() < 0.05,
            "置信漂移 {} vs {}",
            matched.confidence,
            base.confidence
        );
    }
}

/// 真实权重 CPU 全链：装载（dynseq 塔动态 seq + 批动态 world 档案）→ 真实文本塔
/// 编码词表 → 注入推理 → 亲和解码/hits/label 出口 → 换词表原子换代。
#[test]
#[ignore = "需真实 ORT 与 yolo-world 双模型（.local/tests/yolo-world）"]
fn world_e2e_cpu_real_vocab_pipeline() {
    let ctx = init_ctx();
    let tower = load_model(
        ctx,
        &format!("{REAL}\\text.onnx"),
        "text-encoder-clip-dynseq-v1",
        None,
    );
    let detector = load_model(
        ctx,
        &format!("{BATCHD}\\world-batch.onnx"),
        "detect-yolo-world-batch-v1",
        None,
    );
    // 批 D 攒批资格断言：批动态档案 + 派生共同首轴声明 → native_batch=1。
    let mut info = am_model_info_t::default();
    assert_eq!(unsafe { amv_model_get_info(detector as _, &mut info) }, 0);
    assert_eq!(info.native_batch, 1, "动态批轴判定");
    let labels = bind_vocab(detector, tower);
    // 真实帧（R0.2 样本）：output0 布局断言（真实 world 通道=4+提示词行）+ 亲和分
    // 最高分自标定阈值（仓内无 tokenizer，嵌入非语义配对，分数分布运行期定）。
    let png = std::fs::read(format!("{BATCHD}\\s1_f013.png")).expect("frame png");
    let (_, probe) = infer_png_sync(ctx, detector, &png, 1.0, 3);
    let top = top_affinity(&probe, 3);
    assert!(top.is_finite() && top > 0.0, "亲和分通道有限 {top}");
    let threshold = (top * 0.6).max(1e-5);
    eprintln!("[world-cpu] 阈值自标定 top={top:.4} threshold={threshold:.4}");
    let boxes = infer_png_sync(ctx, detector, &png, threshold, 3).0;
    assert!(
        !boxes.is_empty(),
        "真实画面自标定阈值应有检出（解码真实发生）"
    );
    for box0 in &boxes {
        let expected = labels
            .get(box0.class_id as usize)
            .map_or_else(String::new, |l| l.to_string_lossy().into_owned());
        assert_eq!(
            box0.label, expected,
            "label 必须与 class_id（条目索引）对应"
        );
        assert!(
            box0.bounds[2] > box0.bounds[0] && box0.bounds[3] > box0.bounds[1],
            "框非退化 {:?}",
            box0.bounds
        );
    }
    eprintln!("[world-cpu] conf>={threshold:.5} 检出 {} 框", boxes.len());
    // 换词表：单条目 relabel → label 出口跟随（原子换代语义）。
    let relabel = [CString::new("zone").unwrap()];
    let tokens = prompt_row(CLIP_EOS);
    let reentry = [am_vocab_entry_t {
        struct_size: std::mem::size_of::<am_vocab_entry_t>() as u32,
        label_utf8: relabel[0].as_ptr(),
        token_ids: tokens.as_ptr(),
        token_rows: 1,
        token_cols: TOKEN_COLS as u32,
    }];
    let _keep = (&relabel, &tokens);
    assert_eq!(
        unsafe { amv_vocab_bind(detector as _, tower as _, reentry.as_ptr(), 1) },
        0
    );
    // 换词表后通道=4+1：布局随词表行数联动；阈值随新嵌入重新自标定。
    let (_, reprobe) = infer_png_sync(ctx, detector, &png, 1.0, 1);
    let top = top_affinity(&reprobe, 1);
    let rethreshold = (top * 0.6).max(1e-5);
    let reboxed = infer_png_sync(ctx, detector, &png, rethreshold, 1).0;
    assert!(!reboxed.is_empty(), "换词表后仍应可检出");
    for box0 in &reboxed {
        assert_eq!(box0.label, "zone", "换词表后 label 跟随");
    }
    unsafe { amv_model_close(detector as _, 0, std::ptr::null_mut()) };
    unsafe { amv_model_close(tower as _, 0, std::ptr::null_mut()) };
}

/// 真实权重 GPU 全链：同帧 NV12 GPU 帧直传 vs CPU PNG 对拍（BT.601 往返容差）；
/// 攒批窗口四帧真攒批 vs 逐帧单跑对拍（同后端，容差收紧）+窗口统计。
#[test]
#[ignore = "需真实 NVIDIA GPU 与 yolo-world 双模型（.local/tests/yolo-world 及 -batchd 派生件）"]
fn world_e2e_gpu_direct_and_batch_window_match_single_runs() {
    use am_vision_core::gpu::{
        cuda_sys::CudaApi, interop::nvidia_nv12_texture, interop::RegisteredTexture,
        pipeline::GpuPipeline,
    };
    let ctx = init_ctx();
    let tower = load_model(
        ctx,
        &format!("{REAL}\\text.onnx"),
        "text-encoder-clip-dynseq-v1",
        None,
    );
    let detector = load_model(
        ctx,
        &format!("{BATCHD}\\world-batch.onnx"),
        "detect-yolo-world-batch-v1",
        Some(2),
    );
    let mut info = am_model_info_t::default();
    assert_eq!(unsafe { amv_model_get_info(detector as _, &mut info) }, 0);
    assert!(info.execution.actual == 2, "CUDA strict 装载");
    assert_eq!(info.native_batch, 1);
    let labels = bind_vocab(detector, tower);
    let _ = &labels;
    const FRAMES: [&str; 4] = ["seek_f030", "seek_f031", "s1_f013", "s1_f014"];
    let runtime = CudaApi::load().expect("cudart");
    GpuPipeline::global().expect("GPU 管线");
    unsafe { (runtime.set_device)(0) };

    let gpu_sync = |frame: &str, threshold: f32| -> (Vec<BoxHit>, Vec<f32>) {
        let nv12 = std::fs::read(format!("{BATCHD}\\{frame}.nv12")).expect("nv12");
        let (w, h) = (640usize, 640usize);
        let texture = unsafe { nvidia_nv12_texture(640, 640).expect("NV12 纹理") };
        unsafe {
            let registered = RegisteredTexture::register(runtime.clone(), texture.0).expect("注册");
            let y = registered.map_subresource(0).expect("Y map");
            let uv = registered.map_subresource(1).expect("UV map");
            (runtime.memcpy2d_to_array)(y.array, 0, 0, nv12.as_ptr().cast(), w, w, h, 1);
            let uv_ptr = nv12.as_ptr().add(w * h);
            (runtime.memcpy2d_to_array)(uv.array, 0, 0, uv_ptr.cast(), w, w, h / 2, 1);
        }
        let gpu_ref = am_gpu_frame_ref_t {
            width: 640,
            height: 640,
            d3d_format: 103,
            texture: texture.0,
            ..Default::default()
        };
        let input = am_image_input_t {
            kind: AM_IMG_GPU_FRAME,
            gpu: &gpu_ref,
            ..Default::default()
        };
        let params = am_infer_params_t {
            confidence_threshold: threshold,
            ..Default::default()
        };
        let mut out: *mut amv_result_t = std::ptr::null_mut();
        let mut id = 0u64;
        let rc = unsafe { amv_infer_image_sync(detector as _, &input, &params, &mut out, &mut id) };
        if rc != 0 {
            if let Ok((_, Some(detail))) = am_sdk_support::diagnostics::snapshot(domain(), detector)
            {
                eprintln!(
                    "[world-gpu] rc={rc} detail={}",
                    detail.message.to_string_lossy()
                );
            }
            panic!("GPU 直传应成功 rc={rc}");
        }
        let boxes = collect(unsafe { &*out });
        let (values, _) = raw_head(unsafe { &*out }, 3);
        unsafe { amv_result_release(out) };
        let _ = unsafe { amv_result_ack(ctx as _, id) };
        (boxes, values)
    };
    let submit_gpu_async = |frame: &str, threshold: f32| -> u64 {
        let nv12 = std::fs::read(format!("{BATCHD}\\{frame}.nv12")).expect("nv12");
        let (w, h) = (640usize, 640usize);
        let texture = unsafe { nvidia_nv12_texture(640, 640).expect("NV12 纹理") };
        unsafe {
            let registered = RegisteredTexture::register(runtime.clone(), texture.0).expect("注册");
            let y = registered.map_subresource(0).expect("Y map");
            let uv = registered.map_subresource(1).expect("UV map");
            (runtime.memcpy2d_to_array)(y.array, 0, 0, nv12.as_ptr().cast(), w, w, h, 1);
            let uv_ptr = nv12.as_ptr().add(w * h);
            (runtime.memcpy2d_to_array)(uv.array, 0, 0, uv_ptr.cast(), w, w, h / 2, 1);
        }
        let gpu_ref = am_gpu_frame_ref_t {
            width: 640,
            height: 640,
            d3d_format: 103,
            texture: texture.0,
            ..Default::default()
        };
        let input = am_image_input_t {
            kind: AM_IMG_GPU_FRAME,
            gpu: &gpu_ref,
            ..Default::default()
        };
        let params = am_infer_params_t {
            confidence_threshold: threshold,
            ..Default::default()
        };
        let mut id = 0u64;
        let rc = unsafe { amv_infer_image_async(detector as _, &input, &params, &mut id) };
        assert_eq!(rc, 0, "异步 GPU 提交应被接受");
        id
    };

    // —— 自适应比较阈值：output0 原始亲和分最高值×0.6（布局断言在 raw_head 内
    // 逐次执行：GPU 单帧 [1,7,8400] / 批拆分后逐项同形）。
    let (_, probe) = gpu_sync("s1_f013", 1.0);
    let top = top_affinity(&probe, 3);
    assert!(top.is_finite() && top > 0.0, "GPU 亲和分通道有限 {top}");
    let threshold = (top * 0.6).max(1e-5);
    eprintln!("[world-gpu] 比较阈值自标定 top={top:.4} threshold={threshold:.4}");

    // —— 基线：逐帧单跑（窗口未启用，走单帧 GPU 直传路径）——
    let singles: Vec<(Vec<BoxHit>, Vec<f32>)> =
        FRAMES.iter().map(|f| gpu_sync(f, threshold)).collect();
    for (frame, (boxes, _)) in FRAMES.iter().zip(&singles) {
        eprintln!(
            "[world-gpu] {frame} 单跑 conf>={threshold:.4} 检出 {} 框",
            boxes.len()
        );
    }

    // —— GPU 直传 vs CPU PNG 对拍（同帧 s1_f013）：亲和分通道 Pearson 相关
    // （跨预处理域判据——同图像同结构；BT.601 往返幅值差不进判据）+ 框级几何
    // 辅证 ——
    let png = std::fs::read(format!("{BATCHD}\\s1_f013.png")).expect("frame png");
    let (cpu_boxes, cpu_raw) = infer_png_sync(ctx, detector, &png, threshold, 3);
    assert_scores_correlate(
        score_channels(&singles[2].1, 3),
        score_channels(&cpu_raw, 3),
        0.90,
        "GPU直传 vs CPU PNG（亲和分通道）",
    );
    assert_boxes_match(&singles[2].0, &cpu_boxes, 2, 0.7, Some(1));

    // —— 攒批窗口：四帧真攒批（formed/batched 统计）vs 逐帧单跑：同域 raw
    // 逐元素收紧对拍（合批仅 GEMM 形状差）+ 框级几何 ——
    let options = am_batching_options_t {
        struct_size: std::mem::size_of::<am_batching_options_t>() as u32,
        max_batch_items: 4,
        // 窗口须覆盖逐次异步提交内的 GPU 接受段开销（纹理注册+预处理，逐项
        // 数十毫秒）：过短会在第四项提交前截止成 3 项批（凑满前提被窗口时限
        // 吃掉），500ms 给足四提交串行余量。
        max_wait_ms: 500,
        flags: 1, // AM_BATCHING_ENABLE
        ..Default::default()
    };
    assert_eq!(
        unsafe { amv_model_configure_batching(detector as _, &options) },
        0,
        "动态批轴档案可启用攒批"
    );
    let ids: Vec<u64> = FRAMES
        .iter()
        .map(|f| submit_gpu_async(f, threshold))
        .collect();
    for (index, (frame, id)) in FRAMES.iter().zip(&ids).enumerate() {
        assert_eq!(wait_terminal(ctx, *id), AM_REQ_DONE, "{frame} 攒批终态");
        let (batched, batched_raw) = fetch_boxes(ctx, *id, 3);
        assert_raw_close(
            &singles[index].1,
            &batched_raw,
            0.02,
            1e-5,
            &format!("攒批 vs 单帧 {frame}"),
        );
        assert_boxes_match(&singles[index].0, &batched, 1, 0.9, None);
    }
    let mut current = am_batching_options_t::default();
    let mut stats = am_batching_stats_t::default();
    assert_eq!(
        unsafe { amv_model_get_batching(detector as _, &mut current, &mut stats) },
        0
    );
    eprintln!(
        "[world-gpu] 攒批统计 formed={} batched={} rejected={}",
        stats.formed_batches, stats.batched_items, stats.rejected_items
    );
    assert!(stats.formed_batches >= 1, "真实 PROFILE 图像攒批成批");
    assert!(stats.batched_items >= 4, "四帧计入成批项");
    assert_eq!(stats.rejected_items, 0, "不应触发准入背压");
    unsafe { amv_model_close(detector as _, 0, std::ptr::null_mut()) };
    unsafe { amv_model_close(tower as _, 0, std::ptr::null_mut()) };
}
