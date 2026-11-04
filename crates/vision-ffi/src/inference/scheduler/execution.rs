use super::super::results::CascadeData;
use super::*;
use am_vision_core::profiles::{self, ImageMapping, Options, TensorView};
use am_vision_core::sdk::{self, cascade, pixels::Pixels};
use std::{ffi::CString, time::Duration};
pub(super) fn worker(engine: Arc<Engine>) {
    loop {
        storage::purge_contexts(&engine);
        let mut pruned_later;
        let job = {
            let mut data = match engine.data.lock() {
                Ok(d) => d,
                Err(_) => return,
            };
            if engine.stop.load(Ordering::Acquire) {
                return;
            }
            // 攒批窗口触发（凑满/截止/关闭排空）在选队前搬入执行队列；
            // 剪除项的收口延迟到域锁外（finish 锁序与正常执行完成一致）。
            let outcome = batching::tick(std::time::Instant::now());
            for job in outcome.jobs {
                data.queue.push_back(job);
            }
            pruned_later = outcome.pruned;
            let available = |job: &Job| {
                job.model.data::<ModelHandle>().is_ok_and(|m| {
                    !m.busy.load(Ordering::Acquire)
                        && m.state.running.load(Ordering::Acquire) < job.parallel
                })
            };
            let chosen = data
                .queue
                .iter()
                .position(|j| {
                    available(j)
                        && j.model.data::<ModelHandle>().is_ok_and(|m| {
                            m.policy.lock().is_ok_and(|p| p.fair_round_robin == 0)
                                || data.last
                                    != Some((m.info.key.load(Ordering::Acquire), j.params.slot_key))
                        })
                })
                .or_else(|| data.queue.iter().position(available));
            if let Some(index) = chosen {
                let Some(job) = data.queue.remove(index) else {
                    continue;
                };
                if let Ok(m) = job.model.data::<ModelHandle>() {
                    m.busy.store(true, Ordering::Release);
                    m.state.running.fetch_add(1, Ordering::AcqRel);
                    data.last = Some((m.info.key.load(Ordering::Acquire), job.params.slot_key));
                }
                Some((job, false))
            } else {
                let mut contended = false;
                // 普通高频提交不查records；取消/替换/关闭才触发额外扫描。
                let retired = data
                    .retirement_pending
                    .then(|| {
                        data.queue.iter().position(|job| match retired::check(job) {
                            Some(value) => value,
                            None => {
                                contended = true;
                                false
                            }
                        })
                    })
                    .flatten();
                if let Some(index) = retired {
                    data.queue.remove(index).map(|job| (job, true))
                } else {
                    if !contended {
                        data.retirement_pending = false;
                    }
                    // records仅try_lock，避免与准入的records→域锁形成锁序反转。
                    // 攒批开窗截止时间缩短睡眠，保证窗口及时触发。
                    let delay = if contended {
                        1
                    } else {
                        batching::next_deadline()
                            .and_then(|d| {
                                d.checked_duration_since(std::time::Instant::now())
                                    .map(|remaining| remaining.as_millis() as u64)
                            })
                            .map(|ms| ms.min(50))
                            .unwrap_or(50)
                    };
                    let _ = engine.wake.wait_timeout(data, Duration::from_millis(delay));
                    None
                }
            }
        };
        batching::finish_pruned(std::mem::take(&mut pruned_later));
        if let Some((job, retired)) = job {
            if retired {
                retired::release(job);
            } else {
                execute(job);
            }
            engine.wake.notify_all();
        }
    }
}
fn execute(job: Job) {
    let Job {
        model: entry,
        input,
        id,
        params,
        _admissions,
        parallel: _,
    } = job;
    let input = match input {
        Payload::Single(input) => input,
        Payload::Native { inputs, ids } => {
            native_execution::execute(entry, inputs, ids, _admissions);
            return;
        }
    };
    let Ok(model) = entry.data::<ModelHandle>() else {
        return;
    };
    let state = &model.state;
    let canceled = lock(&state.records)
        .map(|mut records| {
            records.get_mut(&id).is_none_or(|r| {
                if r.canceled || r.state >= AM_REQ_DONE as u32 {
                    true
                } else {
                    r.state = AM_REQ_RUNNING as u32;
                    r.started = am_sdk_support::clock::monotonic_ms();
                    false
                }
            })
        })
        .unwrap_or(true);
    let result = if canceled || model.stopping.load(Ordering::Acquire) {
        Err(AM_ERR_SDK_HANDLE_CLOSED)
    } else {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(model, &input, id, params)
        }))
        .unwrap_or(Err(AM_ERR_SDK_INTERNAL_ERROR))
    };
    // 终态前释放输入保留引用，取消意图不提前释放仍在运行的平面。
    drop(input);
    drop(_admissions);
    finish(model, id, result);
    model.inflight.fetch_sub(1, Ordering::AcqRel);
    state.running.fetch_sub(1, Ordering::AcqRel);
    model.busy.store(false, Ordering::Release);
    state.changed.notify_all();
}
pub(super) fn finish(model: &ModelHandle, id: u64, result: Result<Arc<ResultBuffer>>) {
    let state = &model.state;
    let mut terminal = None;
    if let Ok(mut records) = state.records.lock() {
        if let Some(r) = records.get_mut(&id) {
            if r.state < AM_REQ_DONE as u32 {
                let now = am_sdk_support::clock::monotonic_ms();
                r.finished = now;
                if r.state == AM_REQ_RUNNING as u32 {
                    state.metrics.executed.fetch_add(1, Ordering::AcqRel);
                    state.metrics.exec_ms.fetch_add(
                        now.saturating_sub(r.started).max(0) as u64,
                        Ordering::AcqRel,
                    );
                    state.metrics.queue_wait.fetch_add(
                        r.started.saturating_sub(r.created).max(0) as u64,
                        Ordering::AcqRel,
                    );
                }
                let ttl = state
                    .context()
                    .ok()
                    .and_then(|c| {
                        c.data::<Context>().ok().and_then(|c| {
                            c.queues
                                .lock()
                                .ok()
                                .map(|q| q[AM_Q_RESULTS as usize].ttl_ms)
                        })
                    })
                    .unwrap_or(0);
                r.expires = now + ttl as i64;
                if r.canceled || model.stopping.load(Ordering::Acquire) {
                    r.state = AM_REQ_CANCELED as u32;
                } else {
                    match result {
                        Ok(output) => {
                            r.state = AM_REQ_DONE as u32;
                            r.result = Some(output);
                        }
                        Err(code) => {
                            r.state = AM_REQ_FAILED as u32;
                            r.code = code;
                            state.metrics.failed.fetch_add(1, Ordering::AcqRel);
                        }
                    }
                }
                terminal = Some(r.state);
            }
        }
    }
    if let Some(terminal) = terminal {
        model.notify.enqueue(model, id, terminal);
    }
    state.changed.notify_all();
}
fn run_gpu(
    model: &ModelHandle,
    session: &mut am_vision_core::sdk::Model,
    gpu_ref: am_gpu_frame_ref_t,
    id: u64,
    params: am_infer_params_t,
) -> Result<Arc<ResultBuffer>> {
    use am_vision_core::gpu::pipeline::{GpuFrameData, GpuPipeline};
    if !session.is_cuda() || !GpuPipeline::available() {
        return Err(AM_ERR_SDK_NOT_SUPPORTED);
    }
    let pipeline = GpuPipeline::global().map_err(error)?;
    let name = session.inputs[0].name.clone();
    let (input_w, input_h) = {
        let shape = &session.inputs[0].shape;
        // L1 防御（审计 2026-11-01）：rank<2 下溢/负维 as u32 巨值转明确拒绝——
        // 正常加载（byte_count 拒非正维）后恒走 Ok 路径，仅异常错误码语义收窄。
        if shape.len() < 2 || shape[shape.len() - 1] <= 0 || shape[shape.len() - 2] <= 0 {
            return Err(AM_ERR_VISION_PROFILE_UNSUPPORTED);
        }
        let last = shape.len() - 1;
        (shape[last] as u32, shape[last - 1] as u32)
    };
    let frame = GpuFrameData {
        width: gpu_ref.width,
        height: gpu_ref.height,
        d3d_format: gpu_ref.d3d_format,
        subresource: gpu_ref.subresource,
        texture: gpu_ref.texture,
    };
    let (binding, transform) = pipeline
        .preprocess_to_binding(&frame, session.ort_session(), &name, input_w, input_h)
        .map_err(error)?;
    let ctx = model.state.context()?;
    let context = ctx.data::<Context>()?;
    let limits = *lock(&context.limits)?;
    let mapping = am_vision_core::sdk::ImageTransform {
        width: transform.width,
        height: transform.height,
        dst_w: transform.dst_w,
        dst_h: transform.dst_h,
        scale_x: transform.scale_x,
        scale_y: transform.scale_y,
        crop_x: 0,
        crop_y: 0,
        left: transform.left,
        top: transform.top,
    };
    let queue_bytes = lock(&context.queues)?[AM_Q_RESULTS as usize].max_bytes;
    let available = limits
        .result_budget_bytes
        .min(queue_bytes)
        .saturating_sub(model.state.usage.bytes.load(Ordering::Acquire) as u64)
        as usize;
    let raw_bytes = outputs_bytes(&session.outputs, available);
    let max_det = if params.max_det == 0 {
        300
    } else {
        params.max_det as usize
    };
    let mask_bytes = if session
        .profile
        .is_some_and(|p| p.task == profiles::Task::Segment)
    {
        (mapping.width as usize).div_ceil(8) * mapping.height as usize * max_det
    } else {
        0
    };
    let reserve = raw_bytes
        .saturating_add(mask_bytes)
        .saturating_add(512 * 1024)
        .min(available);
    if reserve == 0 || raw_bytes > reserve {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    let charge = model.state.charge(reserve)?;
    let start = am_sdk_support::clock::monotonic_ms();
    let output = session.run_gpu(&binding, reserve);
    model.refresh_selection(session);
    let raw = output.map_err(error)?;
    let prediction =
        decode_prediction(session, &raw, &mapping, params, max_det, reserve, raw_bytes)?;
    let end = am_sdk_support::clock::monotonic_ms();
    ResultBuffer::build(
        model,
        id,
        gpu_assoc(gpu_ref),
        Some(mapping),
        raw,
        prediction,
        None,
        start,
        end,
        charge,
    )
}

fn gpu_assoc(gpu_ref: am_gpu_frame_ref_t) -> Option<am_frame_assoc_t> {
    if gpu_ref.assoc.is_null() {
        return None;
    }
    unsafe { am_sdk_support::args::read(gpu_ref.assoc, size_of::<am_frame_assoc_t>()) }.ok()
}

fn decode_prediction(
    session: &am_vision_core::sdk::Model,
    raw: &[am_vision_core::sdk::TensorData],
    mapping: &am_vision_core::sdk::ImageTransform,
    params: am_infer_params_t,
    max_det: usize,
    reserve: usize,
    raw_bytes: usize,
) -> Result<Option<profiles::Prediction>> {
    // GPU/批量共用的独立解码入口无词汇上下文：亲和头档案在此路径明确
    // 拒绝（decode 内 affinity&&vocab 缺失 → profileUnsupported），与注入层
    // 的 vocabNotBound 分工一致。
    decode_profile(
        session, raw, mapping, params, max_det, reserve, raw_bytes, None,
    )
}

/// PROFILE 解码共享段（图像主路径与级联 C1 两级共用）：形状重组→TensorView→
/// profiles::decode（vocab 布局透传）；RAW 会话返回 None。
#[allow(clippy::too_many_arguments)]
fn decode_profile(
    session: &am_vision_core::sdk::Model,
    raw: &[am_vision_core::sdk::TensorData],
    mapping: &am_vision_core::sdk::ImageTransform,
    params: am_infer_params_t,
    max_det: usize,
    reserve: usize,
    raw_bytes: usize,
    vocab: Option<&profiles::VocabLayout>,
) -> Result<Option<profiles::Prediction>> {
    let Some(profile) = session.profile else {
        return Ok(None);
    };
    let shapes = raw
        .iter()
        .map(|r| r.info.shape.iter().map(|d| *d as usize).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let tensors = raw
        .iter()
        .zip(&shapes)
        .map(|(r, s)| {
            Ok(TensorView {
                name: &r.info.name,
                shape: s,
                values: r.floats().map_err(error)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(
        profiles::decode(
            &profile.id,
            &tensors,
            ImageMapping {
                width: mapping.width,
                height: mapping.height,
                scale: mapping.scale_x,
                left: mapping.left,
                top: mapping.top,
            },
            Options {
                confidence: params.confidence_threshold,
                nms_iou: if params.nms_iou_threshold == 0.0 {
                    0.7
                } else {
                    params.nms_iou_threshold
                },
                max_detections: max_det,
                mask_budget_bytes: reserve.saturating_sub(raw_bytes + 512 * 1024),
                ..Options::default()
            },
            vocab,
        )
        .map_err(error)?,
    ))
}

/// 会话输出字节量合计（形状积×dtype 宽度；形状异常按 fallback 饱和）。
fn outputs_bytes(outputs: &[sdk::TensorInfo], fallback: usize) -> usize {
    outputs
        .iter()
        .try_fold(0usize, |n, s| {
            let count = s.shape.iter().try_fold(1usize, |a, d| {
                usize::try_from(*d).ok().and_then(|v| a.checked_mul(v))
            });
            count
                .and_then(|v| {
                    v.checked_mul(match s.dtype {
                        1 => 2,
                        2 => 8,
                        4 => 1,
                        _ => 4,
                    })
                })
                .and_then(|v| n.checked_add(v))
        })
        .unwrap_or(fallback)
}

/// A1 词汇注入共享段（§4.1）：PROFILE 双输入 detect 档案（YOLO-World 头形态）
/// 的第二输入由词表嵌入快照自动填充——在途请求快照后执行，换代不影响已开始
/// 推理；未绑定时按合同拒绝（vocabNotBound）；单输入/RAW 模型返回 None。
fn vocab_injection(
    handle: &ModelHandle,
    session: &am_vision_core::sdk::Model,
) -> Result<Option<(sdk::TensorData, profiles::VocabLayout)>> {
    if !session
        .profile
        .is_some_and(|p| p.inputs.len() > 1 && p.classes.is_none())
    {
        return Ok(None);
    }
    let snapshot = handle
        .vocab
        .read()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
        .clone();
    let Some(vocab) = snapshot.as_ref() else {
        return Err(AM_ERR_VISION_VOCAB_NOT_BOUND);
    };
    let tensor = vocab.injection_tensor(&session.inputs[1]).map_err(error)?;
    let layout = vocab.layout();
    Ok(Some((tensor, layout)))
}

fn run(
    model: &ModelHandle,
    input: &Input,
    id: u64,
    params: am_infer_params_t,
) -> Result<Arc<ResultBuffer>> {
    let ctx = model.state.context()?;
    let context = ctx.data::<Context>()?;
    let limits = *lock(&context.limits)?;
    // ROI 级联分派（C1 §4.1）：双会话锁须按句柄键序获取，在一级会话锁前分流。
    if let super::super::submission::Input::Cascade(..) = input {
        return run_cascade(model, input, id, params, context, limits);
    }
    let mut session = lock(&model.session)?;
    let session = session.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
    // GPU 帧直传分派（M2b §3.3a）：显式输入不静默降级——非 CUDA 后端或管线
    // 不可用报 notSupported（宿主按能力位选择输入 kind，像素路径永远可用）。
    if let super::super::submission::Input::Image(image) = input {
        if let Some(gpu_ref) = image.gpu {
            return run_gpu(model, session, gpu_ref, id, params);
        }
    }
    let prepared = input.prepare(
        &session.inputs[0].name,
        limits.preprocess_budget_bytes as usize,
    )?;
    // 张量没有原图变换；业务档案在输入平面归一化，绝不伪造回原图的变换。
    let mapping = prepared.transform();
    let m = mapping.unwrap_or_else(|| {
        let shape = &session.inputs[0].shape;
        // L1 防御（审计 2026-11-01）：PROFILE 档案输入恒为 NCHW rank4（档案校验），
        // 异常 shape 不下标越界——按未知尺寸 0 走既有拒绝链。
        let (width, height) =
            if session.profile.is_some() && shape.len() == 4 && shape[2] > 0 && shape[3] > 0 {
                (shape[3] as u32, shape[2] as u32)
            } else {
                (0, 0)
            };
        am_vision_core::sdk::ImageTransform {
            width,
            height,
            dst_w: width,
            dst_h: height,
            scale_x: 1.0,
            scale_y: 1.0,
            crop_x: 0,
            crop_y: 0,
            left: 0,
            top: 0,
        }
    });
    let queue_bytes = lock(&context.queues)?[AM_Q_RESULTS as usize].max_bytes;
    let available = limits
        .result_budget_bytes
        .min(queue_bytes)
        .saturating_sub(model.state.usage.bytes.load(Ordering::Acquire) as u64)
        as usize;
    let raw_bytes = outputs_bytes(&session.outputs, available);
    let max_det = if params.max_det == 0 {
        300
    } else {
        params.max_det as usize
    };
    let mask_bytes = if session
        .profile
        .is_some_and(|p| p.task == profiles::Task::Segment)
    {
        (m.width as usize).div_ceil(8) * m.height as usize * max_det
    } else {
        0
    };
    let reserve = raw_bytes
        .saturating_add(mask_bytes)
        .saturating_add(512 * 1024)
        .min(available);
    if reserve == 0 || raw_bytes > reserve {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    let charge = model.state.charge(reserve)?;
    let start = am_sdk_support::clock::monotonic_ms();
    // A1 词汇注入（§4.1）：注入段与级联二级共用 vocab_injection；引用切片组装
    // 免图像张量复制（run_ref 语义与 run 一致）。
    let injected = vocab_injection(model, session)?;
    let vocab_layout = injected.as_ref().map(|(_, l)| l);
    let mut infer_inputs: Vec<&am_vision_core::sdk::TensorData> =
        prepared.tensors().iter().collect();
    if let Some((tensor, _)) = injected.as_ref() {
        infer_inputs.push(tensor);
    }
    let output = session.run_ref(&infer_inputs, reserve);
    model.refresh_selection(session);
    let raw = output.map_err(error)?;
    let prediction = decode_profile(
        session,
        &raw,
        &m,
        params,
        max_det,
        reserve,
        raw_bytes,
        vocab_layout,
    )?;
    let end = am_sdk_support::clock::monotonic_ms();
    ResultBuffer::build(
        model,
        id,
        input.assoc(),
        mapping,
        raw,
        prediction,
        None,
        start,
        end,
        charge,
    )
}

/// ROI 子图（裁剪产物）：打包格式保序拷贝，逐像素路径统一转 RGB8。
struct RoiImage {
    data: Vec<u8>,
    width: u32,
    height: u32,
    format: u32,
}

impl RoiImage {
    fn as_pixels(&self) -> Pixels<'_> {
        let bpp = if self.format == AM_PF_RGBA8 as u32 {
            4
        } else {
            3
        };
        Pixels {
            width: self.width,
            height: self.height,
            format: self.format,
            planes: vec![&self.data],
            strides: vec![self.width as usize * bpp],
            color_matrix: 0,
            color_range: 0,
        }
    }
}

/// ROI 归一化矩形裁剪：打包 RGB/BGR/RGBA 平面按行偏移直接拷贝（保留原格式，
/// 通道序交由 Pixels::rgb 处理）；NV12/其它逐像素经 Pixels::rgb 转 RGB8
/// （正确性优先）。ROI 已 clamp 非退化，像素矩形恒非空。
fn crop_roi(pixels: &Pixels<'_>, roi: [f32; 4]) -> RoiImage {
    let (x0, y0) = (
        (roi[0] * pixels.width as f32).floor() as usize,
        (roi[1] * pixels.height as f32).floor() as usize,
    );
    let (x1, y1) = (
        ((roi[2] * pixels.width as f32).ceil() as usize).clamp(x0 + 1, pixels.width as usize),
        ((roi[3] * pixels.height as f32).ceil() as usize).clamp(y0 + 1, pixels.height as usize),
    );
    let (sub_w, sub_h) = (x1 - x0, y1 - y0);
    match pixels.format as i32 {
        AM_PF_RGB8 | AM_PF_BGR8 | AM_PF_RGBA8 => {
            let bpp = if pixels.format == AM_PF_RGBA8 as u32 {
                4
            } else {
                3
            };
            let stride = pixels.strides[0];
            let mut data = Vec::with_capacity(sub_w * sub_h * bpp);
            for y in y0..y1 {
                let row = y * stride + x0 * bpp;
                data.extend_from_slice(&pixels.planes[0][row..row + sub_w * bpp]);
            }
            RoiImage {
                data,
                width: sub_w as u32,
                height: sub_h as u32,
                format: pixels.format,
            }
        }
        _ => {
            let mut data = Vec::with_capacity(sub_w * sub_h * 3);
            for y in y0..y1 {
                for x in x0..x1 {
                    let rgb = pixels.rgb(x, y);
                    data.extend_from_slice(&[rgb[0] as u8, rgb[1] as u8, rgb[2] as u8]);
                }
            }
            RoiImage {
                data,
                width: sub_w as u32,
                height: sub_h as u32,
                format: AM_PF_RGB8 as u32,
            }
        }
    }
}

/// ROI 级联执行（C1，§4.1）：一级完整推理（图像主路径同款 prepare→注入→
/// run_ref→decode）→ROI 生成→原图取像素一次→逐 ROI 裁剪放大（二级档案
/// letterbox）→二级推理解码→坐标回映原图归一化平面。失败语义：一级失败=
/// 整请求失败；二级会话锁失败/句柄关闭/词表未绑=ROI 全跳过（cascade 空、
/// 请求成功）；单 ROI 推理/解码失败跳过该 ROI。双会话锁按句柄键升序获取，
/// 防对向级联（A→B 与 B→A 并发）互锁；同句柄兼任两级已在计划期拒绝。
fn run_cascade(
    model: &ModelHandle,
    input: &Input,
    id: u64,
    params: am_infer_params_t,
    context: &Context,
    limits: am_limits_t,
) -> Result<Arc<ResultBuffer>> {
    let super::super::submission::Input::Cascade(image, job) = input else {
        return Err(AM_ERR_SDK_INTERNAL_ERROR);
    };
    let secondary_handle = job.secondary.data::<ModelHandle>().ok();
    let pkey = model.info.key.load(Ordering::Acquire);
    let secondary_first =
        secondary_handle.is_some_and(|h| h.info.key.load(Ordering::Acquire) < pkey);
    let mut secondary_guard = secondary_first
        .then(|| secondary_handle.and_then(|h| lock(&h.session).ok()))
        .flatten();
    let mut primary_lock = lock(&model.session)?;
    if !secondary_first {
        secondary_guard = secondary_handle.and_then(|h| lock(&h.session).ok());
    }
    let session = primary_lock.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
    let (prepared_tensor, mapping) = image.prepare(
        &session.inputs[0].name,
        limits.preprocess_budget_bytes as usize,
    )?;
    let queue_bytes = lock(&context.queues)?[AM_Q_RESULTS as usize].max_bytes;
    let available = limits
        .result_budget_bytes
        .min(queue_bytes)
        .saturating_sub(model.state.usage.bytes.load(Ordering::Acquire) as u64)
        as usize;
    let raw_bytes = outputs_bytes(&session.outputs, available);
    let secondary_bytes = secondary_guard
        .as_ref()
        .and_then(|g| g.as_ref())
        .map(|s| outputs_bytes(&s.outputs, available))
        .unwrap_or(0);
    // 预算：一级逻辑 + 二级单 ROI 输出×ROI 上限粗估（两级均 detect 档案，无掩膜项）。
    let reserve = raw_bytes
        .saturating_add(secondary_bytes.saturating_mul(cascade::MAX_ROIS))
        .saturating_add(512 * 1024)
        .min(available);
    if reserve == 0 || raw_bytes > reserve {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    let charge = model.state.charge(reserve)?;
    let max_det = if params.max_det == 0 {
        300
    } else {
        params.max_det as usize
    };
    let injected = vocab_injection(model, session)?;
    let vocab_layout = injected.as_ref().map(|(_, l)| l);
    let mut infer_inputs: Vec<&sdk::TensorData> =
        std::slice::from_ref(&prepared_tensor).iter().collect();
    if let Some((tensor, _)) = injected.as_ref() {
        infer_inputs.push(tensor);
    }
    let start = am_sdk_support::clock::monotonic_ms();
    let output = session.run_ref(&infer_inputs, reserve);
    model.refresh_selection(session);
    let raw = output.map_err(error)?;
    let prediction = decode_profile(
        session,
        &raw,
        &mapping,
        params,
        max_det,
        reserve,
        raw_bytes,
        vocab_layout,
    )?;
    // ROI 生成（C1）：一级框 + 规则/静态合并；一级无检出时仅静态 ROI 生效。
    let primary_boxes: Vec<cascade::PrimaryBox> = prediction
        .as_ref()
        .and_then(|p| match &p.output {
            profiles::Output::Detect(v) => Some(
                v.iter()
                    .map(|d| cascade::PrimaryBox {
                        bounds: d.bounds,
                        confidence: d.confidence,
                        class_id: d.class_id,
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    let rois = cascade::generate_rois(&primary_boxes, &job.spec.rules, &job.spec.static_rois);
    let mut secondary_session = secondary_guard.as_mut().and_then(|g| g.as_mut());
    let secondary_vocab = match (secondary_handle, secondary_session.as_deref_mut()) {
        (Some(h), Some(s)) => vocab_injection(h, s).ok().flatten(),
        _ => None,
    };
    // 二级阈值：请求 0 时已在解析处落定继承 params；NMS/max_det 沿用一级参数，
    // decode 逐 ROI 独立调用即 NMS 逐 ROI 独立。
    let mut secondary_params = params;
    secondary_params.confidence_threshold = job.spec.secondary_confidence;
    let secondary_name = secondary_session.as_ref().map(|s| s.inputs[0].name.clone());
    let labels: Vec<CString> = secondary_vocab
        .as_ref()
        .and_then(|(_, l)| {
            l.entries
                .iter()
                .map(|(label, _)| CString::new(label.as_str()).ok())
                .collect::<Option<Vec<_>>>()
        })
        .unwrap_or_default();
    let cascade_data = match (secondary_session, secondary_name.as_deref()) {
        (Some(s), Some(name)) => {
            let vocab = secondary_vocab.as_ref();
            let (boxes, hits, skipped) = image
                .with_pixels(|pixels| {
                    let mut boxes: Vec<am_detect_box_t> = Vec::new();
                    let mut hits: Vec<am_cascade_hit_t> = Vec::new();
                    let mut skipped = 0usize;
                    for (roi, primary_index) in rois {
                        // 单 ROI 失败（前处理/推理/解码）跳过该 ROI，不废整批。
                        let sub = crop_roi(pixels, roi);
                        let Ok((tensor, roi_mapping)) = sdk::prepare(
                            &sub.as_pixels(),
                            name,
                            job.secondary_preprocess,
                            limits.preprocess_budget_bytes as usize,
                        ) else {
                            skipped += 1;
                            continue;
                        };
                        let mut infer_inputs: Vec<&sdk::TensorData> = vec![&tensor];
                        if let Some((vocab_tensor, _)) = vocab {
                            infer_inputs.push(vocab_tensor);
                        }
                        let Ok(raw) = s.run_ref(&infer_inputs, reserve) else {
                            skipped += 1;
                            continue;
                        };
                        if let Some(h) = secondary_handle {
                            h.refresh_selection(s);
                        }
                        let Ok(Some(prediction)) = decode_profile(
                            s,
                            &raw,
                            &roi_mapping,
                            secondary_params,
                            max_det,
                            reserve,
                            secondary_bytes,
                            vocab.map(|(_, l)| l),
                        ) else {
                            skipped += 1;
                            continue;
                        };
                        let detections = match prediction.output {
                            profiles::Output::Detect(v) => v,
                            _ => {
                                skipped += 1;
                                continue;
                            }
                        };
                        for d in detections {
                            boxes.push(am_detect_box_t {
                                bounds: cascade::map_box_to_original(d.bounds, roi),
                                confidence: d.confidence,
                                class_id: d.class_id as i32,
                                class_name_utf8: labels
                                    .get(d.class_id as usize)
                                    .map_or(std::ptr::null(), |l| l.as_ptr()),
                            });
                            hits.push(am_cascade_hit_t { primary_index, roi });
                        }
                    }
                    (boxes, hits, skipped)
                })
                .unwrap_or_default();
            CascadeData {
                boxes,
                hits,
                labels,
                skipped,
            }
        }
        _ => CascadeData {
            boxes: Vec::new(),
            hits: Vec::new(),
            labels,
            // 二级会话不可用（句柄关闭/锁失败/词表未绑）=ROI 全跳过（三态可观测）。
            skipped: rois.len(),
        },
    };
    let end = am_sdk_support::clock::monotonic_ms();
    ResultBuffer::build(
        model,
        id,
        image.assoc,
        Some(mapping),
        raw,
        prediction,
        Some(cascade_data),
        start,
        end,
        charge,
    )
}
