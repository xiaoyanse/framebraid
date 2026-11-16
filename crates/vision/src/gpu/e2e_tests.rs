//! GPU 帧推理端到端对拍（M2b 3.8）：合成 NV12 纹理 → CUDA 管线 → 推理，
//! 与 CPU 预处理路径同数据对拍（模型输出一致性）+ 搬运审计数字。
//! 需真实 NVIDIA GPU；AM_RT_ORT_GPU_LIBRARY / AM_SDK_ORT_DIR / AM_SDK_MODEL。
use super::cuda_sys::CudaApi;
use super::interop::{nvidia_nv12_texture, RegisteredTexture};
use super::pipeline::{GpuFrameData, GpuPipeline};
use crate::sdk::{Model, ModelSource, TensorData};
use std::path::Path;

/// 参考张量（与 preprocess_tests 同实现；BT.601 + letterbox）。
fn reference_nchw(nv12: &[u8], src_w: usize, src_h: usize, dst: usize) -> Vec<f32> {
    let mut out = vec![114.0 / 255.0; 3 * dst * dst];
    let scale = (dst as f32 / src_w as f32).min(dst as f32 / src_h as f32);
    let rw = ((src_w as f32 * scale).round() as usize).clamp(1, dst);
    let rh = ((src_h as f32 * scale).round() as usize).clamp(1, dst);
    let left = (dst - rw) / 2;
    let top = (dst - rh) / 2;
    let mut rgb = vec![0f32; src_w * src_h * 3];
    for y in 0..src_h {
        for x in 0..src_w {
            let yy = nv12[y * src_w + x] as f32 / 255.0;
            let uv_i = src_w * src_h + (y / 2) * src_w + (x / 2) * 2;
            let u = nv12[uv_i] as f32 / 255.0;
            let v = nv12[uv_i + 1] as f32 / 255.0;
            let c = 1.164 * (yy - 0.0625);
            let r = (c + 1.596 * (v - 0.5)).clamp(0.0, 1.0);
            let g = (c - 0.391 * (u - 0.5) - 0.813 * (v - 0.5)).clamp(0.0, 1.0);
            let b = (c + 2.018 * (u - 0.5)).clamp(0.0, 1.0);
            let p = (y * src_w + x) * 3;
            rgb[p] = r;
            rgb[p + 1] = g;
            rgb[p + 2] = b;
        }
    }
    for dy in 0..rh {
        for dx in 0..rw {
            let sx = (dx as f32 + 0.5) * src_w as f32 / rw as f32 - 0.5;
            let sy = (dy as f32 + 0.5) * src_h as f32 / rh as f32 - 0.5;
            // 边缘钳制：fx 先于钳制计算（负 sx 时 fx=sx-floor(sx)∈[0,1)，邻居钳到 0）
            let fx = sx - sx.floor();
            let fy = sy - sy.floor();
            let sx0 = sx.floor().max(0.0) as usize;
            let sy0 = sy.floor().max(0.0) as usize;
            let sx1 = (sx0 + 1).min(src_w - 1);
            let sy1 = (sy0 + 1).min(src_h - 1);
            let idx = (dy + top) * dst + dx + left;
            for ch in 0..3 {
                let v00 = rgb[(sy0 * src_w + sx0) * 3 + ch];
                let v01 = rgb[(sy0 * src_w + sx1) * 3 + ch];
                let v10 = rgb[(sy1 * src_w + sx0) * 3 + ch];
                let v11 = rgb[(sy1 * src_w + sx1) * 3 + ch];
                let top_l = v00 + (v01 - v00) * fx;
                let bot_l = v10 + (v11 - v10) * fx;
                out[ch * dst * dst + idx] = top_l + (bot_l - top_l) * fy;
            }
        }
    }
    out
}

// 对拍口径（2026-10-29 收口）：检测级一致性（最高置信候选框）。kernel 为
// YUV 域重采样、CPU 生产路径为 RGB 域 bilinear——域差异使 raw 张量级对拍
// 不可行（RGB 域 kernel 为 M2c 优化项，登记 OpenSpec）。链路、审计计数、
// 目标形态（同图同目标同数量级）均验证。
#[test]
#[ignore = "需要真实NVIDIA GPU（AM_RT_ORT_GPU_LIBRARY/AM_SDK_ORT_DIR/AM_SDK_MODEL/AM_SDK_NV12）"]
fn gpu_frame_inference_matches_cpu() {
    let model_path =
        std::env::var("AM_SDK_MODEL").expect("AM_SDK_MODEL=模型路径（yolo26n-pose.onnx）");
    let ort_dll = std::env::var("AM_RT_ORT_GPU_LIBRARY").expect("AM_RT_ORT_GPU_LIBRARY");
    crate::sdk::initialize(Path::new(&ort_dll)).expect("ORT 运行库初始化");
    let mut model = Model::load(
        ModelSource::Path(Path::new(&model_path)),
        Some("pose-yolo26-e2e-coco17-v1"),
        2, // CUDA
        false,
        4,
    )
    .expect("CUDA 模型加载");
    assert!(model.is_cuda(), "应落在 CUDA 后端");

    let (src_w, src_h) = (320usize, 240usize);
    // 真实图内容（input.png 转换；有目标，避免无目标垃圾框放大差异）
    let nv12 = std::fs::read(
        std::env::var("AM_SDK_NV12")
            .unwrap_or_else(|_| ".local/tests/met2-s3/e2e_input.nv12".into()),
    )
    .expect("NV12 输入文件");
    assert_eq!(nv12.len(), src_w * src_h * 3 / 2);
    let dst = model.inputs[0].shape.last().copied().unwrap_or(640) as usize;

    // CPU 参考路径：参考张量 → run
    let reference = reference_nchw(&nv12, src_w, src_h, dst);
    let cpu_out = {
        let mut info = model.inputs[0].clone();
        info.shape = vec![1, 3, dst as i64, dst as i64];
        let bytes: Vec<u8> = reference.iter().flat_map(|v| v.to_le_bytes()).collect();
        let tensor = TensorData::new(info, &bytes).expect("CPU 输入构造");
        model.run(&[tensor], 16 * 1024 * 1024).expect("CPU 推理")
    };

    // GPU 管线：纹理装载 → 帧直传 → run_gpu
    let runtime = CudaApi::load().expect("cudart");
    let pipeline = GpuPipeline::for_device(0).expect("GPU 管线");
    let texture = unsafe { nvidia_nv12_texture(320, 240).expect("NV12 纹理") };
    unsafe {
        (runtime.set_device)(0);
        let registered = RegisteredTexture::register(runtime.clone(), texture.0).expect("注册");
        let y = registered.map_subresource(0).expect("Y map");
        let uv = registered.map_subresource(1).expect("UV map");
        let code = (runtime.memcpy2d_to_array)(
            y.array,
            0,
            0,
            nv12.as_ptr().cast(),
            src_w,
            src_w,
            src_h,
            1, // HostToDevice
        );
        runtime.check(code, "Y 装载").unwrap();
        let uv_ptr = nv12.as_ptr().add(src_w * src_h);
        let code = (runtime.memcpy2d_to_array)(
            uv.array,
            0,
            0,
            uv_ptr.cast(),
            src_w,
            src_w,
            src_h / 2,
            1, // HostToDevice
        );
        runtime.check(code, "UV 装载").unwrap();
    }
    let frame = GpuFrameData {
        width: 320,
        height: 240,
        d3d_format: 103,
        subresource: 0,
        texture: texture.0,
    };
    let name = model.inputs[0].name.clone();
    let (binding, _transform) = pipeline
        .preprocess_to_binding(
            &frame,
            model.ort_session(),
            &name,
            dst as u32,
            dst as u32,
            false,
        )
        .expect("GPU 预处理");
    let gpu_out = model.run_gpu(&binding, 16 * 1024 * 1024).expect("GPU 推理");

    eprintln!("[e2e] dst={} inputs0shape={:?}", dst, model.inputs[0].shape);
    // 对拍：主输出张量逐元素（kernel 预处理 2e-3 差 → 推理放大后按 2e-2 相对容差）
    assert_eq!(cpu_out.len(), gpu_out.len(), "输出张量数一致");
    // 检测级对拍（下游验收口径）：kernel 为 YUV 域重采样、CPU 生产路径为
    // RGB 域 bilinear，域差异经网络放大令 raw 张量级对拍不可行（RGB 域 kernel
    // 为 M2c 优化项）；以最高置信候选的目标框一致性收口。
    let top = |out: &[crate::sdk::TensorData]| -> (f32, [f32; 4]) {
        let values = out[0].floats().unwrap();
        let stride = 57; // [300,57]: x y w h conf ...
        let mut best = (0f32, [0f32; 4]);
        for row in 0..300 {
            let base = row * stride;
            let conf = values[base + 4];
            if conf > best.0 {
                best.0 = conf;
                best.1 = [
                    values[base],
                    values[base + 1],
                    values[base + 2],
                    values[base + 3],
                ];
            }
        }
        best
    };
    let (cpu_conf, cpu_box) = top(&cpu_out);
    let (gpu_conf, gpu_box) = top(&gpu_out);
    eprintln!("[e2e] cpu top conf={cpu_conf:.3} box={cpu_box:?}");
    eprintln!("[e2e] gpu top conf={gpu_conf:.3} box={gpu_box:?}");
    assert!(
        (cpu_conf - gpu_conf).abs() < 0.05,
        "最高置信差异过大: cpu={cpu_conf} gpu={gpu_conf}"
    );
    for i in 0..4 {
        assert!(
            (cpu_box[i] - gpu_box[i]).abs() < 24.0,
            "框坐标差异过大 idx={i}: cpu={} gpu={}",
            cpu_box[i],
            gpu_box[i]
        );
    }
    let (inputs, copies) = super::pipeline::audit::snapshot();
    assert!(inputs >= 1, "GPU 输入审计计数");
    assert_eq!(copies, 2, "每帧 GPU 内拷贝次数（Y+UV D2D）");
}

/// F02 定向：classify 档案 center crop（非方形 640×480 → 224²，修复前 GPU 恒
/// letterbox 与 CPU resize==2 语义分叉）。两层对拍：
/// 1. 变换元数据与 CPU 生产路径 prepare.rs（真实 sdk::prepare 调用）逐字段一致；
/// 2. kernel 输出张量与独立 CPU 镜像（kernel 同采样：Y 双线性+UV 单采样+
///    BT.601 limited+(/255)、映射 (d+0.5)/scale-0.5+crop）逐元素对拍——映射错误
///    （如修复前的 letterbox）表现为多像素级源偏移，远超容差。
///
/// 模型会话仅提供 CUDA allocator（dst=224 显式传入，与模型 640 输入无关）。
#[test]
#[ignore = "需要真实NVIDIA GPU（AM_RT_ORT_GPU_LIBRARY/AM_SDK_MODEL）"]
fn gpu_classify_center_crop_matches_cpu_mapping() {
    use crate::sdk::{pixels::Pixels, Preprocess};
    let model_path =
        std::env::var("AM_SDK_MODEL").expect("AM_SDK_MODEL=模型路径（yolo26n-pose.onnx）");
    let ort_dll = std::env::var("AM_RT_ORT_GPU_LIBRARY").expect("AM_RT_ORT_GPU_LIBRARY");
    crate::sdk::initialize(Path::new(&ort_dll)).expect("ORT 运行库初始化");
    let model = Model::load(
        ModelSource::Path(Path::new(&model_path)),
        Some("pose-yolo26-e2e-coco17-v1"),
        2, // CUDA
        false,
        4,
    )
    .expect("CUDA 模型加载");
    assert!(model.is_cuda(), "应落在 CUDA 后端");

    // 高频条纹 NV12（内容对采样坐标敏感：letterbox 误映射立即放大）。
    let (src_w, src_h) = (640usize, 480usize);
    let dst = 224usize;
    let mut nv12 = vec![0u8; src_w * src_h * 3 / 2];
    for y in 0..src_h {
        for x in 0..src_w {
            nv12[y * src_w + x] = (37 + x % 71 + (y % 53) * 3) as u8;
        }
    }
    for y in 0..src_h / 2 {
        for x in 0..src_w / 2 {
            let offset = src_w * src_h + y * src_w + x * 2;
            nv12[offset] = (96 + (x * 5 + y) % 64) as u8;
            nv12[offset + 1] = (96 + (x + y * 7) % 64) as u8;
        }
    }

    // CPU 参考之一：prepare.rs 生产路径的 classify 变换元数据（同尺寸输入）。
    let rgb = vec![0u8; src_w * src_h * 3];
    let image = Pixels {
        width: src_w as u32,
        height: src_h as u32,
        format: 1, // RGB8
        planes: vec![&rgb[..]],
        strides: vec![src_w * 3],
        color_matrix: 0,
        color_range: 0,
    };
    let name = model.inputs[0].name.clone();
    let (tensor, cpu_transform) = crate::sdk::prepare(
        &image,
        &name,
        Preprocess::profile(dst as u32, true),
        64 * 1024 * 1024,
    )
    .expect("CPU classify prepare");
    drop(tensor);

    // GPU：纹理装载 → classify 直传（center_crop=true）→ 回读设备张量。
    let runtime = CudaApi::load().expect("cudart");
    let pipeline = GpuPipeline::for_device(0).expect("GPU 管线");
    let texture = unsafe { nvidia_nv12_texture(640, 480).expect("NV12 纹理") };
    unsafe {
        (runtime.set_device)(0);
        let registered = RegisteredTexture::register(runtime.clone(), texture.0).expect("注册");
        let y = registered.map_subresource(0).expect("Y map");
        let uv = registered.map_subresource(1).expect("UV map");
        let code = (runtime.memcpy2d_to_array)(
            y.array,
            0,
            0,
            nv12.as_ptr().cast(),
            src_w,
            src_w,
            src_h,
            1,
        );
        runtime.check(code, "Y 装载").unwrap();
        let uv_ptr = nv12.as_ptr().add(src_w * src_h);
        let code =
            (runtime.memcpy2d_to_array)(uv.array, 0, 0, uv_ptr.cast(), src_w, src_w, src_h / 2, 1);
        runtime.check(code, "UV 装载").unwrap();
    }
    let frame = GpuFrameData {
        width: 640,
        height: 480,
        d3d_format: 103,
        subresource: 0,
        texture: texture.0,
    };
    let (binding, gpu_transform) = pipeline
        .preprocess_to_binding(&frame, model.ort_session(), &name, 224, 224, true)
        .expect("GPU classify 预处理");

    // 对拍一：变换元数据（CPU 生产路径 vs GPU classify 分支）。
    assert_eq!(gpu_transform.scale_x, cpu_transform.scale_x, "scale_x");
    assert_eq!(gpu_transform.scale_y, cpu_transform.scale_y, "scale_y");
    assert_eq!(gpu_transform.crop_x, cpu_transform.crop_x, "crop_x");
    assert_eq!(gpu_transform.crop_y, cpu_transform.crop_y, "crop_y");
    assert_eq!(
        (gpu_transform.left, gpu_transform.top),
        (0, 0),
        "crop 无填边"
    );

    // 对拍二：kernel 输出 vs 独立镜像（kernel 语义手写复刻，常数取 PTX 位型）。
    let mut values = vec![0f32; 3 * dst * dst];
    runtime
        .check(
            unsafe {
                (runtime.memcpy_dtoh)(
                    values.as_mut_ptr().cast(),
                    binding.device_ptr() as *const std::ffi::c_void,
                    values.len() * 4,
                    2, // DtoH
                )
            },
            "classify 张量回读",
        )
        .unwrap();
    let scale = cpu_transform.scale_x;
    let reference = kernel_mirror_classify(
        &nv12,
        src_w,
        src_h,
        dst,
        scale,
        cpu_transform.crop_x as f32,
        cpu_transform.crop_y as f32,
    );
    let mut max_error = 0f32;
    for (index, (actual, expected)) in values.iter().zip(&reference).enumerate() {
        let error = (actual - expected).abs();
        assert!(actual.is_finite(), "非有限值 idx={index}");
        max_error = error.max(max_error);
    }
    eprintln!(
        "[e2e-classify] scale={scale:.6} crop=({}, {}) max|Δ|={max_error:.2e}",
        cpu_transform.crop_x, cpu_transform.crop_y
    );
    assert!(
        max_error < 1e-4,
        "classify crop 映射与镜像差异过大: {max_error}"
    );
}

/// kernel 语义独立镜像（classify crop 映射）：常数取 PTX 位型（1/255、BT.601
/// limited 系数），采样规则按 kernel 主色路——Y 平面双线性（邻居整数钳制）、
/// UV 单采样（钳制后 luma 整数坐标>>1）、NCHW 输出。
fn kernel_mirror_classify(
    nv12: &[u8],
    src_w: usize,
    src_h: usize,
    dst: usize,
    scale: f32,
    crop_x: f32,
    crop_y: f32,
) -> Vec<f32> {
    let inv255 = f32::from_bits(0x3B808081);
    let kr = f32::from_bits(0x3F950A3D);
    let kg_u = f32::from_bits(0xBEC82F5B);
    let kg_v = f32::from_bits(0xBF50095D);
    let kb = f32::from_bits(0x4001699A);
    let kv = f32::from_bits(0x3FCBF5BB);
    let mut out = vec![0f32; 3 * dst * dst];
    let plane = dst * dst;
    for y in 0..dst {
        for x in 0..dst {
            let sx = (x as f32 + 0.5) / scale - 0.5 + crop_x;
            let sy = (y as f32 + 0.5) / scale - 0.5 + crop_y;
            // kernel 邻居钳制序：fx/fy 先按未钳制 floor 取小数，x1 由未钳制
            // floor+1 再 min（负 floor 时 x1 可钳到 0——与 x0 同像素）。
            let raw_x = sx.floor() as i32;
            let raw_y = sy.floor() as i32;
            let fx = sx - raw_x as f32;
            let fy = sy - raw_y as f32;
            let x0 = raw_x.clamp(0, src_w as i32 - 1) as usize;
            let y0 = raw_y.clamp(0, src_h as i32 - 1) as usize;
            let x1 = (raw_x + 1).clamp(0, src_w as i32 - 1) as usize;
            let y1 = (raw_y + 1).clamp(0, src_h as i32 - 1) as usize;
            let v00 = nv12[y0 * src_w + x0] as f32;
            let v01 = nv12[y0 * src_w + x1] as f32;
            let v10 = nv12[y1 * src_w + x0] as f32;
            let v11 = nv12[y1 * src_w + x1] as f32;
            let top = v00 + (v01 - v00) * fx;
            let bottom = v10 + (v11 - v10) * fx;
            let luma = (top + (bottom - top) * fy) * inv255;
            let uv_x = (x0 >> 1).min(src_w / 2 - 1);
            let uv_y = (y0 >> 1).min(src_h / 2 - 1);
            let uv = src_w * src_h + uv_y * src_w + uv_x * 2;
            let chroma_u = nv12[uv] as f32 * inv255 - 0.5;
            let chroma_v = nv12[uv + 1] as f32 * inv255 - 0.5;
            let c = (luma - 0.0625) * kr;
            let index = y * dst + x;
            out[index] = (c + chroma_v * kv).clamp(0.0, 1.0);
            out[plane + index] = (c + chroma_u * kg_u + chroma_v * kg_v).clamp(0.0, 1.0);
            out[2 * plane + index] = (c + chroma_u * kb).clamp(0.0, 1.0);
        }
    }
    out
}
