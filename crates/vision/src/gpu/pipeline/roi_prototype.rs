use super::*;

#[derive(Clone, Copy)]
struct Roi {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

struct RoiInputs {
    inputs: Vec<InferenceInput>,
    transforms: Vec<crate::sdk::ImageTransform>,
    source_bytes: usize,
    blit_count: usize,
    plane_copies: usize,
}

impl GpuPipeline {
    // The prototype is exercised through the real CUDA and ORT path, independently of C ABI admission.
    // P1 修复起源=快照 BGRA 设备缓冲（VP 转换一次），ROI kernel 自缓冲采样。
    fn roi_bindings(
        &self,
        snapshot: &Nv12Snapshot,
        session: &ort::session::Session,
        name: &str,
        dst: u32,
        rois: &[Roi],
        group: usize,
    ) -> Result<RoiInputs, Error> {
        let (snap_w, snap_h) = snapshot.dimensions();
        if rois.is_empty()
            || rois.len() > 16
            || !(1..=16).contains(&group)
            || !(1..=1024).contains(&dst)
            || snap_w < 2
            || snap_h < 2
            || !snap_w.is_multiple_of(2)
            || !snap_h.is_multiple_of(2)
            || rois.iter().any(|r| {
                r.w == 0
                    || r.h == 0
                    || r.x.checked_add(r.w).is_none_or(|end| end > snap_w)
                    || r.y.checked_add(r.h).is_none_or(|end| end > snap_h)
            })
        {
            return Err(Error("vision.gpuFrameInvalid|ROI prototype".into()));
        }
        self.driver.check(
            unsafe { (self.driver.ctx_set_current)(self.context) },
            "ROI context",
        )?;
        let pitch = snap_w as usize * 4;
        let height = snap_h as usize;
        let bgra_bytes = pitch * height;
        let bgra = PooledBuf::acquire(&self.bufs, &self.runtime, bgra_bytes, "ROI BGRA")?;
        let code = unsafe {
            (self.runtime.memcpy_dtod)(
                bgra.ptr(),
                snapshot.ptr as *const c_void,
                bgra_bytes,
                3, // DtoD
            )
        };
        self.runtime.check(code, "ROI source copy")?;
        let mut inputs = Vec::new();
        for chunk in rois.chunks(group) {
            inputs.push(InferenceInput::new_batch(
                session,
                name,
                chunk.len(),
                dst as usize,
                dst as usize,
                self.ordinal,
            )?);
        }
        let mut blit_count = 0;
        let mut plane_copies = 0;
        let mut source_bytes = 0;
        {
            // 快照缓冲只读（kernel 采样源）；本拷贝隔离池缓冲与快照生命周期。
            blit_count += 1;
            plane_copies += 1;
            source_bytes += bgra_bytes;
        }
        let mut transforms = Vec::with_capacity(rois.len());
        let event = EventLease::acquire(&self.events, &self.driver)?;
        let launch = (|| {
            for (input, chunk) in inputs.iter().zip(rois.chunks(group)) {
                let stride = input.tensor_bytes() / chunk.len();
                for (row, roi) in chunk.iter().enumerate() {
                    let scale = (dst as f32 / roi.w as f32).min(dst as f32 / roi.h as f32);
                    let rw = (roi.w as f32 * scale).round().clamp(1.0, dst as f32) as u32;
                    let rh = (roi.h as f32 * scale).round().clamp(1.0, dst as f32) as u32;
                    self.kernel.launch_region(
                        bgra.ptr() as u64,
                        roi.w,
                        roi.h,
                        dst,
                        dst,
                        rw,
                        rh,
                        input.device_ptr() + (row * stride) as u64,
                        snapshot.width,
                        roi.x,
                        roi.y,
                        1,
                        scale,
                        0,
                        0.0,
                        0.0,
                        1.0,
                    )?;
                    transforms.push(crate::sdk::ImageTransform {
                        width: roi.w,
                        height: roi.h,
                        dst_w: dst,
                        dst_h: dst,
                        scale_x: scale,
                        scale_y: scale,
                        crop_x: 0,
                        crop_y: 0,
                        left: (dst - rw) / 2,
                        top: (dst - rh) / 2,
                    });
                }
            }
            self.driver.check(
                unsafe { (self.driver.event_record)(event.get(), std::ptr::null_mut()) },
                "ROI event record",
            )?;
            self.driver.check(
                unsafe { (self.driver.event_synchronize)(event.get()) },
                "ROI event sync",
            )
        })();
        if let Err(error) = launch {
            // Earlier launches can still be reading the source or writing ORT buffers.
            if unsafe { (self.driver.ctx_synchronize)() } != 0 {
                std::mem::forget((bgra, inputs, event));
            }
            return Err(error);
        }
        Ok(RoiInputs {
            inputs,
            transforms,
            source_bytes,
            blit_count,
            plane_copies,
        })
    }
}
use crate::profiles::{self, Detection, ImageMapping, Options, Output, TensorView, VocabLayout};
use crate::sdk::{cascade::map_box_to_original, Model, ModelSource, TensorData, TensorInfo};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Instant;

const SIZE: u32 = 640;
const BUDGET: usize = 64 * 1024 * 1024;

fn world_text() -> TensorData {
    let tower_path = PathBuf::from(std::env::var_os("FB_ROI_TEXT_MODEL").unwrap());
    let mut tower = Model::load(ModelSource::Path(&tower_path), None, 0, true, 2).unwrap();
    let mut ids = [0i64; 77];
    ids[..3].copy_from_slice(&[49406, 777, 49407]);
    let mut input = tower.inputs[0].clone();
    input.shape = vec![1, 77];
    let bytes: Vec<u8> = ids.into_iter().flat_map(i64::to_le_bytes).collect();
    let tokens = TensorData::new(input, &bytes).unwrap();
    let raw = tower.run_ref(&[&tokens], BUDGET).unwrap();
    let values = raw[0].floats().unwrap();
    assert_eq!(values.len(), 512, "text tower output width");
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    let normalized: Vec<u8> = values
        .iter()
        .flat_map(|x| (x / norm).to_le_bytes())
        .collect();
    TensorData::new(
        TensorInfo {
            name: "txt_feats".into(),
            dtype: 0,
            shape: vec![1, 1, 512],
        },
        &normalized,
    )
    .unwrap()
}

/// 构造条纹 NV12 并经 D3D staging 整 slice 写入纹理（P1 修复后 CUDA 不可写
/// NV12 平面——装载走 CopySubresourceRegion 扁平 slice 口径，RELAY 先例）。
fn fill_texture(frame: &GpuFrameData) -> Vec<u8> {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let mut pixels = vec![0u8; w * h * 3 / 2];
    for row in 0..h {
        for col in 0..w {
            let stripe = 16 + (col / 37 * 17 + row / 47 * 31) % 220;
            pixels[row * w + col] = stripe as u8;
        }
    }
    for row in 0..h / 2 {
        for col in 0..w / 2 {
            let offset = w * h + row * w + 2 * col;
            pixels[offset] = (96 + (row + col * 3) % 64) as u8;
            pixels[offset + 1] = (96 + (row * 7 + col) % 64) as u8;
        }
    }
    unsafe {
        super::super::vp_transit::upload_nv12_slice(
            frame.texture,
            frame.subresource,
            &pixels,
            frame.width,
            frame.height,
        )
    }
    .expect("D3D staging 装载");
    pixels
}

/// 快照 BGRA 读回（host 视图，用于 CPU 镜像参考输入）。
fn readback_bgra(pipeline: &GpuPipeline, snapshot: &Nv12Snapshot) -> Vec<u8> {
    let bytes = snapshot.width as usize * snapshot.height as usize * 4;
    let mut out = vec![0u8; bytes];
    pipeline
        .runtime
        .check(
            unsafe {
                (pipeline.runtime.memcpy_dtoh)(
                    out.as_mut_ptr().cast(),
                    snapshot.ptr as *const c_void,
                    bytes,
                    2, // DtoH
                )
            },
            "snapshot readback",
        )
        .unwrap();
    out
}

fn check_tensor(
    pipeline: &GpuPipeline,
    input: &InferenceInput,
    roi: Roi,
    bgra: &[u8],
    width: usize,
) -> f32 {
    // 640×640 identity-size crop at odd origins. RGB-domain bilinear parity on the
    // SAME BGRA bytes the kernel sampled (P1 修复后输入即 VP 转换结果，kernel 与
    // 本镜像同域同数据，容差回到 f32 单操作量级）。
    let side = SIZE as usize;
    let n = side * side;
    let mut values = vec![0.0f32; 3 * n];
    pipeline
        .runtime
        .check(
            unsafe {
                (pipeline.runtime.memcpy_dtoh)(
                    values.as_mut_ptr().cast(),
                    input.device_ptr() as *const c_void,
                    n * 12,
                    2, // DtoH
                )
            },
            "ROI tensor test readback",
        )
        .unwrap();
    let inv255 = f32::from_bits(0x3B808081);
    let channel = |px: usize, py: usize, c: usize| -> f32 {
        // BGR 内存序：B=+0 G=+1 R=+2；c: 0=R 1=G 2=B
        let byte = [2usize, 1, 0][c];
        bgra[(py * width + px) * 4 + byte] as f32 * inv255
    };
    let mut error = 0.0f32;
    for y in 0..side {
        for x in 0..side {
            let sx = (x as f32 + 0.5) * roi.w as f32 / side as f32 - 0.5 + roi.x as f32;
            let sy = (y as f32 + 0.5) * roi.h as f32 / side as f32 - 0.5 + roi.y as f32;
            let raw_x = sx.floor() as i32;
            let raw_y = sy.floor() as i32;
            let fx = sx - raw_x as f32;
            let fy = sy - raw_y as f32;
            let x0 = raw_x.clamp(0, roi.x as i32 + roi.w as i32 - 1) as usize;
            let y0 = raw_y.clamp(0, roi.y as i32 + roi.h as i32 - 1) as usize;
            let x1 = (raw_x + 1).clamp(0, roi.x as i32 + roi.w as i32 - 1) as usize;
            let y1 = (raw_y + 1).clamp(0, roi.y as i32 + roi.h as i32 - 1) as usize;
            let index = y * side + x;
            for c in 0..3 {
                let v00 = channel(x0, y0, c);
                let v01 = channel(x1, y0, c);
                let v10 = channel(x0, y1, c);
                let v11 = channel(x1, y1, c);
                let top = v00 + (v01 - v00) * fx;
                let bottom = v10 + (v11 - v10) * fx;
                let expected = top + (bottom - top) * fy;
                let actual = values[c * n + index];
                error = error.max((actual - expected).abs());
                assert!(actual.is_finite());
            }
        }
    }
    assert!(error < 3e-4, "BGRA parity max error {error}");
    error
}

fn detect(
    raw: &TensorData,
    index: usize,
    count: usize,
    tr: crate::sdk::ImageTransform,
) -> Vec<Detection> {
    let shape = [1, raw.info.shape[1] as usize, raw.info.shape[2] as usize];
    let elements = shape[1] * shape[2];
    let values = raw.floats().unwrap();
    assert_eq!(values.len(), count * elements);
    let view = TensorView {
        name: &raw.info.name,
        shape: &shape,
        values: &values[index * elements..(index + 1) * elements],
    };
    let mapping = ImageMapping {
        width: tr.width,
        height: tr.height,
        scale: tr.scale_x,
        left: tr.left,
        top: tr.top,
    };
    let opts = Options {
        confidence: 0.0,
        max_detections: 1,
        ..Default::default()
    };
    let layout = VocabLayout {
        entries: vec![("fixture".into(), 1)],
    };
    let result = profiles::decode(
        "detect-yolo-world-batch-v1",
        &[view],
        mapping,
        opts,
        Some(&layout),
    )
    .unwrap();
    let Output::Detect(boxes) = result.output else {
        panic!("not a detection")
    };
    assert_eq!(boxes.len(), 1, "fixture must exercise coordinate mapping");
    boxes
}

#[derive(Clone)]
struct Run {
    boxes: Vec<Detection>,
    prep_ms: f64,
    run_ms: f64,
    decode_ms: f64,
    used_delta: i64,
    live_bytes: usize,
    blit_count: usize,
    plane_copies: usize,
    source_bytes: usize,
}

type MemInfo = unsafe extern "C" fn(*mut usize, *mut usize) -> i32;

fn free_bytes(pipeline: &GpuPipeline, mem_info: MemInfo) -> usize {
    let mut free = 0;
    let mut total = 0;
    pipeline
        .runtime
        .check(unsafe { mem_info(&mut free, &mut total) }, "cudaMemGetInfo")
        .unwrap();
    free
}

#[allow(clippy::too_many_arguments)]
fn measure(
    pipeline: &GpuPipeline,
    model: &mut Model,
    snapshot: &Nv12Snapshot,
    rois: &[Roi],
    group: usize,
    text: &TensorData,
    info: MemInfo,
    baseline: usize,
) -> Run {
    let name = model.inputs[0].name.clone();
    let begin = Instant::now();
    let prepared = if group == 0 {
        let mut all = RoiInputs {
            inputs: Vec::new(),
            transforms: Vec::new(),
            source_bytes: 0,
            blit_count: 0,
            plane_copies: 0,
        };
        for roi in rois {
            let one = pipeline
                .roi_bindings(
                    snapshot,
                    model.ort_session(),
                    &name,
                    SIZE,
                    std::slice::from_ref(roi),
                    1,
                )
                .unwrap();
            all.inputs.extend(one.inputs);
            all.transforms.extend(one.transforms);
            all.source_bytes += one.source_bytes;
            all.blit_count += one.blit_count;
            all.plane_copies += one.plane_copies;
        }
        all
    } else {
        pipeline
            .roi_bindings(snapshot, model.ort_session(), &name, SIZE, rois, group)
            .unwrap()
    };
    let prep_ms = begin.elapsed().as_secs_f64() * 1000.0;
    let expected_copies = if group == 0 { rois.len() } else { 1 };
    assert_eq!(prepared.blit_count, expected_copies);
    assert_eq!(prepared.plane_copies, expected_copies);
    assert_eq!(
        prepared.source_bytes,
        expected_copies * snapshot.width as usize * snapshot.height as usize * 4
    );
    let live_bytes = prepared
        .inputs
        .iter()
        .map(InferenceInput::tensor_bytes)
        .sum();
    let mut least_free = free_bytes(pipeline, info);
    let mut boxes = Vec::new();
    let mut run_ms = 0.0;
    let mut decode_ms = 0.0;
    for (chunk, mut input) in prepared.inputs.into_iter().enumerate() {
        let count = input.batch_len();
        let start = Instant::now();
        let mut text_info = text.info.clone();
        text_info.shape[0] = count as i64;
        let tiled = TensorData::new(text_info, &text.bytes().repeat(count)).unwrap();
        input.bind_vocab(&tiled).unwrap();
        input.binding.synchronize_inputs().unwrap();
        let raw = model.run_gpu(&input, BUDGET).unwrap();
        run_ms += start.elapsed().as_secs_f64() * 1000.0;
        least_free = least_free.min(free_bytes(pipeline, info));
        let start = Instant::now();
        assert_eq!(raw[0].info.shape[0], count as i64);
        for row in 0..count {
            let index = chunk * group.max(1) + row;
            let roi = rois[index];
            let [w, h] = [snapshot.width as f32, snapshot.height as f32];
            let normalized = [
                roi.x as f32 / w,
                roi.y as f32 / h,
                (roi.x + roi.w) as f32 / w,
                (roi.y + roi.h) as f32 / h,
            ];
            let mut result = detect(&raw[0], row, count, prepared.transforms[index]).remove(0);
            let mapped = map_box_to_original(result.bounds, normalized);
            for axis in 0..4 {
                let (offset, span, dimension) = if axis % 2 == 0 {
                    (roi.x, roi.w, w)
                } else {
                    (roi.y, roi.h, h)
                };
                let reference = (offset as f64 + f64::from(result.bounds[axis]) * span as f64)
                    / f64::from(dimension);
                assert!((reference - f64::from(mapped[axis])).abs() * f64::from(dimension) < 0.001);
                assert!(
                    mapped[axis] >= normalized[axis % 2] - 1e-6
                        && mapped[axis] <= normalized[axis % 2 + 2] + 1e-6
                );
            }
            result.bounds = mapped;
            boxes.push(result);
        }
        decode_ms += start.elapsed().as_secs_f64() * 1000.0;
    }
    Run {
        boxes,
        prep_ms,
        run_ms,
        decode_ms,
        used_delta: baseline as i64 - least_free as i64,
        live_bytes,
        blit_count: prepared.blit_count,
        plane_copies: prepared.plane_copies,
        source_bytes: prepared.source_bytes,
    }
}

fn row(run: &Run) -> Value {
    json!({"preprocessMs":run.prep_ms,"inferenceMs":run.run_ms,"decodeMs":run.decode_ms,
           "sampledDeviceDeltaBytes":run.used_delta,"liveInputTensorBytes":run.live_bytes,"blitCount":run.blit_count,
           "planeCopies":run.plane_copies,"sourceBytes":run.source_bytes,"boxes":run.boxes})
}

#[test]
#[ignore = "RTX/D3D11, CUDA ORT and YOLO-World detector/text tower required"]
fn real_detector_roi_prototype() {
    let ort_dir = PathBuf::from(std::env::var_os("AM_SDK_ORT_DIR").unwrap());
    crate::sdk::initialize(&ort_dir.join("onnxruntime.dll")).unwrap();
    let text = world_text();
    let detector = PathBuf::from(std::env::var_os("FB_ROI_MODEL").unwrap());
    let mut model = Model::load(ModelSource::Path(&detector), None, 2, true, 4).unwrap();
    assert!(model.is_cuda());
    let pipeline = GpuPipeline::for_device(0).unwrap();
    let runtime = unsafe { libloading::Library::new(ort_dir.join("cudart64_13.dll")) }.unwrap();
    let mem_info = unsafe { *runtime.get::<MemInfo>(b"cudaMemGetInfo\0").unwrap() };
    let texture = unsafe { super::super::interop::nvidia_nv12_texture(1920, 1080) }.unwrap();
    let frame = GpuFrameData {
        width: 1920,
        height: 1080,
        d3d_format: 103,
        subresource: 0,
        texture: texture.0,
    };
    let baseline = free_bytes(pipeline, mem_info);
    let _pixels = fill_texture(&frame);
    let snapshot = unsafe { pipeline.snapshot_nv12(&frame) }.unwrap();
    let bgra = readback_bgra(pipeline, &snapshot);
    let probe = Roi {
        x: 313,
        y: 117,
        w: SIZE,
        h: SIZE,
    };
    let name = model.inputs[0].name.clone();
    let parity = pipeline
        .roi_bindings(&snapshot, model.ort_session(), &name, SIZE, &[probe], 1)
        .unwrap();
    let pixel_error = check_tensor(pipeline, &parity.inputs[0], probe, &bgra, 1920);
    drop(parity);
    let rois: Vec<_> = (0..16)
        .map(|i| Roi {
            x: 11 + (i % 4) * 381,
            y: 17 + (i / 4) * 171,
            w: 390 + i * 13,
            h: 307 + i * 11,
        })
        .collect();
    let group = match std::env::var("FB_ROI_GROUP").as_deref() {
        Ok("repeat") => 0,
        Ok("1") => 1,
        Ok("4") => 4,
        _ => panic!("FB_ROI_GROUP must be repeat, 1 or 4"),
    };
    let mut rows = Vec::new();
    for count in [1, 4, 8, 16] {
        for _ in 0..2 {
            measure(
                pipeline,
                &mut model,
                &snapshot,
                &rois[..count],
                group,
                &text,
                mem_info,
                baseline,
            );
        }
        let mut samples = Vec::new();
        for _ in 0..7 {
            samples.push(row(&measure(
                pipeline,
                &mut model,
                &snapshot,
                &rois[..count],
                group,
                &text,
                mem_info,
                baseline,
            )));
        }
        rows.push(json!({"roiCount":count,"group":group,"samples":samples}));
    }
    println!(
        "ROI_REPORT {}",
        json!({"width":frame.width,"height":frame.height,
             "rgbMaxError":pixel_error,
             "baselineFreeBytes":baseline,"rows":rows})
    );
}
