//! UV 通道真值门（P1 教训落地，2026-11-20）：slice 各异图案 + 颜色级断言 +
//! 三形态矩阵（单 slice/数组/现场同款 ArraySize=20）+ 尾 slice 必测。
//!
//! P1 根因：CUDA 对 NV12 数组纹理仅暴露 luma 平面（扁平 slice 口径），旧
//! k/k+1 双映射的 "UV" 实为下一 slice 的 luma——测试载体若同图案（旧
//! ArraySize=4 载体四 slice 同内容）两种口径不可区分，伪绿掩盖缺陷。本门
//! 每 slice 装载**互异**的 Y/UV 常量图案：错 slice/伪 UV 立即偏离真值。
//!
//! 两层断言：
//! 1. 快照 BGRA（VP 转换结果）与该 slice 图案的 CPU BT.601 镜像颜色级比对
//!    （常量图案无上采样/插值敏感度，容差覆盖 VP 定点 ±2/255）；
//! 2. kernel 输出（preprocess_to_binding 回读）与同 BGRA 字节的 CPU 双线性
//!    letterbox 镜像逐元素比对（同域同数据，f32 级 1e-4）。
use super::cuda_sys::CudaApi;
use super::interop::nvidia_nv12_texture_ex;
use super::pipeline::{GpuFrameData, GpuPipeline};
use super::vp_transit::upload_nv12_slice;
use crate::sdk::{Model, ModelSource};
use std::path::PathBuf;

const W: usize = 320;
const H: usize = 240;
const DST: usize = 64;

/// slice 各异常量图案（Y/U/V 均随 slice 互异——错源任何形式立即暴露；步长
/// 足够大使相邻 slice 输出差远超互异阈值 2/255）。
fn slice_pattern(slice: u32) -> Vec<u8> {
    let y = 30u8 + (slice as u16 * 9) as u8;
    let u = 60u8 + (slice as u16 * 5) as u8;
    let v = 240u8 - (slice as u16 * 8) as u8;
    let mut nv12 = vec![y; W * H];
    nv12.extend(vec![0u8; W * H / 2]);
    for row in 0..H / 2 {
        for col in 0..W / 2 {
            let i = W * H + row * W + col * 2;
            nv12[i] = u;
            nv12[i + 1] = v;
        }
    }
    nv12
}

/// BT.601 limited f32 镜像（VP 转换的 CPU 镜像）→ 期望 RGB（0..255）。
fn mirror_rgb(nv12: &[u8]) -> [f32; 3] {
    let y = nv12[0] as f32 / 255.0;
    let u = nv12[W * H] as f32 / 255.0 - 0.5;
    let v = nv12[W * H + 1] as f32 / 255.0 - 0.5;
    let c = 1.1643835 * (y - 0.0625);
    [
        (c + 1.5960268 * v).clamp(0.0, 1.0),
        (c - 0.391_762_3 * u - 0.812_968 * v).clamp(0.0, 1.0),
        (c + 2.0172317 * u).clamp(0.0, 1.0),
    ]
}

/// 三形态矩阵：单 slice / 数组 4（SDK 测试形态）/ 现场同款 ArraySize=20
/// （desc=0x208/Misc=0——AM-RT ffmpeg D3D11VA 池同款）。每 slice 各异图案，
/// 首中尾三层取样（尾 slice=旧 k+1 越界必炸点）。
#[test]
#[ignore = "需要真实 NVIDIA GPU（AM_RT_ORT_GPU_LIBRARY/AM_SDK_MODEL 提供 CUDA 会话）"]
fn uv_truth_three_form_matrix() {
    let model_path =
        std::env::var("AM_SDK_MODEL").expect("AM_SDK_MODEL=模型路径（yolo26n-pose.onnx）");
    let ort_dll = std::env::var("AM_RT_ORT_GPU_LIBRARY").expect("AM_RT_ORT_GPU_LIBRARY");
    crate::sdk::initialize(&PathBuf::from(&ort_dll)).expect("ORT 运行库初始化");
    let model = Model::load(
        ModelSource::Path(PathBuf::from(&model_path).as_path()),
        Some("pose-yolo26-e2e-coco17-v1"),
        2, // CUDA
        false,
        4,
    )
    .expect("CUDA 模型加载");
    assert!(model.is_cuda());
    let runtime = CudaApi::load().expect("cudart");
    let pipeline = GpuPipeline::for_device(0).expect("GPU 管线");
    let name = model.inputs[0].name.clone();

    // (形态名, ArraySize, bind, misc)
    let forms: [(&str, u32, u32, u32); 3] = [
        ("单slice", 1, 0x200 | 0x8, 0x2),
        ("数组4", 4, 0x200 | 0x8, 0x2),
        ("现场20", 20, 0x200 | 0x8, 0),
    ];
    let mut outputs: Vec<(String, u32, Vec<f32>)> = Vec::new();
    for (form, array, bind, misc) in forms {
        eprintln!("[uv-truth] === 形态 {form}（array={array}）===");
        let texture = unsafe { nvidia_nv12_texture_ex(W as u32, H as u32, array, bind, misc) }
            .unwrap_or_else(|| panic!("{form} 纹理创建失败"));
        // 全 slice 各异装载（错源即错色）
        for slice in 0..array {
            let pattern = slice_pattern(slice);
            unsafe { upload_nv12_slice(texture.0, slice, &pattern, W as u32, H as u32) }
                .unwrap_or_else(|e| panic!("{form} slice{slice} 装载失败: {e}"));
        }
        // 首中尾取样（含尾 slice——旧实现 k+1 越界点）
        let picks: Vec<u32> = if array == 1 {
            vec![0]
        } else {
            vec![0, array / 2, array - 1]
        };
        for slice in picks {
            let frame = GpuFrameData {
                width: W as u32,
                height: H as u32,
                d3d_format: 103,
                subresource: slice,
                texture: texture.0,
            };
            // 断言一：快照 BGRA 与该 slice 图案 BT.601 镜像颜色级比对
            let snapshot = unsafe { pipeline.snapshot_nv12(&frame) }
                .unwrap_or_else(|e| panic!("{form} slice{slice} 快照失败: {e}"));
            let mut bgra = vec![0u8; W * H * 4];
            runtime
                .check(
                    unsafe {
                        (runtime.memcpy_dtoh)(
                            bgra.as_mut_ptr().cast(),
                            snapshot.bgra_ptr_for_test(),
                            bgra.len(),
                            2,
                        )
                    },
                    "快照回读",
                )
                .unwrap();
            drop(snapshot);
            let expected = mirror_rgb(&slice_pattern(slice));
            for (row, col) in [(0usize, 0usize), (H / 2, W / 2), (H - 1, W - 1)] {
                let i = (row * W + col) * 4;
                let got = [
                    bgra[i + 2] as f32 / 255.0,
                    bgra[i + 1] as f32 / 255.0,
                    bgra[i] as f32 / 255.0,
                ];
                eprintln!(
                    "[uv-truth] {form} slice{slice} px({row},{col}) BGR={:?} want={expected:?}",
                    &bgra[i..i + 4]
                );
                for (channel, want) in got.iter().zip(expected) {
                    assert!(
                        (channel - want).abs() < 2.5 / 255.0,
                        "{form} slice{slice} UV 真值不符: got={channel:.4} want={want:.4}（错 slice/伪 UV 立即暴露）"
                    );
                }
            }
            // 断言二：kernel 输出与同 BGRA 字节的双线性 letterbox 镜像逐元素比对
            let (binding, _transform) = pipeline
                .preprocess_to_binding(
                    &frame,
                    model.ort_session(),
                    &name,
                    DST as u32,
                    DST as u32,
                    false,
                    false,
                )
                .unwrap_or_else(|e| panic!("{form} slice{slice} 预处理失败: {e}"));
            let mut values = vec![0f32; 3 * DST * DST];
            runtime
                .check(
                    unsafe {
                        (runtime.memcpy_dtoh)(
                            values.as_mut_ptr().cast(),
                            binding.device_ptr() as *const std::ffi::c_void,
                            values.len() * 4,
                            2,
                        )
                    },
                    "张量回读",
                )
                .unwrap();
            let inv255 = f32::from_bits(0x3B808081);
            let scale = (DST as f32 / W as f32).min(DST as f32 / H as f32);
            let rw = ((W as f32 * scale).round() as usize).clamp(1, DST);
            let rh = ((H as f32 * scale).round() as usize).clamp(1, DST);
            let left = (DST - rw) / 2;
            let top = (DST - rh) / 2;
            let mut max_error = 0f32;
            for (index, actual) in values.iter().enumerate() {
                let plane = DST * DST;
                let ch = index / plane;
                let rem = index % plane;
                let y = rem / DST;
                let x = rem % DST;
                // pad 带：114/255
                if x < left || x >= left + rw || y < top || y >= top + rh {
                    let want = 114.0f32 / 255.0;
                    max_error = max_error.max((actual - want).abs());
                    continue;
                }
                let dx = x - left;
                let dy = y - top;
                let sx = (dx as f32 + 0.5) * W as f32 / rw as f32 - 0.5;
                let sy = (dy as f32 + 0.5) * H as f32 / rh as f32 - 0.5;
                let fx = sx - sx.floor();
                let fy = sy - sy.floor();
                let x0 = sx.floor().max(0.0) as usize;
                let y0 = sy.floor().max(0.0) as usize;
                let x1 = (x0 + 1).min(W - 1);
                let y1 = (y0 + 1).min(H - 1);
                let byte = [2usize, 1, 0][ch];
                let v00 = bgra[(y0 * W + x0) * 4 + byte] as f32;
                let v01 = bgra[(y0 * W + x1) * 4 + byte] as f32;
                let v10 = bgra[(y1 * W + x0) * 4 + byte] as f32;
                let v11 = bgra[(y1 * W + x1) * 4 + byte] as f32;
                let line_top = v00 + (v01 - v00) * fx;
                let line_bottom = v10 + (v11 - v10) * fx;
                let want = (line_top + (line_bottom - line_top) * fy) * inv255;
                max_error = max_error.max((actual - want).abs());
            }
            eprintln!("[uv-truth] {form} slice{slice}: kernel 镜像 max|Δ|={max_error:.2e}");
            assert!(
                max_error < 1e-4,
                "{form} slice{slice} kernel 输出与镜像差异过大: {max_error}"
            );
            outputs.push((form.to_owned(), slice, values));
        }
    }
    // 批内互异：同形态下不同 slice 输出实质不同（同图案掩盖缺陷的旧载体
    // 反例——若通道取错源，多 slice 输出会雷同或错位）
    for (form, slice, values) in &outputs {
        for (other_form, other_slice, other) in &outputs {
            if form == other_form && slice != other_slice {
                let delta = values
                    .iter()
                    .zip(other)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0f32, f32::max);
                assert!(
                    delta > 2.0 / 255.0,
                    "{form} slice{slice} 与 slice{other_slice} 输出应互异（max|Δ|={delta}）"
                );
            }
        }
    }
}
