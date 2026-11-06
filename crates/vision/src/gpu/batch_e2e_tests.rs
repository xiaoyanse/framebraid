//! 批 C 设备侧组装端到端门（ignore 显式跑，需真实 NVIDIA GPU）：world.onnx
//! （双输入 ['batch',..] 动态轴，无批元数据 → 原生校验旁路，纯组装正确性）
//! 四帧各异 NV12 → preprocess_to_binding 各自产 [1,3,640,640] 设备张量 →
//! assemble_frames D2D 拼接为 [4,3,640,640] 批绑定 + 词汇张量 batch 维复制
//! → 单次 run_gpu；逐项输出与单帧 run_gpu 对拍 + 批内互异（帧内容可区分）。
use super::cuda_sys::CudaApi;
use super::interop::{nvidia_nv12_texture, RegisteredTexture};
use super::pipeline::{GpuFrameData, GpuPipeline};
use crate::gpu::iobinding::InferenceInput;
use crate::sdk::{initialize, Model, ModelSource, TensorData, TensorInfo};
use std::path::{Path, PathBuf};

const BUDGET: usize = 64 * 1024 * 1024;
const TXT_SEED: u64 = 20261106;
const TXT_SHAPE: [usize; 3] = [1, 2, 512];
const SIZE: u32 = 640;
/// 帧内容标志：四帧取不同常量亮度（UV 中性灰），输出应随之互异。
const Y_LEVELS: [u8; 4] = [60, 100, 150, 200];

/// world.onnx txt_feats [1,2,512]：R1.3/multilane 同构归一化随机嵌入。
fn build_txt_feats() -> TensorData {
    let n = TXT_SHAPE.iter().product::<usize>();
    let mut floats = Vec::with_capacity(n);
    let mut state = TXT_SEED;
    for _ in 0..n {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let u1 = ((state >> 11) as f64 / (1u64 << 53) as f64).max(1e-12);
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let u2 = (state >> 11) as f64 / (1u64 << 53) as f64;
        floats.push(((-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()) as f32);
    }
    let norm = (floats.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / n as f64).sqrt() as f32;
    for v in floats.iter_mut() {
        *v /= norm.max(1e-9);
    }
    let info = TensorInfo {
        name: "txt_feats".into(),
        dtype: 0,
        shape: TXT_SHAPE.iter().map(|d| *d as i64).collect(),
    };
    let raw = unsafe { std::slice::from_raw_parts(floats.as_ptr().cast::<u8>(), floats.len() * 4) };
    TensorData::new(info, raw).expect("txt tensor")
}

/// 嵌入张量 batch 维复制对齐（R1.4 已验证组装语义；GPU 批注入同款）。
fn tile_batch(src: &TensorData, n: usize) -> TensorData {
    let mut shape = src.info.shape.clone();
    shape[0] *= n as i64;
    let info = TensorInfo {
        name: src.info.name.clone(),
        dtype: src.info.dtype,
        shape,
    };
    let bytes = src.bytes();
    let mut tiled = Vec::with_capacity(bytes.len() * n);
    for _ in 0..n {
        tiled.extend_from_slice(bytes);
    }
    TensorData::new(info, &tiled).expect("tile")
}

/// 常量亮度 NV12（Y=level，UV 中性）装载进 D3D 纹理并经管线产设备输入。
fn frame_binding(
    runtime: &std::sync::Arc<CudaApi>,
    pipeline: &GpuPipeline,
    session: &ort::session::Session,
    input_name: &str,
    level: u8,
) -> InferenceInput {
    let (w, h) = (SIZE as usize, SIZE as usize);
    let mut nv12 = vec![level; w * h];
    nv12.extend(vec![128u8; w * h / 2]);
    let texture = unsafe { nvidia_nv12_texture(SIZE, SIZE).expect("NV12 纹理") };
    unsafe {
        let registered = RegisteredTexture::register(runtime.clone(), texture.0).expect("注册");
        let y = registered.map_subresource(0).expect("Y map");
        let uv = registered.map_subresource(1).expect("UV map");
        (runtime.memcpy2d_to_array)(y.array, 0, 0, nv12.as_ptr().cast(), w, w, h, 1);
        let uv_ptr = nv12.as_ptr().add(w * h);
        (runtime.memcpy2d_to_array)(uv.array, 0, 0, uv_ptr.cast(), w, w, h / 2, 1);
    }
    let frame = GpuFrameData {
        width: SIZE,
        height: SIZE,
        d3d_format: 103,
        subresource: 0,
        texture: texture.0,
    };
    pipeline
        .preprocess_to_binding(&frame, session, input_name, SIZE, SIZE)
        .expect("GPU 预处理")
        .0
}

fn floats_of(t: &TensorData) -> &[f32] {
    t.floats().expect("f32 输出")
}

#[test]
#[ignore = "需真实 NVIDIA GPU 与 world.onnx（AM_RT_ORT_GPU_LIBRARY）"]
fn gpu_batch_assembly_matches_single_runs() {
    let repo = "E:/projects/framebraid";
    let gpu_dir = PathBuf::from(repo).join(".local/runtimes/ort-gpu-131-final");
    let ort_dll = gpu_dir.join("onnxruntime.dll");
    initialize(&ort_dll).expect("ORT 初始化");
    let mut model = Model::load(
        ModelSource::Path(Path::new(&format!(
            "{repo}/.local/tests/yolo-world/world.onnx"
        ))),
        None,
        2,
        true,
        4,
    )
    .expect("CUDA 模型加载");
    assert!(model.is_cuda());
    let input_name = model.inputs[0].name.clone();
    let session = model.ort_session();
    let runtime = CudaApi::load().expect("cudart");
    unsafe { (runtime.set_device)(0) };
    let pipeline = GpuPipeline::global().expect("GPU 管线");

    let txt = build_txt_feats();
    // 四帧：各异亮度 → 各自 [1,3,640,640] 设备张量。
    let mut frames = Vec::new();
    for level in Y_LEVELS {
        frames.push(frame_binding(
            &runtime,
            pipeline,
            session,
            &input_name,
            level,
        ));
    }
    let n = frames.len();

    // —— 批路径：设备侧组装 + 嵌入 batch 维复制 + 单次 run_gpu ——
    let frame_refs: Vec<&InferenceInput> = frames.iter().collect();
    let mut batch = pipeline
        .assemble_frames(&frame_refs, session, &input_name, SIZE, SIZE)
        .expect("设备侧组装");
    assert_eq!(batch.batch_len(), n);
    let tiled = tile_batch(&txt, n);
    batch.bind_vocab(&tiled).expect("批词汇绑定");
    batch.binding.synchronize_inputs().expect("vocab 同步");
    let batch_out = model.run_gpu(&batch, BUDGET).expect("批 run_gpu");
    assert_eq!(
        batch_out[0].info.shape.first(),
        Some(&(n as i64)),
        "批输出首轴 {:?}",
        batch_out[0].info.shape
    );

    // —— 单帧路径：同帧同嵌入逐帧 run_gpu（对拍基准；帧张量为只读 D2D 源，
    // 组装后重跑合法）——
    let mut singles = Vec::new();
    for frame in frames.iter_mut() {
        frame.bind_vocab(&txt).expect("单帧词汇绑定");
        frame.binding.synchronize_inputs().expect("vocab 同步");
        singles.push(model.run_gpu(frame, BUDGET).expect("单帧 run_gpu"));
    }

    // —— 对拍：批行 i vs 单帧 i（同输入确定性 ±批 GEMM 形状差）+ 批内互异 ——
    let stride = floats_of(&batch_out[0]).len() / n;
    let batch_rows: Vec<&[f32]> = floats_of(&batch_out[0]).chunks_exact(stride).collect();
    // 诊断矩阵：行 i 对各单帧 j 的最大差（错位/幅度域判断）。
    for (i, row) in batch_rows.iter().enumerate() {
        let mut cols = Vec::with_capacity(n);
        for single in &singles {
            let s = floats_of(&single[0]);
            cols.push(
                row.iter()
                    .zip(s)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0f32, f32::max),
            );
        }
        eprintln!("[gpu-batch] row{i} vs singles = {cols:?}");
    }
    let mut max_diff = 0f32;
    let mut max_scale = 0f32;
    for (i, row) in batch_rows.iter().enumerate() {
        let single = floats_of(&singles[i][0]);
        assert_eq!(single.len(), stride, "单帧/批行元素数一致");
        for (a, b) in row.iter().zip(single) {
            max_diff = max_diff.max((a - b).abs());
            max_scale = max_scale.max(a.abs()).max(b.abs());
        }
    }
    eprintln!("[gpu-batch] per-item max|Δ|={max_diff:.4} (输出幅值 {max_scale:.1})，相对容差判定");
    assert!(
        max_diff < (0.01 * max_scale).max(1.0),
        "批/单帧相对漂移过大: {max_diff} / {max_scale}"
    );
    let mut intra = 0f32;
    for i in 1..n {
        for (a, b) in batch_rows[i].iter().zip(batch_rows[0]) {
            intra = intra.max((a - b).abs());
        }
    }
    eprintln!("[gpu-batch] intra-batch max|Δ| = {intra:.4}");
    assert!(intra > 0.01, "四帧亮度各异，批内输出应互异: {intra}");
}
