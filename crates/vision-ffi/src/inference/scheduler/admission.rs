use super::*;
use std::collections::{HashMap, HashSet};

pub(super) unsafe fn submit(
    entry: Arc<Entry>,
    plans: &[&Plan],
    native: bool,
    offline: bool,
    deadline: Option<std::time::Instant>,
) -> Result<Vec<u64>> {
    if plans.is_empty() || plans.len() > 64 || (!native && plans.len() != 1) {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let model = entry.data::<ModelHandle>()?;
    let state = &model.state;
    let ctx = state.context()?;
    let context = ctx.data::<Context>()?;
    let limits = *lock(&context.limits)?;
    if limits.infer_parallelism == 0 {
        return Err(AM_ERR_SDK_QUEUE_FULL);
    }
    let queues = *lock(&context.queues)?;
    let class = if offline {
        AM_Q_OFFLINE_BATCH
    } else {
        AM_Q_REALTIME_INFER
    } as usize;
    let count = plans.len();
    let cost = plans
        .iter()
        .try_fold(0usize, |n, p| {
            n.checked_add(p.cost())
                .and_then(|n| n.checked_add(if native { p.native_extra() } else { 0 }))
        })
        .ok_or(AM_ERR_SDK_QUEUE_FULL)?;
    let e = engine()?;
    let admission = {
        let mut d = lock(&e.data)?;
        if d.inflight + count > e.budget.slots as usize
            || count > queues[class].max_items as usize
            || d.bytes.checked_add(cost).is_none_or(|n| {
                n > limits
                    .preprocess_budget_bytes
                    .min(queues[class].max_bytes)
                    .min(64 * 1024 * 1024) as usize
            })
        {
            return Err(AM_ERR_SDK_QUEUE_FULL);
        }
        d.inflight += count;
        d.bytes += cost;
        Admission {
            engine: e.clone(),
            bytes: cost,
            items: count,
        }
    };
    let inputs = plans
        .iter()
        .map(|p| unsafe { p.capture() })
        .collect::<Result<Vec<_>>>()?;
    let policy = *lock(&model.policy)?;
    let mut records = lock(&state.records)?;
    state.purge(&mut records);
    if model.stopping.load(Ordering::Acquire) {
        return Err(AM_ERR_SDK_HANDLE_CLOSED);
    }
    let key = model.info.key.load(Ordering::Acquire);
    let live = records
        .values()
        .filter(|r| r.model == key && r.state < AM_REQ_DONE as u32)
        .collect::<Vec<_>>();
    let mut slots = live.iter().map(|r| r.slot).collect::<HashSet<_>>();
    let mut per_slot = HashMap::new();
    for r in &live {
        *per_slot.entry(r.slot).or_insert(0usize) += 1;
    }
    let maximum = policy
        .max_inflight
        .min(limits.realtime_max_inflight)
        .min(queues[class].max_items) as usize;
    let mut replacement = None;
    for p in plans {
        let slot = p.params().slot_key;
        slots.insert(slot);
        if slots.len() > policy.slot_count as usize {
            return Err(AM_ERR_SDK_QUEUE_FULL);
        }
        let used = per_slot.entry(slot).or_insert(0);
        if *used >= maximum {
            if offline || maximum == 0 || policy.replace_oldest == 0 {
                return Err(AM_ERR_SDK_QUEUE_FULL);
            }
            replacement = Some(
                records
                    .iter()
                    .filter(|(_, r)| {
                        r.model == key
                            && r.slot == slot
                            && r.state == AM_REQ_PENDING as u32
                            && !r.offline
                    })
                    .min_by_key(|(id, _)| *id)
                    .map(|(id, _)| *id)
                    .ok_or(AM_ERR_SDK_QUEUE_FULL)?,
            );
        }
        *used += 1;
    }
    if records.len() + count > limits.result_max_items as usize + limits.control_max_items as usize
    {
        return Err(AM_ERR_SDK_QUEUE_FULL);
    }
    let sequence = state.next.load(Ordering::Acquire);
    if sequence + count as u64 > 0xffffffffffff {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    let ids = (0..count)
        .map(|i| state.prefix | (sequence + i as u64))
        .collect::<Vec<_>>();
    let now = am_sdk_support::clock::monotonic_ms();
    let mut data = lock(&e.data)?;
    if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
        return Err(AM_ERR_SDK_DEADLINE_EXCEEDED);
    }
    if let Some(old) = replacement {
        data.retirement_pending = true;
        if let Some(r) = records.get_mut(&old) {
            r.state = AM_REQ_REPLACED as u32;
            r.finished = now;
            r.expires = now + queues[AM_Q_RESULTS as usize].ttl_ms as i64;
            r.canceled = true;
        }
    }
    for (id, p) in ids.iter().zip(plans) {
        records.insert(
            *id,
            Record {
                model: key,
                slot: p.params().slot_key,
                state: AM_REQ_PENDING as u32,
                code: 0,
                created: now,
                started: 0,
                finished: 0,
                expires: i64::MAX,
                result: None,
                canceled: false,
                offline,
            },
        );
    }
    state.next.store(sequence + count as u64, Ordering::Release);
    state
        .metrics
        .accepted
        .fetch_add(count as u64, Ordering::AcqRel);
    model.inflight.fetch_add(count, Ordering::AcqRel);
    data.contexts.retain(|w| w.strong_count() > 0);
    if !data
        .contexts
        .iter()
        .any(|w| w.ptr_eq(&Arc::downgrade(state)))
    {
        data.contexts.push(Arc::downgrade(state));
    }
    let mut params = plans[0].params();
    params.raw_preprocess = std::ptr::null();
    let input = if native {
        Payload::Native {
            inputs,
            ids: ids.clone(),
        }
    } else {
        Payload::Single(inputs.into_iter().next().ok_or(AM_ERR_SDK_INTERNAL_ERROR)?)
    };
    data.queue.push_back(Job {
        model: entry.clone(),
        input,
        id: ids[0],
        params,
        parallel: limits.infer_parallelism as usize,
        _admission: admission,
    });
    drop(data);
    drop(records);
    if let Some(old) = replacement {
        state.metrics.replaced.fetch_add(1, Ordering::AcqRel);
        model.notify.enqueue(model, old, AM_REQ_REPLACED as u32);
    }
    e.wake.notify_all();
    Ok(ids)
}
