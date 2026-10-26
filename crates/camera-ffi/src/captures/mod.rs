//! ISAPI抓图独立于业务任务；缓存、外持引用与关闭屏障分别计费。
use crate::{domain, frames::CameraState};
use am_sdk_support::{abi::*, args, clock::monotonic_ms, handles::Managed, status, Result};
use std::{
    any::Any,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};
mod continuous;
mod images;
mod io;
mod requests;
mod state;
pub(crate) mod tables;

pub(crate) fn queue_usage(ctx: usize) -> Result<(u64, u64)> {
    use am_sdk_support::context::Context;
    use std::any::TypeId;
    let entry = domain().get(ctx)?;
    let service = lock(&entry.data::<Context>()?.services)?
        .get(&TypeId::of::<state::State>())
        .cloned();
    let Some(service) = service else {
        return Ok((0, 0));
    };
    let state = service
        .downcast::<state::State>()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let (mut items, mut bytes) = {
        let mut records = lock(&state.records)?;
        state.purge(&mut records);
        records
            .values()
            .filter_map(|r| r.result.as_ref())
            .fold((0u64, 0u64), |(items, bytes), image| {
                (items.saturating_add(1), bytes.saturating_add(image.bytes()))
            })
    };
    // collector保留的最近原图属于数据缓存，HTTP在途与请求控制元数据不属于队列。
    {
        let recent = lock(&state.recent)?;
        items = items.saturating_add(recent.len() as u64);
        bytes = recent.values().fold(bytes, |sum, raw| {
            sum.saturating_add(raw.encoded.len() as u64)
        });
    }
    let subscriptions = {
        let mut subscriptions = lock(&state.subscriptions)?;
        subscriptions.retain(|_, sub| sub.strong_count() != 0);
        let mut values = subscriptions.values();
        // 当前连续采集硬限4；查询不用额外堆快照。
        std::array::from_fn::<_, 4, _>(|_| values.next().and_then(std::sync::Weak::upgrade))
    };
    for sub in subscriptions.into_iter().flatten() {
        let usage = sub.queue_usage()?;
        items = items.saturating_add(usage.0);
        bytes = bytes.saturating_add(usage.1);
    }
    Ok((items, bytes))
}

fn lock<T>(value: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    value.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)
}
fn kind(value: u32) -> Result<()> {
    if value > AM_CAP_CPU_FRAME {
        Err(AM_ERR_SDK_INVALID_ARGUMENT)
    } else {
        Ok(())
    }
}
fn deadline(value: u32) -> Result<Duration> {
    let value = if value == 0 {
        SDK_CAPTURE_TIMEOUT_MS as u32
    } else {
        value
    };
    if value as u64 > SDK_CAPTURE_TIMEOUT_MS {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    Ok(Duration::from_millis(value as u64))
}
