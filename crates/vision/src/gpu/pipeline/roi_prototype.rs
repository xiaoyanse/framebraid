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
    map_count: usize,
    plane_copies: usize,
}

impl GpuPipeline {
    // The prototype is exercised through the real CUDA and ORT path, independently of C ABI admission.
    fn roi_bindings(
        &self,
        frame: &GpuFrameData,
        session: &ort::session::Session,
        name: &str,
        dst: u32,
        rois: &[Roi],
        group: usize,
    ) -> Result<RoiInputs, Error> {
        if rois.is_empty()
            || rois.len() > 16
            || !(1..=16).contains(&group)
            || !(1..=1024).contains(&dst)
            || frame.texture.is_null()
            || frame.d3d_format != 103
            || !(2..=8192).contains(&frame.width)
            || !(2..=8192).contains(&frame.height)
            || !frame.width.is_multiple_of(2)
            || !frame.height.is_multiple_of(2)
            || frame.subresource == u32::MAX
            || rois.iter().any(|r| {
                r.w == 0
                    || r.h == 0
                    || r.x.checked_add(r.w).is_none_or(|end| end > frame.width)
                    || r.y.checked_add(r.h).is_none_or(|end| end > frame.height)
            })
        {
            return Err(Error("vision.gpuFrameInvalid|ROI prototype".into()));
        }
        self.driver.check(
            unsafe { (self.driver.ctx_set_current)(self.context) },
            "ROI context",
        )?;
        let pitch = frame.width as usize;
        let height = frame.height as usize;
        let y_bytes = pitch * height;
        let uv_bytes = y_bytes / 2;
        let y = PooledBuf::acquire(&self.bufs, &self.runtime, y_bytes, "ROI Y")?;
        let uv = PooledBuf::acquire(&self.bufs, &self.runtime, uv_bytes, "ROI UV")?;
        let mut inputs = Vec::new();
        for chunk in rois.chunks(group) {
            inputs.push(InferenceInput::new_batch(
                session,
                name,
                chunk.len(),
                dst as usize,
                dst as usize,
            )?);
        }
        let mut map_count = 0;
        let mut plane_copies = 0;
        let mut source_bytes = 0;
        {
            let cached = unsafe {
                registry::global()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .acquire(&self.runtime, frame.texture)?
            };
            let _guard = cached.map_lock.lock().unwrap_or_else(|e| e.into_inner());
            // Both guards share one underlying resource mapping.
            let y_plane = cached.texture.map_subresource(frame.subresource)?;
            map_count += 1;
            let uv_plane = cached.texture.map_subresource(frame.subresource + 1)?;
            for (buffer, array, rows) in [
                (y.ptr(), y_plane.array, height),
                (uv.ptr(), uv_plane.array, height / 2),
            ] {
                self.runtime.check(
                    unsafe {
                        (self.runtime.memcpy2d_from_array)(
                            buffer, pitch, array, 0, 0, pitch, rows, 3,
                        )
                    },
                    "ROI source plane copy",
                )?;
                plane_copies += 1;
                source_bytes += pitch * rows;
            }
            // Synchronous plane copies finish before the texture is unmapped.
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
                        y.ptr() as u64,
                        uv.ptr() as u64,
                        roi.w,
                        roi.h,
                        dst,
                        dst,
                        rw,
                        rh,
                        input.device_ptr() + (row * stride) as u64,
                        frame.width,
                        roi.x,
                        roi.y,
                        1,
                        scale,
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
                std::mem::forget((y, uv, inputs, event));
            }
            return Err(error);
        }
        Ok(RoiInputs {
            inputs,
            transforms,
            source_bytes,
            map_count,
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

fn fill_texture(pipeline: &GpuPipeline, frame: &GpuFrameData) -> Vec<u8> {
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
    let texture = unsafe {
        super::super::interop::RegisteredTexture::register(pipeline.runtime.clone(), frame.texture)
    }
    .unwrap();
    let y = texture.map_subresource(0).unwrap();
    let uv = texture.map_subresource(1).unwrap();
    for (array, offset, rows) in [(y.array, 0, h), (uv.array, w * h, h / 2)] {
        pipeline
            .runtime
            .check(
                unsafe {
                    (pipeline.runtime.memcpy2d_to_array)(
                        array,
                        0,
                        0,
                        pixels.as_ptr().add(offset).cast(),
                        w,
                        w,
                        rows,
                        1,
                    )
                },
                "fixture upload",
            )
            .unwrap();
    }
    pixels
}

fn check_tensor(
    pipeline: &GpuPipeline,
    input: &InferenceInput,
    roi: Roi,
    pixels: &[u8],
    width: usize,
    height: usize,
) -> f32 {
    // 640×640 identity-size crop at odd origins. Compare RGB at each pixel against full-frame
    // NV12 addressing, including chroma phase and the first and last ROI rows and columns.
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
                    2,
                )
            },
            "ROI tensor test readback",
        )
        .unwrap();
    let mut error = 0.0f32;
    for y in 0..side {
        for x in 0..side {
            let gx = x + roi.x as usize;
            let gy = y + roi.y as usize;
            let uv = width * height + gy / 2 * width + gx / 2 * 2;
            let luma = 1.164375 * (pixels[gy * width + gx] as f32 / 255.0 - 0.0625);
            let u = pixels[uv] as f32 / 255.0 - 0.5;
            let v = pixels[uv + 1] as f32 / 255.0 - 0.5;
            let rgb = [
                luma + 1.5934375 * v,
                luma - 0.39125 * u - 0.81264 * v,
                luma + 2.0211 * u,
            ];
            for (c, reference) in rgb.into_iter().enumerate() {
                let actual = values[c * n + y * side + x];
                error = error.max((actual - reference.clamp(0.0, 1.0)).abs());
                assert!(actual.is_finite());
            }
        }
    }
    assert!(error < 0.006, "RGB parity max error {error}");
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
    map_count: usize,
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
    frame: &GpuFrameData,
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
            map_count: 0,
            plane_copies: 0,
        };
        for roi in rois {
            let one = pipeline
                .roi_bindings(
                    frame,
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
            all.map_count += one.map_count;
            all.plane_copies += one.plane_copies;
        }
        all
    } else {
        pipeline
            .roi_bindings(frame, model.ort_session(), &name, SIZE, rois, group)
            .unwrap()
    };
    let prep_ms = begin.elapsed().as_secs_f64() * 1000.0;
    let expected_maps = if group == 0 { rois.len() } else { 1 };
    assert_eq!(prepared.map_count, expected_maps);
    assert_eq!(prepared.plane_copies, 2 * expected_maps);
    assert_eq!(
        prepared.source_bytes,
        expected_maps * frame.width as usize * frame.height as usize * 3 / 2
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
            let [w, h] = [frame.width as f32, frame.height as f32];
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
        map_count: prepared.map_count,
        plane_copies: prepared.plane_copies,
        source_bytes: prepared.source_bytes,
    }
}

fn row(run: &Run) -> Value {
    json!({"preprocessMs":run.prep_ms,"inferenceMs":run.run_ms,"decodeMs":run.decode_ms,
           "sampledDeviceDeltaBytes":run.used_delta,"liveInputTensorBytes":run.live_bytes,"mapCount":run.map_count,
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
    let pipeline = GpuPipeline::global().unwrap();
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
    let pixels = fill_texture(pipeline, &frame);
    let probe = Roi {
        x: 313,
        y: 117,
        w: SIZE,
        h: SIZE,
    };
    let name = model.inputs[0].name.clone();
    let parity = pipeline
        .roi_bindings(&frame, model.ort_session(), &name, SIZE, &[probe], 1)
        .unwrap();
    let pixel_error = check_tensor(pipeline, &parity.inputs[0], probe, &pixels, 1920, 1080);
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
                &frame,
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
                &frame,
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
