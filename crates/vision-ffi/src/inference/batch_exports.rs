#![allow(clippy::missing_safety_doc)]
use super::{
    input, lock,
    models::ModelHandle,
    receipt::{self, Receipt},
    scheduler,
    submission::{Plan, TensorSignature},
    tensors,
};
use am_sdk_support::{abi::*, args, context::Context, handles::Entry, status, Result};
use std::{
    mem::size_of,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};

#[no_mangle]
pub unsafe extern "C" fn amv_infer_batch(
    model: *mut amv_model_t,
    count: u32,
    items: *const am_batch_item_t,
    options: *const am_batch_options_t,
    out: *mut am_batch_receipt_t,
) -> i32 {
    status(crate::domain(), model as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::validate_output(out, size_of::<am_batch_receipt_t>())?;
        args::write(out, am_batch_receipt_t::default())?;
        if count == 0 || count > 64 {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        args::pointer(items)?;
        let options = if options.is_null() {
            am_batch_options_t {
                batch_mode: AM_BATCH_AUTO,
                ..Default::default()
            }
        } else {
            args::read(options, size_of::<am_batch_options_t>())?
        };
        if options.batch_mode > AM_BATCH_AUTO || options.wait_deadline_ms > 5000 {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        let started = Instant::now();
        let entry = crate::domain().active(model as usize)?;
        let m = entry.data::<ModelHandle>()?;
        let ctx = m.state.context()?;
        let c = ctx.data::<Context>()?;
        let queue = lock(&c.queues)?[AM_Q_OFFLINE_BATCH as usize];
        if options.wait_deadline_ms > queue.wait_deadline_ms {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        let owner = Receipt::create(&m.state, count as usize)?;
        let mut guard = lock(&owner.data)?;
        let data = &mut *guard;
        let key = data.ids.as_ptr() as usize;
        // 先登记回执容量，后接受请求；不能接受后才发现已无可交付的回执句柄。
        crate::domain().buffers.insert(key, owner.clone())?;
        let run = process(
            entry.0.clone(),
            count,
            items,
            &options,
            started,
            &mut data.ids,
            &mut data.codes,
        );
        if let Err(code) = run {
            for (id, error) in data.ids.iter().zip(data.codes.iter_mut()) {
                if *id == 0 && *error == 0 {
                    *error = code;
                }
            }
        }
        let rejected = data
            .codes
            .iter()
            .filter(|code| {
                matches!(
                    **code,
                    AM_ERR_SDK_QUEUE_FULL
                        | AM_ERR_SDK_RESOURCE_EXHAUSTED
                        | AM_ERR_SDK_DEADLINE_EXCEEDED
                )
            })
            .count();
        m.state
            .metrics
            .rejected
            .fetch_add(rejected as u64, Ordering::AcqRel);
        let value = am_batch_receipt_t {
            accepted_count: data.ids.iter().filter(|id| **id != 0).count() as u32,
            request_ids: data.ids.as_mut_ptr(),
            rejects: data.codes.as_ptr(),
            ..Default::default()
        };
        args::write(out, value)?;
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_receipt_release(pointer: *mut am_batch_receipt_t) -> i32 {
    status(crate::domain(), 0, || unsafe { receipt::release(pointer) })
}
unsafe fn process(
    entry: Arc<Entry>,
    count: u32,
    items: *const am_batch_item_t,
    options: &am_batch_options_t,
    started: Instant,
    ids: &mut [u64],
    codes: &mut [i32],
) -> Result<()> {
    let model = entry.data::<ModelHandle>()?;
    let native = match options.batch_mode {
        AM_BATCH_NATIVE => true,
        AM_BATCH_SPLIT => false,
        _ => model.info.native_batch != 0,
    };
    if (native && model.info.native_batch == 0) || (!native && model.info.native_batch >= 2) {
        return Err(AM_ERR_VISION_BATCH_UNSUPPORTED);
    }
    let metadata = model
        .info
        .input_specs
        .iter()
        .map(|s| s.name.len() + s.shape.len() * 8 + 128)
        .sum::<usize>()
        .saturating_mul(count as usize)
        .saturating_mul(4)
        .saturating_add(4096);
    let _metadata = scheduler::reserve_metadata(&model.state, metadata)?;
    let mut plans = Vec::new();
    for (i, ptr) in (0..count as usize).map(|i| (i, unsafe { items.add(i) })) {
        let plan = (|| unsafe {
            let item = args::read(ptr, size_of::<am_batch_item_t>())?;
            let params = am_infer_params_t {
                confidence_threshold: 0.25,
                slot_key: item.slot_key,
                raw_preprocess: item.raw_preprocess,
                ..Default::default()
            };
            if !item.image.is_null() && item.tensor_count == 0 && item.tensors.is_null() {
                Ok(Plan::Image(Box::new(input::plan_item(
                    model,
                    item.image,
                    &params,
                    true,
                    model.info.native_batch != 0,
                )?)))
            } else if item.image.is_null() && item.tensor_count != 0 && !item.tensors.is_null() {
                Ok(Plan::Tensors(tensors::plan_item(
                    model,
                    item.tensor_count,
                    item.tensors,
                    &params,
                    model.info.native_batch != 0,
                )?))
            } else {
                Err(AM_ERR_SDK_INVALID_ARGUMENT)
            }
        })();
        match plan {
            Ok(p) => plans.push((i, p)),
            Err(e) => codes[i] = e,
        }
    }
    let groups = groups(model, &plans, native, codes)?;
    let deadline = started + Duration::from_millis(options.wait_deadline_ms as u64);
    let ctx = model.state.context()?;
    let context = ctx.data::<Context>()?;
    let queue = lock(&context.queues)?[AM_Q_OFFLINE_BATCH as usize];
    let mut waiter = None;
    for group in groups {
        let selected = group.iter().map(|i| &plans[*i].1).collect::<Vec<_>>();
        let result = loop {
            let result = if options.wait_deadline_ms != 0 && Instant::now() >= deadline {
                Err(AM_ERR_SDK_DEADLINE_EXCEEDED)
            } else {
                unsafe {
                    scheduler::submit_offline(
                        entry.clone(),
                        &selected,
                        native,
                        (options.wait_deadline_ms != 0).then_some(deadline),
                    )
                }
            };
            if result.as_ref().is_err_and(|e| *e == AM_ERR_SDK_QUEUE_FULL)
                && options.wait_deadline_ms != 0
            {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break Err(AM_ERR_SDK_DEADLINE_EXCEEDED);
                }
                if waiter.is_none() {
                    match scheduler::waiter(model.state.clone(), queue.max_waiters as usize) {
                        Ok(value) => waiter = Some(value),
                        Err(e) => break Err(e),
                    }
                }
                scheduler::wait(remaining)?;
                continue;
            }
            break result;
        };
        match result {
            Ok(accepted) => {
                for (index, id) in group.iter().zip(accepted) {
                    ids[plans[*index].0] = id;
                }
            }
            Err(e) => {
                for index in group {
                    codes[plans[index].0] = e;
                }
            }
        }
        if model.stopping.load(Ordering::Acquire) {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
    }
    Ok(())
}
fn groups(
    model: &ModelHandle,
    plans: &[(usize, Plan)],
    native: bool,
    codes: &mut [i32],
) -> Result<Vec<Vec<usize>>> {
    if !native {
        return Ok((0..plans.len()).map(|i| vec![i]).collect());
    }
    let mut buckets: Vec<(TensorSignature, Vec<usize>)> = Vec::new();
    for (index, (_, plan)) in plans.iter().enumerate() {
        let signature = plan.signature(&model.info.input_specs[0].name);
        if let Some((_, bucket)) = buckets.iter_mut().find(|(s, _)| *s == signature) {
            bucket.push(index);
        } else {
            buckets.push((signature, vec![index]));
        }
    }
    let mut result = Vec::new();
    for (_, bucket) in buckets {
        if model.info.native_batch >= 2 {
            for group in bucket.chunks(model.info.native_batch as usize) {
                if group.len() != model.info.native_batch as usize {
                    for i in group {
                        codes[plans[*i].0] = AM_ERR_VISION_INSUFFICIENT_BATCH_REJECTED;
                    }
                } else {
                    result.push(group.to_vec());
                }
            }
        } else {
            // 背压不能改变跨batch计算的结果；完整组容量不足只能拒绝或等待。
            result.push(bucket);
        }
    }
    Ok(result)
}
