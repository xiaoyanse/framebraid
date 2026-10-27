use super::*;
use am_vision_core::sdk;

pub(super) fn execute(
    entry: Arc<Entry>,
    inputs: Vec<Input>,
    ids: Vec<u64>,
    admissions: Vec<Admission>,
) {
    let Ok(model) = entry.data::<ModelHandle>() else {
        return;
    };
    let canceled = lock(&model.state.records)
        .map(|mut records| {
            let mut live = false;
            for id in &ids {
                if let Some(r) = records.get_mut(id) {
                    if !r.canceled && r.state < AM_REQ_DONE as u32 {
                        r.state = AM_REQ_RUNNING as u32;
                        r.started = am_sdk_support::clock::monotonic_ms();
                        live = true;
                    }
                }
            }
            !live
        })
        .unwrap_or(true);
    let input_budget = Admission::totals(&admissions).1;
    let output = if canceled || model.stopping.load(Ordering::Acquire) {
        Err(AM_ERR_SDK_HANDLE_CLOSED)
    } else {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(model, &inputs, &ids, input_budget)
        }))
        .unwrap_or(Err(AM_ERR_SDK_INTERNAL_ERROR))
    };
    // 固定批次内取消只改变该项终态；仍在执行的整批输入不得提前归还。
    drop(inputs);
    drop(admissions);
    match output {
        Ok(results) => {
            for (id, result) in ids.iter().zip(results) {
                execution::finish(model, *id, Ok(result));
            }
        }
        Err(code) => {
            for id in &ids {
                execution::finish(model, *id, Err(code));
            }
        }
    }
    model.inflight.fetch_sub(ids.len(), Ordering::AcqRel);
    model.state.running.fetch_sub(1, Ordering::AcqRel);
    model.busy.store(false, Ordering::Release);
    model.state.changed.notify_all();
}
fn run(
    model: &ModelHandle,
    inputs: &[Input],
    ids: &[u64],
    input_budget: usize,
) -> Result<Vec<Arc<ResultBuffer>>> {
    let ctx = model.state.context()?;
    let context = ctx.data::<Context>()?;
    let limits = *lock(&context.limits)?;
    let mut session = lock(&model.session)?;
    let session = session.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
    if session.profile.is_some() {
        return Err(AM_ERR_VISION_BATCH_UNSUPPORTED);
    }
    let prepared = inputs
        .iter()
        .map(|v| {
            v.prepare(
                &session.inputs[0].name,
                limits.preprocess_budget_bytes as usize,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let known = session.outputs.iter().try_fold(0usize, |n, s| {
        let mut shape = s.shape.clone();
        *shape.first_mut()? = 1;
        let bytes = sdk::byte_count(s.dtype, &shape)
            .ok()?
            .checked_next_multiple_of(8)?;
        n.checked_add(bytes)
    });
    let overhead = 512 * 1024;
    let available = limits
        .result_budget_bytes
        .min(lock(&context.queues)?[AM_Q_RESULTS as usize].max_bytes) as usize;
    let maximum = available
        .checked_sub(overhead * inputs.len())
        .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?
        / 2
        / inputs.len();
    let row_bytes = known.unwrap_or(maximum);
    if row_bytes == 0 || row_bytes > maximum {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    let temporary = model.state.temporary(row_bytes * inputs.len())?;
    let charges = (0..inputs.len())
        .map(|_| model.state.charge(row_bytes + overhead))
        .collect::<Result<Vec<_>>>()?;
    let start = am_sdk_support::clock::monotonic_ms();
    let output = session.run_batch(
        &prepared.iter().map(|p| p.tensors()).collect::<Vec<_>>(),
        input_budget,
        row_bytes * inputs.len() * 2,
    );
    model.refresh_selection(session);
    let output = output.map_err(error)?;
    let end = am_sdk_support::clock::monotonic_ms();
    drop(temporary);
    output
        .into_iter()
        .zip(charges)
        .enumerate()
        .map(|(i, (raw, charge))| {
            ResultBuffer::build(
                model,
                ids[i],
                inputs[i].assoc(),
                prepared[i].transform(),
                raw,
                None,
                start,
                end,
                charge,
            )
        })
        .collect()
}
