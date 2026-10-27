use super::*;
use am_vision_core::profiles::{self, ImageMapping, Options, TensorView};
use std::time::Duration;
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
fn run(
    model: &ModelHandle,
    input: &Input,
    id: u64,
    params: am_infer_params_t,
) -> Result<Arc<ResultBuffer>> {
    let ctx = model.state.context()?;
    let context = ctx.data::<Context>()?;
    let limits = *lock(&context.limits)?;
    let mut session = lock(&model.session)?;
    let session = session.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
    let prepared = input.prepare(
        &session.inputs[0].name,
        limits.preprocess_budget_bytes as usize,
    )?;
    // 张量没有原图变换；业务档案在输入平面归一化，绝不伪造回原图的变换。
    let mapping = prepared.transform();
    let m = mapping.unwrap_or_else(|| {
        let shape = &session.inputs[0].shape;
        let (width, height) = if session.profile.is_some() {
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
    let raw_bytes = session
        .outputs
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
        .unwrap_or(available);
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
    let output = session.run(prepared.tensors(), reserve);
    model.refresh_selection(session);
    let raw = output.map_err(error)?;
    let prediction = if let Some(profile) = session.profile {
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
        Some(
            profiles::decode(
                &profile.id,
                &tensors,
                ImageMapping {
                    width: m.width,
                    height: m.height,
                    scale: m.scale_x,
                    left: m.left,
                    top: m.top,
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
            )
            .map_err(error)?,
        )
    } else {
        None
    };
    let end = am_sdk_support::clock::monotonic_ms();
    ResultBuffer::build(
        model,
        id,
        input.assoc(),
        mapping,
        raw,
        prediction,
        start,
        end,
        charge,
    )
}
