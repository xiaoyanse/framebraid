use super::batching_exports::amv_model_configure_batching;
use super::exports::*;
use super::models::ModelHandle;
use super::state::State;
use am_sdk_support::{abi::*, context};
use am_vision_core::gpu::{
    cuda_sys::CudaApi,
    interop::{nvidia_nv12_texture, RegisteredTexture},
};
use std::{
    ffi::CString,
    path::PathBuf,
    ptr,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

struct StopSampler<'a>(&'a std::sync::atomic::AtomicBool);
impl Drop for StopSampler<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

struct Objects {
    ctx: usize,
    models: Vec<usize>,
}

impl Objects {
    fn model(&mut self, path: PathBuf, profile: &str, backend: i32) -> usize {
        let path = CString::new(path.to_str().unwrap()).unwrap();
        let profile = CString::new(profile).unwrap();
        let policy = am_backend_policy_t {
            requested: backend as u32,
            strict: if backend == AM_BACKEND_CUDA && std::env::var_os("FB_ROI_CUDA_FP32").is_some()
            {
                AM_BACKEND_STRICT_CUDA_FP32
            } else {
                1
            },
            ..Default::default()
        };
        let desc = am_model_desc_t {
            path_utf8: path.as_ptr(),
            profile_id: profile.as_ptr(),
            backend: &policy,
            intra_threads: 2,
            ..Default::default()
        };
        let mut out = ptr::null_mut();
        assert_eq!(unsafe { amv_model_load(self.ctx as _, &desc, &mut out) }, 0);
        self.models.push(out as usize);
        out as usize
    }
}
impl Drop for Objects {
    fn drop(&mut self) {
        for key in self.models.drain(..).rev() {
            let _ = crate::domain().close(key, 5000);
            let _ = crate::domain().destroy(key);
        }
        let _ = crate::domain().close(self.ctx, 5000);
        let _ = crate::domain().destroy(self.ctx);
    }
}

#[derive(Debug, Clone, Copy)]
struct Hit {
    class: i32,
    bounds: [f32; 4],
    roi: [f32; 4],
    parent: i32,
    score: f32,
}

fn hits(ctx: usize, id: u64, result: *mut amv_result_t) -> Vec<Hit> {
    unsafe {
        let view = &*(*result).cascade;
        assert_eq!(view.skipped_count, 0);
        let hits: Vec<_> = (0..view.secondary_count as usize)
            .map(|i| {
                let row = &*view.secondary.add(i);
                let meta = &*view.hits.add(i);
                Hit {
                    class: row.class_id,
                    bounds: row.bounds,
                    roi: meta.roi,
                    parent: meta.primary_index,
                    score: row.confidence,
                }
            })
            .collect();
        assert_eq!(amv_result_release(result), 0);
        assert_eq!(amv_result_ack(ctx as _, id), 0);
        hits
    }
}

fn state(ctx: usize, id: u64, want: u32) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut result = ptr::null_mut();
        assert_eq!(unsafe { amv_request_query(ctx as _, id, &mut result) }, 0);
        let snapshot = unsafe { &*result };
        let actual = snapshot.state;
        let code = unsafe { snapshot.error.as_ref() }.map(|e| e.code);
        assert_eq!(unsafe { amv_request_snapshot_release(result) }, 0);
        if actual == want {
            break;
        }
        assert!(
            actual < AM_REQ_DONE as u32,
            "{id}: {actual} {code:?} expected {want}"
        );
        assert!(
            Instant::now() < deadline,
            "request {id}: state {actual} expected {want}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn configure(model: usize, enable: bool) {
    let opts = am_batching_options_t {
        flags: if enable { AM_BATCHING_ENABLE } else { 0 },
        max_batch_items: 4,
        ..Default::default()
    };
    assert_eq!(
        unsafe { amv_model_configure_batching(model as _, &opts) },
        0
    );
}

fn bind(model: usize, tower: usize) {
    let label = CString::new("fixture-token-777").unwrap();
    let mut tokens = [0i64; 77];
    tokens[..3].copy_from_slice(&[49406, 777, 49407]);
    let entry = am_vocab_entry_t {
        label_utf8: label.as_ptr(),
        token_ids: tokens.as_ptr(),
        token_rows: 1,
        token_cols: 77,
        ..Default::default()
    };
    assert_eq!(
        unsafe { amv_vocab_bind(model as _, tower as _, &entry, 1) },
        0
    );
}

fn compare(reference: &[Hit], actual: &[Hit]) -> (usize, f32, f32) {
    let mut remaining = actual.to_vec();
    let mut matched = 0;
    let mut max_px = 0.0f32;
    let mut max_score = 0.0f32;
    for expected in reference {
        let candidate = remaining
            .iter()
            .enumerate()
            .filter(|(_, h)| {
                h.class == expected.class && h.roi == expected.roi && h.parent == expected.parent
            })
            .map(|(i, h)| {
                (
                    i,
                    h.bounds
                        .into_iter()
                        .zip(expected.bounds)
                        .map(|(x, y)| (x - y).abs() * 640.0)
                        .fold(0.0f32, f32::max),
                )
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, delta)) = candidate {
            let hit = remaining.swap_remove(i);
            matched += 1;
            max_px = max_px.max(delta);
            max_score = max_score.max((expected.score - hit.score).abs());
        }
    }
    (matched, max_px, max_score)
}

#[test]
#[ignore = "requires real NVIDIA GPU, YOLO-World weights, and existing NV12 replay samples"]
fn gpu_cascade_public_entry_real_replay() {
    let ort = PathBuf::from(std::env::var_os("AM_SDK_ORT_DIR").unwrap());
    let fixture = PathBuf::from(std::env::var_os("FB_ROI_FIXTURES").unwrap());
    let dir = CString::new(ort.to_str().unwrap()).unwrap();
    let dirs = [dir.as_ptr()];
    let options = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: dirs.as_ptr(),
        runtime_dir_count: 1,
        infer_threads: 2,
        ..Default::default()
    };
    let ctx = unsafe { context::initialize_vision(crate::domain(), &options) }.unwrap();
    let mut objects = Objects {
        ctx,
        models: Vec::new(),
    };
    let photos = std::env::var_os("FB_ROI_PHOTOS").map(PathBuf::from);
    let detector = photos.as_ref().map_or_else(
        || fixture.join("yolo-world-batchd/world-batch.onnx"),
        |dir| dir.join("yolo11n.onnx"),
    );
    let profile = if photos.is_some() {
        "detect-yolo11n-nms-v1"
    } else {
        "detect-yolo-world-batch-v1"
    };
    let model = |objects: &mut Objects, backend| objects.model(detector.clone(), profile, backend);
    let cpu_a = model(&mut objects, AM_BACKEND_CPU);
    let cpu_b = model(&mut objects, AM_BACKEND_CPU);
    let gpu_a = model(&mut objects, AM_BACKEND_CUDA);
    let gpu_b = model(&mut objects, AM_BACKEND_CUDA);
    if photos.is_none() {
        let tower = objects.model(
            fixture.join("yolo-world/text.onnx"),
            "text-encoder-clip-dynseq-v1",
            AM_BACKEND_CPU,
        );
        for m in [cpu_a, cpu_b, gpu_a, gpu_b] {
            bind(m, tower);
        }
    }
    let mut capability = amv_capability_t::default();
    assert_eq!(
        unsafe { crate::amv_query_capability(ctx as _, &mut capability) },
        0
    );
    assert_ne!(u64::from(capability.feature_bits) & AMV_CAP_GPU_CASCADE, 0);
    let runtime = CudaApi::load().unwrap();
    runtime
        .check(unsafe { (runtime.set_device)(0) }, "device")
        .unwrap();
    let free_memory = || {
        let (mut free, mut total) = (0, 0);
        assert_eq!(unsafe { (runtime.mem_get_info)(&mut free, &mut total) }, 0);
        free
    };
    let native_runs = |key| {
        let entry = crate::domain().get(key).unwrap();
        let handle = entry.data::<ModelHandle>().unwrap();
        let session = handle.session.lock().unwrap();
        session.as_ref().unwrap().selection_report().native_runs
    };
    let rois: [[f32; 4]; 8] = [
        [0., 0., 1., 1.],
        [0.05, 0.13, 0.95, 0.81],
        [0.1, 0.05, 0.81, 0.92],
        [0.17, 0.19, 0.91, 0.93],
        [0., 0., 0.85, 1.],
        [0.15, 0., 1., 0.9],
        [0., 0.13, 1., 1.],
        [0.1, 0., 1., 1.],
    ];
    let request = am_cascade_request_t {
        static_rois: rois.as_flattened().as_ptr(),
        static_roi_count: 8,
        secondary_confidence: if photos.is_some() { 0.25 } else { 0.0 },
        ..Default::default()
    };
    let params = am_infer_params_t {
        confidence_threshold: if photos.is_some() { 0.25 } else { 0.0 },
        max_det: 1,
        ..Default::default()
    };
    let call_with = |primary: usize,
                     secondary: usize,
                     input: &am_image_input_t,
                     request: &am_cascade_request_t| {
        let begin = Instant::now();
        let mut output = ptr::null_mut();
        let mut id = 0;
        let rc = unsafe {
            amv_infer_cascade_sync(
                primary as _,
                secondary as _,
                input,
                request,
                &params,
                &mut output,
                &mut id,
            )
        };
        assert_eq!(rc, 0, "sync cascade request {id}");
        (
            hits(ctx, id, output),
            begin.elapsed().as_secs_f64() * 1000.0,
        )
    };
    let call = |primary, secondary, input: &am_image_input_t| {
        call_with(primary, secondary, input, &request)
    };
    let mut total_hits = 0;
    let mut max_cpu_drift = 0.0f32;
    let mut max_batch_drift = 0.0f32;
    let samples: &[&str] = if photos.is_some() {
        &["bus", "zidane"]
    } else {
        &["seek_f030", "seek_f031", "s1_f013", "s1_f014"]
    };
    for &sample in samples {
        let directory = photos
            .clone()
            .unwrap_or_else(|| fixture.join("yolo-world-batchd"));
        let bytes = std::fs::read(directory.join(format!("{sample}.nv12"))).unwrap();
        assert_eq!(bytes.len(), 614400);
        let texture = unsafe { nvidia_nv12_texture(640, 640) }.unwrap();
        {
            let reg = unsafe { RegisteredTexture::register(runtime.clone(), texture.0) }.unwrap();
            let y = reg.map_subresource(0).unwrap();
            let uv = reg.map_subresource(1).unwrap();
            for (array, offset, rows) in [(y.array, 0, 640), (uv.array, 409600, 320)] {
                runtime
                    .check(
                        unsafe {
                            (runtime.memcpy2d_to_array)(
                                array,
                                0,
                                0,
                                bytes.as_ptr().add(offset).cast(),
                                640,
                                640,
                                rows,
                                1,
                            )
                        },
                        "frame upload",
                    )
                    .unwrap();
            }
        }
        let frame = am_frame_view_t {
            width: 640,
            height: 640,
            format: AM_PF_NV12 as u32,
            plane_count: 2,
            planes: [
                bytes.as_ptr(),
                bytes[409600..].as_ptr(),
                ptr::null(),
                ptr::null(),
            ],
            strides: [640, 640, 0, 0],
            plane_size_bytes: [409600, 204800, 0, 0],
            ..Default::default()
        };
        let view = am_pixel_view_t {
            magic: AM_PIXEL_VIEW_MAGIC,
            view: &frame,
            ..Default::default()
        };
        let cpu = am_image_input_t {
            kind: AM_IMG_PIXEL_VIEW,
            view: &view,
            ..Default::default()
        };
        let gpu_ref = am_gpu_frame_ref_t {
            width: 640,
            height: 640,
            d3d_format: 103,
            texture: texture.0,
            ..Default::default()
        };
        let gpu = am_image_input_t {
            kind: AM_IMG_GPU_FRAME,
            gpu: &gpu_ref,
            ..Default::default()
        };
        let (cpu_hits, cpu_ms) = call(cpu_a, cpu_b, &cpu);
        let (cuda_pixels, cuda_pixels_ms) = call(gpu_a, gpu_b, &cpu);
        let pixels_cmp = compare(&cpu_hits, &cuda_pixels);
        configure(gpu_b, false);
        let before = native_runs(gpu_b);
        let (single, single_ms) = call(gpu_a, gpu_b, &gpu);
        assert_eq!(native_runs(gpu_b) - before, 8);
        configure(gpu_b, true);
        let before = native_runs(gpu_b);
        let (batch, batch_ms) = call(gpu_a, gpu_b, &gpu);
        assert_eq!(native_runs(gpu_b) - before, 2);
        let cpu_cmp = compare(&cpu_hits, &single);
        if photos.is_some() {
            assert!(
                !cpu_hits.is_empty(),
                "{sample}: no detections at threshold 0.25"
            );
            assert_eq!(cpu_cmp.0, cpu_hits.len(), "{sample}: CPU ROI identity");
            assert_eq!(cpu_hits.len(), single.len(), "{sample}: detection count");
            let score_min = cpu_hits.iter().map(|hit| hit.score).fold(1.0f32, f32::min);
            eprintln!(
                "ROI_PHOTO {sample} detections={} min_score={score_min:.5}",
                cpu_hits.len()
            );
        }
        let gpu_cmp = compare(&single, &batch);
        let source_cmp = compare(&cuda_pixels, &single);
        eprintln!("ROI_ABLATION {sample} cpu_cuda_pixels={pixels_cmp:?} cuda_pixels_gpu_source={source_cmp:?} cuda_pixels_ms={cuda_pixels_ms:.2}");
        if cpu_cmp.1 > 10.0 {
            for (index, (cpu, gpu)) in cpu_hits.iter().zip(&single).enumerate() {
                eprintln!("ROI_BOX {sample} {index}: CPU {cpu:?}, GPU {gpu:?}");
            }
        }
        eprintln!("ROI_REPLAY {sample} cpu={} gpu={} batch={} cpu_gpu={cpu_cmp:?} gpu_batch={gpu_cmp:?} ms={cpu_ms:.2}/{single_ms:.2}/{batch_ms:.2}",cpu_hits.len(),single.len(),batch.len());
        assert_eq!(source_cmp.0, cuda_pixels.len());
        assert_eq!(cuda_pixels.len(), single.len());
        assert!(
            source_cmp.1 < 0.5,
            "{sample}: same-CUDA pixel vs texture {source_cmp:?}"
        );
        assert_eq!(gpu_cmp.0, single.len());
        assert!(
            gpu_cmp.1 < 1.0,
            "{sample}: CUDA single vs batch {gpu_cmp:?}"
        );
        assert_eq!(single.len(), batch.len());

        total_hits += cpu_cmp.0;
        max_cpu_drift = max_cpu_drift.max(cpu_cmp.1);
        max_batch_drift = max_batch_drift.max(gpu_cmp.1);
        if photos.is_some() || std::env::var_os("FB_ROI_CUDA_FP32").is_some() {
            let rule = am_cascade_rule_t {
                source_class_id: -1,
                expand: 0.15,
                min_roi_size: 0.0001,
                ..Default::default()
            };
            let dynamic = am_cascade_request_t {
                rules: &rule,
                rule_count: 1,
                ..Default::default()
            };
            let (cpu_dynamic, _) = call_with(cpu_a, cpu_b, &cpu, &dynamic);
            let (gpu_dynamic, _) = call_with(gpu_a, gpu_b, &gpu, &dynamic);
            assert_eq!(cpu_dynamic.len(), 1, "{sample}: CPU dynamic ROI");
            assert_eq!(gpu_dynamic.len(), 1, "{sample}: GPU dynamic ROI");
            let (cpu_hit, gpu_hit) = (cpu_dynamic[0], gpu_dynamic[0]);
            assert_eq!((cpu_hit.parent, gpu_hit.parent), (0, 0));
            assert_eq!(cpu_hit.class, gpu_hit.class);
            let delta = |left: [f32; 4], right: [f32; 4]| {
                left.into_iter()
                    .zip(right)
                    .map(|(a, b)| (a - b).abs() * 640.0)
                    .fold(0.0f32, f32::max)
            };
            let roi_px = delta(cpu_hit.roi, gpu_hit.roi);
            let box_px = delta(cpu_hit.bounds, gpu_hit.bounds);
            eprintln!("ROI_DYNAMIC {sample} roi_px={roi_px:.5} box_px={box_px:.5}");
            assert!(roi_px < 0.5 && box_px < 3.0);
        }
        let submit = |primary, secondary, input: &am_image_input_t| {
            let mut id = 0;
            let code = unsafe {
                amv_infer_cascade_async(
                    primary as _,
                    secondary as _,
                    input,
                    &request,
                    &params,
                    &mut id,
                )
            };
            (code, id)
        };
        assert_eq!(submit(cpu_a, cpu_b, &gpu), (AM_ERR_SDK_NOT_SUPPORTED, 0));
        if sample != "s1_f014" && sample != "zidane" {
            continue;
        }
        // Three logical lanes share a pair of model sessions. Keep all three
        // accepted simultaneously before releasing the primary worker gate.
        let peak_usage = State::get(ctx).unwrap();
        let peak_entry = crate::domain().get(gpu_a).unwrap();
        let peak_model = peak_entry.data::<ModelHandle>().unwrap();
        let policy = am_realtime_policy_t {
            max_inflight: 1,
            slot_count: 3,
            ..Default::default()
        };
        assert_eq!(unsafe { amv_set_realtime_policy(gpu_a as _, &policy) }, 0);
        let bytes_per_job = 614400 + 4 * 640 * 640 * 12;
        let idle_free = free_memory();
        let done = std::sync::atomic::AtomicBool::new(false);
        let (floor, max_admission, sampled) = std::thread::scope(|scope| {
            let monitor = scope.spawn(|| {
                let mut floor = usize::MAX;
                let mut max_admission = 0;
                let mut sampled = 0;
                while !done.load(Ordering::Acquire) {
                    floor = floor.min(free_memory());
                    max_admission =
                        max_admission.max(peak_usage.usage.preprocess.lock().unwrap()[0]);
                    sampled += 1;
                    std::thread::sleep(Duration::from_millis(2));
                }
                (floor, max_admission, sampled)
            });
            let _stop = StopSampler(&done);
            for _ in 0..5 {
                let gate = peak_model.session.lock().unwrap();
                let mut ids = Vec::new();
                for slot in 0..3 {
                    let lane = am_infer_params_t {
                        slot_key: slot,
                        ..params
                    };
                    let mut id = 0;
                    assert_eq!(
                        unsafe {
                            amv_infer_cascade_async(
                                gpu_a as _, gpu_b as _, &gpu, &request, &lane, &mut id,
                            )
                        },
                        0
                    );
                    assert!(id != 0);
                    ids.push(id);
                }
                assert_eq!(
                    peak_usage.usage.preprocess.lock().unwrap()[0],
                    3 * bytes_per_job
                );
                assert_eq!(submit(gpu_a, gpu_b, &gpu), (AM_ERR_SDK_QUEUE_FULL, 0));
                drop(gate);
                for id in ids {
                    state(ctx, id, AM_REQ_DONE as u32);
                    let mut output = ptr::null_mut();
                    assert_eq!(unsafe { amv_result_fetch(ctx as _, id, &mut output) }, 0);
                    assert_eq!(hits(ctx, id, output).len(), batch.len());
                }
                assert_eq!(peak_usage.usage.preprocess.lock().unwrap()[0], 0);
            }
            drop(_stop);
            monitor.join().unwrap()
        });
        eprintln!("ROI_MULTILANE lanes=3 jobs=15 samples={sampled} free_idle={idle_free} free_low={floor} free_after={} sampled_admission={max_admission} exact_admission={}",free_memory(),3*bytes_per_job);
        assert!(sampled > 10);
        let policy = am_realtime_policy_t {
            max_inflight: 1,
            slot_count: 1,
            ..Default::default()
        };
        assert_eq!(unsafe { amv_set_realtime_policy(gpu_a as _, &policy) }, 0);
        let _ = call(cpu_a, cpu_b, &cpu);
        let model_entry = crate::domain().get(gpu_a).unwrap();
        let primary = model_entry.data::<ModelHandle>().unwrap();
        let usage = State::get(ctx).unwrap();
        let expected = 614400 + 4 * 640 * 640 * 12;
        let mut queue = am_queue_policy_t::default();
        assert_eq!(
            unsafe {
                crate::amv_get_queue_policy(ctx as _, AM_Q_REALTIME_INFER as u32, &mut queue)
            },
            0
        );
        queue.max_bytes = expected as u64;
        assert_eq!(
            unsafe { crate::amv_set_queue_policy(ctx as _, AM_Q_REALTIME_INFER as u32, &queue) },
            0
        );
        let session = primary.session.lock().unwrap();
        let mem_before = free_memory();
        let (code, id) = submit(gpu_a, gpu_b, &gpu);
        assert_eq!(code, 0);
        state(ctx, id, AM_REQ_RUNNING as u32);
        assert_eq!(usage.usage.preprocess.lock().unwrap()[0], expected);
        let mem_accepted = free_memory();
        assert_eq!(submit(gpu_a, gpu_b, &gpu), (AM_ERR_SDK_QUEUE_FULL, 0));
        let mem_rejected = free_memory();
        assert_eq!(unsafe { amv_request_cancel(ctx as _, id) }, 0);
        assert_eq!(usage.usage.preprocess.lock().unwrap()[0], expected);
        drop(session);
        state(ctx, id, AM_REQ_CANCELED as u32);
        assert_eq!(usage.usage.preprocess.lock().unwrap()[0], 0);
        assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0);
        let mem_canceled = free_memory();
        let session = primary.session.lock().unwrap();
        let (code, id) = submit(gpu_a, gpu_b, &gpu);
        assert_eq!(code, 0);
        state(ctx, id, AM_REQ_RUNNING as u32);
        assert_eq!(
            crate::domain().close(gpu_b, 0).unwrap(),
            AM_LIFECYCLE_CLOSING as u32
        );
        assert_eq!(
            crate::domain().close(gpu_a, 0).unwrap(),
            AM_LIFECYCLE_CLOSING as u32
        );
        assert!(primary.stopping.load(Ordering::Acquire));
        drop(session);
        assert_eq!(
            crate::domain().close(gpu_a, 5000).unwrap(),
            AM_LIFECYCLE_CLOSED as u32
        );
        assert_eq!(
            crate::domain().close(gpu_b, 5000).unwrap(),
            AM_LIFECYCLE_CLOSED as u32
        );
        assert_eq!(usage.usage.preprocess.lock().unwrap()[0], 0);
        state(ctx, id, AM_REQ_CANCELED as u32);
        assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0);
        let mem_closed = free_memory();
        eprintln!("ROI_DEVICE_MEMORY free_before={mem_before} accepted={mem_accepted} rejected={mem_rejected} canceled={mem_canceled} closed={mem_closed}");
        eprintln!("ROI_LIFECYCLE cap={expected} reject=queueFull cancel=released close=bothDrained fallback=cpuPixels");
    }
    assert!(
        total_hits > 0,
        "real replay must exercise matched detections"
    );
    eprintln!("ROI_PARITY cpu_max_px={max_cpu_drift:.3} batch_max_px={max_batch_drift:.3}");
    if std::env::var_os("FB_ROI_REQUIRE_CPU_PARITY").is_some() {
        assert!(
            max_cpu_drift < 3.0,
            "CPU/GPU cascade parity failed: {max_cpu_drift:.3} pixels"
        );
    }
}
